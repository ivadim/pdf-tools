//! Convert documents to grayscale while keeping text and vector graphics.
//!
//! Colours set in content streams (pages, forms, annotation appearances,
//! tiling patterns), indexed colour palettes and 8-bit images are converted.
//! Shadings (gradients) and spot colours keep their colour.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};

use crate::image_data::{color_components, decode_image, is_image, jpeg_bytes, mask_ids, plain_content};
use crate::merge::inherited_attribute;
use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GrayscaleSummary {
    /// Content streams rewritten.
    pub streams: usize,
    /// Images converted.
    pub images: usize,
    /// Content streams that couldn't be parsed and were left as they were.
    pub skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Space {
    Gray,
    Rgb,
    Cmyk,
    Other,
}

pub fn grayscale_document(doc: &mut Document, convert_images: bool) -> GrayscaleSummary {
    let mut summary = GrayscaleSummary::default();

    for (stream_id, resources) in content_streams(doc) {
        let spaces = named_spaces(doc, resources.as_ref());
        match convert_content(doc, stream_id, &spaces) {
            Some(true) => summary.streams += 1,
            Some(false) => {}
            None => summary.skipped += 1,
        }
    }

    convert_indexed_palettes(doc);

    if convert_images {
        let masks = mask_ids(doc);
        let images: Vec<ObjectId> = doc
            .objects
            .iter()
            .filter(|(id, object)| !masks.contains(id) && is_image(object))
            .map(|(id, _)| *id)
            .collect();
        for id in images {
            if let Some(stream) = gray_image(doc, id) {
                doc.objects.insert(id, Object::Stream(stream));
                summary.images += 1;
            }
        }
    }

    doc.compress();
    summary
}

pub fn grayscale_file(input: &Path, convert_images: bool, output: &Path) -> Result<GrayscaleSummary, Error> {
    let mut doc = crate::load(input)?;
    let summary = grayscale_document(&mut doc, convert_images);
    crate::save(&mut doc, output)?;
    Ok(summary)
}

/// Every content stream with the resources its names refer to.
fn content_streams(doc: &Document) -> Vec<(ObjectId, Option<Dictionary>)> {
    let resolve = |object: Option<&Object>| {
        object.and_then(|r| doc.dereference(r).ok()?.1.as_dict().ok().cloned())
    };
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();

    for page_id in doc.get_pages().into_values() {
        let resources = resolve(inherited_attribute(doc, page_id, b"Resources").as_ref());
        for id in doc.get_page_contents(page_id) {
            if seen.insert(id) {
                jobs.push((id, resources.clone()));
            }
        }
    }
    for (&id, object) in &doc.objects {
        let Ok(stream) = object.as_stream() else { continue };
        let is_form = stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Form");
        let is_tiling = stream.dict.get(b"PatternType").and_then(Object::as_i64).ok() == Some(1);
        if (is_form || is_tiling) && seen.insert(id) {
            jobs.push((id, resolve(stream.dict.get(b"Resources").ok())));
        }
    }
    jobs
}

/// Kinds of the colour spaces named in a resource dictionary.
fn named_spaces(doc: &Document, resources: Option<&Dictionary>) -> HashMap<Vec<u8>, Space> {
    let Some(spaces) = resources
        .and_then(|r| r.get(b"ColorSpace").ok())
        .and_then(|c| doc.dereference(c).ok()?.1.as_dict().ok())
    else {
        return HashMap::new();
    };
    spaces
        .iter()
        .map(|(name, space)| {
            let kind = match color_components(doc, space) {
                Some(1) => Space::Gray,
                Some(3) => Space::Rgb,
                Some(4) => Space::Cmyk,
                _ => Space::Other,
            };
            (name.clone(), kind)
        })
        .collect()
}

/// `Some(changed)`, or `None` if the stream couldn't be processed.
fn convert_content(doc: &mut Document, id: ObjectId, spaces: &HashMap<Vec<u8>, Space>) -> Option<bool> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let scanned = scan_content(&plain_content(stream)?)?;
    let mut content = Content::decode(&scanned.without_comments).ok()?;
    // The parser can stop early without reporting an error; rewriting a
    // partially parsed stream would silently drop the rest of the page.
    if content.operations.len() != scanned.operators {
        return None;
    }
    if !gray_operations(&mut content.operations, spaces) {
        return Some(false);
    }
    let encoded = content.encode().ok()?;
    doc.get_object_mut(id).ok()?.as_stream_mut().ok()?.set_plain_content(encoded);
    Some(true)
}

struct ScannedContent {
    /// The stream with comments replaced by spaces.
    without_comments: Vec<u8>,
    /// Number of operators in the stream.
    operators: usize,
}

/// Lexes a content stream just enough to remove comments and count
/// operators, so a parse can be checked for completeness. Returns `None`
/// for streams with inline images (binary data the lexer can't skip) or
/// unbalanced strings.
fn scan_content(data: &[u8]) -> Option<ScannedContent> {
    let is_white = |b: u8| matches!(b, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ');
    let is_delimiter = |b: u8| matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%');

    let mut out = Vec::with_capacity(data.len());
    let mut operators = 0;
    // Depth of arrays and dictionaries; tokens inside them are operands.
    let mut nesting = 0usize;
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        match b {
            b'%' => {
                while i < data.len() && data[i] != b'\n' && data[i] != b'\r' {
                    i += 1;
                }
                out.push(b' ');
                continue;
            }
            b'(' => {
                // Literal string: balanced parentheses, backslash escapes.
                let start = i;
                let mut depth = 0usize;
                loop {
                    let c = *data.get(i)?;
                    i += 1;
                    match c {
                        b'\\' => i += 1,
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                out.extend_from_slice(data.get(start..i)?);
                continue;
            }
            b'<' if data.get(i + 1) == Some(&b'<') => {
                nesting += 1;
                out.extend_from_slice(b"<<");
                i += 2;
                continue;
            }
            b'>' if data.get(i + 1) == Some(&b'>') => {
                nesting = nesting.saturating_sub(1);
                out.extend_from_slice(b">>");
                i += 2;
                continue;
            }
            b'<' => {
                let end = i + data[i..].iter().position(|&c| c == b'>')? + 1;
                out.extend_from_slice(&data[i..end]);
                i = end;
                continue;
            }
            b'[' => nesting += 1,
            b']' => nesting = nesting.saturating_sub(1),
            _ if is_white(b) || is_delimiter(b) => {}
            _ => {
                // A regular token: a number, keyword or operator.
                let start = i;
                while i < data.len() && !is_white(data[i]) && !is_delimiter(data[i]) {
                    i += 1;
                }
                let token = &data[start..i];
                let names_a_value = start > 0 && data[start - 1] == b'/';
                let is_operand = names_a_value
                    || matches!(token, b"true" | b"false" | b"null")
                    || matches!(token[0], b'0'..=b'9' | b'+' | b'-' | b'.');
                if !is_operand && nesting == 0 {
                    if token == b"BI" {
                        return None;
                    }
                    operators += 1;
                }
                out.extend_from_slice(token);
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    Some(ScannedContent { without_comments: out, operators })
}

/// Rewrites colour operators to their grey equivalents; returns whether
/// anything changed.
fn gray_operations(operations: &mut [Operation], spaces: &HashMap<Vec<u8>, Space>) -> bool {
    let mut changed = false;
    // Current (fill, stroke) colour spaces, saved and restored with q/Q.
    let mut current = (Space::Gray, Space::Gray);
    let mut saved = Vec::new();

    for op in operations.iter_mut() {
        let stroke = op.operator.chars().all(|c| c.is_ascii_uppercase());
        match op.operator.as_str() {
            "q" => saved.push(current),
            "Q" => current = saved.pop().unwrap_or((Space::Gray, Space::Gray)),
            "rg" | "RG" => {
                if let Some(level) = numbers::<3>(&op.operands).map(|[r, g, b]| rgb_luma(r, g, b)) {
                    *op = Operation::new(if stroke { "G" } else { "g" }, vec![Object::Real(level)]);
                    changed = true;
                }
                set_space(&mut current, stroke, Space::Gray);
            }
            "k" | "K" => {
                if let Some(level) = numbers::<4>(&op.operands).map(|[c, m, y, k]| cmyk_luma(c, m, y, k)) {
                    *op = Operation::new(if stroke { "G" } else { "g" }, vec![Object::Real(level)]);
                    changed = true;
                }
                set_space(&mut current, stroke, Space::Gray);
            }
            "g" | "G" => set_space(&mut current, stroke, Space::Gray),
            "cs" | "CS" => {
                let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) else { continue };
                let kind = match name {
                    b"DeviceGray" => Space::Gray,
                    b"DeviceRGB" => Space::Rgb,
                    b"DeviceCMYK" => Space::Cmyk,
                    other => spaces.get(other).copied().unwrap_or(Space::Other),
                };
                if matches!(kind, Space::Rgb | Space::Cmyk) {
                    op.operands[0] = Object::Name(b"DeviceGray".to_vec());
                    changed = true;
                }
                set_space(&mut current, stroke, kind);
            }
            "sc" | "scn" | "SC" | "SCN" => {
                let space = if stroke { current.1 } else { current.0 };
                let level = match space {
                    Space::Rgb => numbers::<3>(&op.operands).map(|[r, g, b]| rgb_luma(r, g, b)),
                    Space::Cmyk => numbers::<4>(&op.operands).map(|[c, m, y, k]| cmyk_luma(c, m, y, k)),
                    _ => None,
                };
                if let Some(level) = level {
                    op.operands = vec![Object::Real(level)];
                    changed = true;
                }
            }
            _ => {}
        }
    }
    changed
}

fn set_space(current: &mut (Space, Space), stroke: bool, space: Space) {
    if stroke {
        current.1 = space;
    } else {
        current.0 = space;
    }
}

fn numbers<const N: usize>(operands: &[Object]) -> Option<[f32; N]> {
    if operands.len() != N {
        return None;
    }
    let mut values = [0.0; N];
    for (value, operand) in values.iter_mut().zip(operands) {
        *value = operand.as_float().ok()?;
    }
    Some(values)
}

fn rgb_luma(r: f32, g: f32, b: f32) -> f32 {
    (0.299 * r + 0.587 * g + 0.114 * b).clamp(0.0, 1.0)
}

/// Rec. 601 luma of 8-bit RGB, matching [`rgb_luma`].
fn luma_u8([r, g, b]: [u8; 3]) -> u8 {
    (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32).round() as u8
}

fn cmyk_luma(c: f32, m: f32, y: f32, k: f32) -> f32 {
    rgb_luma((1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k))
}

/// Replaces `[/Indexed <rgb or cmyk base> hival lookup]` colour spaces with
/// grey palettes, wherever they appear.
fn convert_indexed_palettes(doc: &mut Document) {
    let mut replacements = Vec::new();
    for (&id, object) in &doc.objects {
        let mut copy = object.clone();
        if visit_indexed(doc, &mut copy) {
            replacements.push((id, copy));
        }
    }
    for (id, object) in replacements {
        doc.objects.insert(id, object);
    }
}

fn visit_indexed(doc: &Document, object: &mut Object) -> bool {
    match object {
        Object::Array(items) => {
            if let Some(gray) = gray_palette(doc, items) {
                *items = gray;
                return true;
            }
            items.iter_mut().fold(false, |changed, item| visit_indexed(doc, item) | changed)
        }
        Object::Dictionary(dict) => dict.iter_mut().fold(false, |changed, (_, v)| visit_indexed(doc, v) | changed),
        Object::Stream(stream) => {
            stream.dict.iter_mut().fold(false, |changed, (_, v)| visit_indexed(doc, v) | changed)
        }
        _ => false,
    }
}

fn gray_palette(doc: &Document, items: &[Object]) -> Option<Vec<Object>> {
    if items.len() != 4 || items[0].as_name().ok()? != b"Indexed" {
        return None;
    }
    let components = color_components(doc, &items[1])?;
    if components != 3 && components != 4 {
        return None;
    }
    let hival = items[2].as_i64().ok()?;
    let lookup = match doc.dereference(&items[3]).ok()?.1 {
        Object::String(bytes, _) => bytes.clone(),
        Object::Stream(stream) => plain_content(stream)?,
        _ => return None,
    };
    let entries = usize::try_from(hival + 1).ok()?;
    let width = components as usize;
    if lookup.len() < entries * width {
        return None;
    }
    let gray = lookup[..entries * width]
        .chunks_exact(width)
        .map(|c| {
            let rgb = if width == 4 { crate::image_data::cmyk_to_rgb(c[0], c[1], c[2], c[3]) } else { [c[0], c[1], c[2]] };
            luma_u8(rgb)
        })
        .collect();
    Some(vec![
        Object::Name(b"Indexed".to_vec()),
        Object::Name(b"DeviceGray".to_vec()),
        Object::Integer(hival),
        Object::String(gray, StringFormat::Hexadecimal),
    ])
}

fn gray_image(doc: &Document, id: ObjectId) -> Option<Stream> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let decoded = decode_image(doc, stream)?;
    if decoded.image.color().channel_count() == 1 {
        return None;
    }
    // Same weights as for vector colours (image's own to_luma8 uses Rec. 709).
    let rgb = decoded.image.to_rgb8();
    let (width, height) = rgb.dimensions();
    let pixels = rgb.pixels().map(|p| luma_u8(p.0)).collect();
    let gray = image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(width, height, pixels)?);

    let mut dict = stream.dict.clone();
    dict.set("ColorSpace", "DeviceGray");
    dict.set("BitsPerComponent", 8);
    dict.remove(b"DecodeParms");
    if decoded.was_jpeg {
        dict.set("Filter", "DCTDecode");
        Some(Stream::new(dict, jpeg_bytes(&gray, 88)?).with_compression(false))
    } else {
        dict.remove(b"Filter");
        let mut stream = Stream::new(dict, gray.into_bytes());
        let _ = stream.compress();
        Some(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{page_labels, round_trip, sample};
    use lopdf::dictionary;

    fn ops(source: &str, spaces: &[(&str, Space)]) -> String {
        let mut content = Content::decode(source.as_bytes()).unwrap();
        let spaces = spaces.iter().map(|(n, s)| (n.as_bytes().to_vec(), *s)).collect();
        gray_operations(&mut content.operations, &spaces);
        String::from_utf8(content.encode().unwrap()).unwrap().replace('\n', " ")
    }

    #[test]
    fn converts_device_colours() {
        assert_eq!(ops("1 0 0 rg 0 0 1 RG", &[]), "0.299 g 0.114 G");
        assert_eq!(ops("0 0 0 1 k", &[]), "0 g");
    }

    #[test]
    fn converts_colours_set_through_colour_spaces() {
        assert_eq!(ops("/DeviceRGB cs 0 1 0 sc", &[]), "/DeviceGray cs 0.587 sc");
        assert_eq!(ops("/CS0 CS 1 1 1 SCN", &[("CS0", Space::Rgb)]), "/DeviceGray CS 1 SCN");
        // Unknown spaces (spot colours, patterns) are left alone.
        assert_eq!(ops("/P0 cs /Pat1 scn", &[]), "/P0 cs /Pat1 scn");
    }

    #[test]
    fn colour_space_state_follows_save_and_restore() {
        assert_eq!(ops("q /DeviceRGB cs Q 0.5 sc", &[]), "q /DeviceGray cs Q 0.5 sc");
    }

    #[test]
    fn converts_page_content_and_images() {
        let mut doc = sample("a", 1);
        let page_id = doc.get_pages()[&1];
        let red = doc.add_object(Stream::new(dictionary! {}, b"1 0 0 rg 0 0 10 10 re f".to_vec()));
        let mut contents = doc.get_dictionary(page_id).unwrap().get(b"Contents").unwrap().clone();
        if let Object::Reference(first) = contents {
            contents = Object::Array(vec![Object::Reference(first), Object::Reference(red)]);
        }
        doc.get_dictionary_mut(page_id).unwrap().set("Contents", contents);
        let image = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            },
            vec![255, 0, 0, 0, 0, 255],
        ));
        doc.trailer.set("KeepImage", Object::Reference(image));

        let summary = grayscale_document(&mut doc, true);
        assert_eq!(summary, GrayscaleSummary { streams: 1, images: 1, skipped: 0 });
        let doc = round_trip(doc);
        let content = String::from_utf8(doc.get_page_content(page_id).unwrap()).unwrap();
        assert!(content.contains("0.299 g"), "{content}");
        assert_eq!(page_labels(&doc), ["a-1"]);

        let image = doc.get_object(image).unwrap().as_stream().unwrap();
        assert_eq!(image.dict.get(b"ColorSpace").unwrap().as_name().unwrap(), b"DeviceGray");
        assert_eq!(plain_content(image).unwrap(), vec![76, 29]);
    }

    #[test]
    fn scanning_strips_comments_but_not_percent_signs_in_strings() {
        let scanned = scan_content(b"1 0 0 rg % red\n(50% off) Tj % note\n<</A [1 2]>> BDC EMC").unwrap();
        assert_eq!(String::from_utf8_lossy(&scanned.without_comments), "1 0 0 rg  \n(50% off) Tj  \n<</A [1 2]>> BDC EMC");
        assert_eq!(scanned.operators, 4);
        assert!(scan_content(b"q BI /W 1 ID \xff EI Q").is_none(), "inline images are skipped");
        assert!(scan_content(b"(unterminated Tj").is_none());
    }

    #[test]
    fn commented_content_is_converted_completely() {
        let mut doc = sample("a", 1);
        let page_id = doc.get_pages()[&1];
        let content = b"1 0 0 rg %\n 0 0 10 10 re %\n f %\nBT /F1 12 Tf (still here) Tj ET".to_vec();
        let id = doc.add_object(Stream::new(dictionary! {}, content));
        doc.get_dictionary_mut(page_id).unwrap().set("Contents", Object::Reference(id));

        let summary = grayscale_document(&mut doc, false);
        assert_eq!(summary.skipped, 0);
        let content = String::from_utf8(doc.get_page_content(page_id).unwrap()).unwrap();
        assert!(content.contains("0.299 g") && content.contains("(still here) Tj"), "{content}");
    }

    #[test]
    fn streams_the_parser_cannot_fully_read_are_left_alone() {
        let mut doc = sample("a", 1);
        let page_id = doc.get_pages()[&1];
        // `'` is a valid operator the parser may not handle; whatever happens,
        // the content must never be truncated.
        let original = b"1 0 0 rg 0 0 10 10 re f BT /F1 12 Tf (x) ' ET 0 1 0 rg".to_vec();
        let id = doc.add_object(Stream::new(dictionary! {}, original.clone()));
        doc.get_dictionary_mut(page_id).unwrap().set("Contents", Object::Reference(id));

        grayscale_document(&mut doc, false);
        let content = String::from_utf8(doc.get_page_content(page_id).unwrap()).unwrap();
        assert!(content.contains("(x)") && content.contains("ET"), "content was truncated: {content}");
    }

    #[test]
    fn converts_indexed_palettes() {
        let mut doc = sample("a", 1);
        let space = doc.add_object(Object::Array(vec![
            Object::Name(b"Indexed".to_vec()),
            Object::Name(b"DeviceRGB".to_vec()),
            Object::Integer(1),
            Object::String(vec![255, 255, 255, 0, 0, 255], StringFormat::Hexadecimal),
        ]));
        convert_indexed_palettes(&mut doc);
        let items = doc.get_object(space).unwrap().as_array().unwrap();
        assert_eq!(items[1].as_name().unwrap(), b"DeviceGray");
        assert_eq!(items[3].as_str().unwrap(), &[255, 29]);
    }
}
