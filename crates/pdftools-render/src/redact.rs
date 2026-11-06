//! Redaction by rendering into a new document, with pixels replaced before
//! they are embedded. No objects from the original PDF are copied.

use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};

use crate::pages::{render, DocCache, PreviewRequest};

/// A rectangle in displayed page coordinates, fractions from the top left.
#[derive(Debug, Clone, Copy)]
pub struct Redaction {
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Quality of the image-only output. Both choices remove the same data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RedactQuality {
    #[default]
    Higher,
    Smaller,
}

/// Creates a new, image-only PDF. All pages are flattened to remove document
/// metadata, hidden objects, annotations, forms, attachments and old revisions.
pub fn redact(input: &Path, output: &Path, marks: &[Redaction]) -> Result<usize, String> {
    redact_with_quality(input, output, marks, RedactQuality::Higher)
}

/// Like [`redact`], with a choice between lossless and smaller page images.
pub fn redact_with_quality(input: &Path, output: &Path, marks: &[Redaction], quality: RedactQuality) -> Result<usize, String> {
    let count = pdftools_core::page_count(input).map_err(|e| e.to_string())?;
    if count == 0 || marks.is_empty() {
        return Err("Select at least one area to redact".into());
    }
    for mark in marks {
        let values = [mark.x, mark.y, mark.width, mark.height];
        if mark.page == 0
            || mark.page > count
            || values.iter().any(|v| !v.is_finite())
            || mark.x < 0.0
            || mark.y < 0.0
            || mark.width <= 0.0
            || mark.height <= 0.0
            || mark.x + mark.width > 1.000_001
            || mark.y + mark.height > 1.000_001
        {
            return Err("Invalid redaction area".into());
        }
    }
    // Never intentionally replace the original, even if it was selected in a
    // save dialog. Compare canonical paths when both files already exist.
    if input == output || (input.canonicalize().ok().zip(output.canonicalize().ok()).is_some_and(|(a, b)| a == b)) {
        return Err("Choose a different file for the redacted copy".into());
    }
    let cache = DocCache::new();
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::with_capacity(count);
    for page in 1..=count {
        let width = match quality {
            RedactQuality::Higher => 2500,
            RedactQuality::Smaller => 1400,
        };
        let request = PreviewRequest { path: input.to_owned(), page, width, rotation: 0, gray: false };
        let mut image = render(&request, &cache)?;
        for mark in marks.iter().filter(|m| m.page == page) {
            black_out(&mut image.pixels, image.width, image.height, mark);
        }
        // Only opaque RGB pixels enter the output; there is no source image or
        // overlay underneath the redaction.
        let stream = page_image(&image.pixels, image.width, image.height, quality)?;
        let image_id = doc.add_object(stream);
        let (w, h) = image.page_size;
        let mut operations = vec![
            Operation::new("q", vec![]),
            Operation::new("cm", vec![w.into(), 0.into(), 0.into(), h.into(), 0.into(), 0.into()]),
            Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]),
            Operation::new("Q", vec![]),
        ];
        // JPEG is made from blackened pixels, but can soften black edges.
        // Draw an opaque box as well, using exactly the blackened pixel bounds.
        operations.push(Operation::new("q", vec![]));
        operations.push(Operation::new("rg", vec![0.into(), 0.into(), 0.into()]));
        for mark in marks.iter().filter(|m| m.page == page) {
            let (left, top, right, bottom) = pixel_bounds(image.width, image.height, mark);
            let x = left as f32 / image.width as f32 * w;
            let y = (image.height - bottom) as f32 / image.height as f32 * h;
            let width = (right - left) as f32 / image.width as f32 * w;
            let height = (bottom - top) as f32 / image.height as f32 * h;
            operations.push(Operation::new("re", vec![x.into(), y.into(), width.into(), height.into()]));
            operations.push(Operation::new("f", vec![]));
        }
        operations.push(Operation::new("Q", vec![]));
        let content = Content { operations };
        let contents = doc.add_object(Stream::new(dictionary! {}, content.encode().map_err(|e| e.to_string())?));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), w.into(), h.into()],
            "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => image_id } },
            "Contents" => contents,
        });
        kids.push(Object::Reference(page_id));
    }
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => count as i64, "Kids" => kids }));
    let root = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", root);
    pdftools_core::save(&mut doc, output).map_err(|e| e.to_string())?;
    Ok(count)
}

/// Encode only the already-redacted pixels. Monochrome pages stay grayscale;
/// PNG's row predictors make text pages smaller without changing pixels.
fn page_image(rgba: &[u8], width: u32, height: u32, quality: RedactQuality) -> Result<Stream, String> {
    let pixels = rgba.as_chunks::<4>().0;
    let gray = pixels.iter().all(|p| p[0] == p[1] && p[1] == p[2]);
    let samples: Vec<u8> =
        if gray { pixels.iter().map(|p| p[0]).collect() } else { pixels.iter().flat_map(|p| [p[0], p[1], p[2]]).collect() };
    let (space, colors, color_type) =
        if gray { ("DeviceGray", 1, ExtendedColorType::L8) } else { ("DeviceRGB", 3, ExtendedColorType::Rgb8) };
    let mut dict = dictionary! {
        "Type" => "XObject", "Subtype" => "Image", "Width" => i64::from(width),
        "Height" => i64::from(height), "ColorSpace" => space, "BitsPerComponent" => 8,
    };
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Default, FilterType::Adaptive)
        .write_image(&samples, width, height, color_type)
        .map_err(|e| e.to_string())?;
    let compressed = png_image_data(&png)?;
    if quality == RedactQuality::Smaller {
        let mut encoded = Vec::new();
        JpegEncoder::new_with_quality(&mut encoded, 75).encode(&samples, width, height, color_type).map_err(|e| e.to_string())?;
        if encoded.len() < compressed.len() {
            dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
            return Ok(Stream::new(dict, encoded).with_compression(false));
        }
    }
    dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
    dict.set("DecodeParms", dictionary! { "Predictor" => 15, "Colors" => colors, "BitsPerComponent" => 8, "Columns" => i64::from(width) });
    Ok(Stream::new(dict, compressed).with_compression(false))
}

/// PNG IDAT chunks contain the zlib stream used by PDF's PNG predictor.
fn png_image_data(png: &[u8]) -> Result<Vec<u8>, String> {
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Unable to encode page image".into());
    }
    let mut pos = 8usize;
    let mut data = Vec::new();
    while pos.checked_add(12).is_some_and(|end| end <= png.len()) {
        let length = u32::from_be_bytes(png[pos..pos + 4].try_into().map_err(|_| "Invalid page image")?) as usize;
        let end = pos.checked_add(12).and_then(|n| n.checked_add(length)).filter(|&n| n <= png.len()).ok_or("Invalid page image")?;
        if &png[pos + 4..pos + 8] == b"IDAT" {
            data.extend_from_slice(&png[pos + 8..end - 4]);
        }
        if &png[pos + 4..pos + 8] == b"IEND" {
            break;
        }
        pos = end;
    }
    if data.is_empty() {
        Err("Unable to encode page image".into())
    } else {
        Ok(data)
    }
}

fn black_out(pixels: &mut [u8], width: u32, height: u32, mark: &Redaction) {
    // Expand to whole pixels so antialiased edge pixels never reveal content.
    let (left, top, right, bottom) = pixel_bounds(width, height, mark);
    for y in top..bottom {
        for x in left..right {
            let i = ((y * width + x) * 4) as usize;
            pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        }
    }
}

fn pixel_bounds(width: u32, height: u32, mark: &Redaction) -> (u32, u32, u32, u32) {
    let left = ((mark.x * width as f32).floor() as u32).saturating_sub(1);
    let top = ((mark.y * height as f32).floor() as u32).saturating_sub(1);
    let right = (((mark.x + mark.width) * width as f32).ceil() as u32).saturating_add(1).min(width);
    let bottom = (((mark.y + mark.height) * height as f32).ceil() as u32).saturating_add(1).min(height);
    (left, top, right, bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_contains_only_redacted_page_pixels() {
        let folder = std::env::temp_dir().join(format!("pdftools-redact-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let input = folder.join("input.pdf");
        let mut source = Document::with_version("1.7");
        let parent = source.new_object_id();
        let contents = source
            .add_object(Stream::new(dictionary! {}, b"1 0 0 rg 0 0 100 100 re f BT /F1 10 Tf 25 50 Td (SECRET-IN-CONTENT) Tj ET".to_vec()));
        let font = source.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" });
        let annotation = source.add_object(
            dictionary! { "Type" => "Annot", "Subtype" => "Text", "Contents" => Object::string_literal("SECRET-IN-ANNOTATION") },
        );
        let page = source.add_object(dictionary! { "Type" => "Page", "Parent" => parent,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } }, "Contents" => contents, "Annots" => vec![annotation.into()] });
        source.objects.insert(parent, Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => 1, "Kids" => vec![page.into()] }));
        let attachment = source.add_object(Stream::new(dictionary! { "Type" => "EmbeddedFile" }, b"SECRET-IN-ATTACHMENT".to_vec()));
        let file_spec = source.add_object(dictionary! {
            "Type" => "Filespec", "F" => Object::string_literal("secret.txt"),
            "EF" => dictionary! { "F" => attachment }
        });
        let root = source.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => parent,
            "Names" => dictionary! { "EmbeddedFiles" => dictionary! { "Names" => vec![Object::string_literal("secret.txt"), file_spec.into()] } }
        });
        let info = source.add_object(dictionary! { "Title" => Object::string_literal("SECRET-IN-METADATA") });
        source.trailer.set("Root", root);
        source.trailer.set("Info", info);
        source.save(&input).unwrap();
        let mark = Redaction { page: 1, x: 0.25, y: 0.25, width: 0.5, height: 0.5 };
        for quality in [RedactQuality::Higher, RedactQuality::Smaller] {
            let output = folder.join(format!("{quality:?}.pdf"));
            assert_eq!(redact_with_quality(&input, &output, &[mark], quality).unwrap(), 1);
            let bytes = std::fs::read(&output).unwrap();
            assert!(!bytes.windows(6).any(|w| w == b"SECRET"));
            let result = Document::load(&output).unwrap();
            assert!(result.trailer.get(b"Info").is_err());
            for object in result.objects.values() {
                if let Ok(stream) = object.as_stream() {
                    let content = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
                    assert!(!content.windows(6).any(|w| w == b"SECRET"));
                    if stream.dict.get(b"Filter").and_then(Object::as_name).ok() == Some(b"DCTDecode") {
                        let image = image::load_from_memory(&stream.content).unwrap().to_rgb8();
                        let pixel = image.get_pixel(image.width() / 2, image.height() / 2).0;
                        assert!(pixel.iter().all(|&v| v < 10), "the encoded image itself must be redacted: {pixel:?}");
                    }
                }
            }
            let rendered =
                render(&PreviewRequest { path: output.clone(), page: 1, width: 100, rotation: 0, gray: false }, &DocCache::new()).unwrap();
            let pixel = |x: usize, y: usize| &rendered.pixels[(y * rendered.width as usize + x) * 4..][..3];
            assert_eq!(pixel(50, 50), &[0, 0, 0]);
            let red = pixel(10, 10);
            assert!(red[0] > 245 && red[1] < 10 && red[2] < 10, "unmarked color remains red: {red:?}");
            assert!(redact_with_quality(&input, &input, &[mark], quality).is_err());
            assert!(redact_with_quality(&input, &output, &[Redaction { x: f32::NAN, ..mark }], quality).is_err());
        }
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn higher_quality_preserves_gray_and_color_pixels_losslessly() {
        for (rgba, expected_space, expected) in [
            (vec![0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255], b"DeviceGray".as_slice(), vec![0, 128, 255]),
            (vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255], b"DeviceRGB".as_slice(), vec![255, 0, 0, 0, 255, 0, 0, 0, 255]),
        ] {
            let stream = page_image(&rgba, 3, 1, RedactQuality::Higher).unwrap();
            assert_eq!(stream.dict.get(b"ColorSpace").unwrap().as_name().unwrap(), expected_space);
            assert_eq!(stream.decompressed_content().unwrap(), expected);
        }
    }

    #[test]
    fn smaller_mode_selects_the_smaller_encoding_per_page() {
        let plain = vec![255; 128 * 128 * 4];
        let lossless = page_image(&plain, 128, 128, RedactQuality::Higher).unwrap();
        let smaller = page_image(&plain, 128, 128, RedactQuality::Smaller).unwrap();
        assert_eq!(smaller.dict.get(b"Filter").unwrap().as_name().unwrap(), b"FlateDecode");
        assert_eq!(smaller.content, lossless.content);

        let mut state = 123456u32;
        let mut noisy: Vec<u8> = (0..128 * 128)
            .flat_map(|_| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                [state as u8, (state >> 8) as u8, (state >> 16) as u8, 255]
            })
            .collect();
        let mark = Redaction { page: 1, x: 0.25, y: 0.25, width: 0.5, height: 0.5 };
        black_out(&mut noisy, 128, 128, &mark);
        let smaller = page_image(&noisy, 128, 128, RedactQuality::Smaller).unwrap();
        let lossless = page_image(&noisy, 128, 128, RedactQuality::Higher).unwrap();
        assert_eq!(smaller.dict.get(b"Filter").unwrap().as_name().unwrap(), b"DCTDecode");
        assert!(smaller.content.len() < lossless.content.len());
        let decoded = image::load_from_memory(&smaller.content).unwrap().to_rgb8();
        assert!(decoded.get_pixel(64, 64).0.iter().all(|&v| v < 10), "pixels are removed before JPEG encoding");
    }

    #[test]
    fn marks_follow_the_displayed_rotation() {
        let repo = if std::env::var_os("TEST_SRCDIR").is_some() {
            std::path::PathBuf::new()
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
        };
        let input = repo.join("tests/fixtures/pdfjs/rotation.pdf");
        let output = std::env::temp_dir().join(format!("pdftools-redact-rotated-{}.pdf", std::process::id()));
        let mark = Redaction { page: 2, x: 0.1, y: 0.1, width: 0.3, height: 0.2 };
        for quality in [RedactQuality::Higher, RedactQuality::Smaller] {
            assert_eq!(redact_with_quality(&input, &output, &[mark], quality).unwrap(), 2);
            let cache = DocCache::new();
            let request = |path: &Path| PreviewRequest { path: path.to_owned(), page: 2, width: 200, rotation: 0, gray: false };
            let original = render(&request(&input), &cache).unwrap();
            let result = render(&request(&output), &cache).unwrap();
            assert_eq!(original.page_size, result.page_size);
            let i = ((result.height as f32 * 0.2) as usize * result.width as usize + (result.width as f32 * 0.2) as usize) * 4;
            assert_eq!(&result.pixels[i..i + 3], &[0, 0, 0]);
        }
        std::fs::remove_file(output).unwrap();
    }
}
