//! Page numbers, image watermarks and signatures.

use std::path::Path;

use lopdf::content::Operation;
use lopdf::{dictionary, Document, Object, StringFormat};

use crate::image_data::embed_image;
use crate::stamp::{add_overlay, cm, page_frame, selected_pages, Resource};
use crate::{Anchor, Error};

// ── Page numbers ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct PageNumbers {
    pub anchor: Anchor,
    /// Distance from the page edges, in points.
    pub margin: f32,
    pub font_size: f32,
    /// Text to draw; `{n}` is the page's number and `{total}` the last number.
    pub format: String,
    /// Number given to the first numbered page.
    pub start_at: i64,
    /// 1-based pages to number; `None` numbers every page.
    pub pages: Option<Vec<usize>>,
    /// RGB, each 0..=1.
    pub color: [f32; 3],
}

/// Returns the number of pages numbered.
pub fn add_page_numbers(doc: &mut Document, options: &PageNumbers) -> Result<usize, Error> {
    if !(options.font_size > 0.0) {
        return Err(Error::InvalidOption("font size must be positive".into()));
    }
    // Fail before changing anything if the text can't be encoded.
    win_ansi(&options.format.replace("{n}", "0").replace("{total}", "0"))?;

    let pages = selected_pages(doc, options.pages.as_deref())?;
    let font = Object::Reference(doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }));
    let last = options.start_at + pages.len() as i64 - 1;
    let [r, g, b] = options.color;

    for (index, (_, page_id)) in pages.iter().enumerate() {
        let number = options.start_at + index as i64;
        let text = options.format.replace("{n}", &number.to_string()).replace("{total}", &last.to_string());
        let bytes = win_ansi(&text)?;
        let size = options.font_size;
        let width = text_width(&bytes) * size / 1000.0;
        // Approximate the glyph box by the cap height so text centres nicely.
        let height = size * 0.72;
        let frame = page_frame(doc, *page_id);
        let (cx, cy) = options.anchor.place(frame.width, frame.height, width, height, options.margin);

        let resources = [Resource { category: b"Font", prefix: "PdfToolsNum", object: font.clone() }];
        add_overlay(doc, *page_id, &resources, false, |names| {
            vec![
                frame.cm(),
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec![Object::Name(names[0].clone()), Object::Real(size)]),
                Operation::new("rg", vec![Object::Real(r), Object::Real(g), Object::Real(b)]),
                Operation::new("Td", vec![Object::Real(cx - width / 2.0), Object::Real(cy - height / 2.0)]),
                Operation::new("Tj", vec![Object::String(bytes, StringFormat::Literal)]),
                Operation::new("ET", vec![]),
            ]
        })?;
    }
    Ok(pages.len())
}

pub fn add_page_numbers_file(input: &Path, options: &PageNumbers, output: &Path) -> Result<usize, Error> {
    let mut doc = crate::load(input)?;
    let count = add_page_numbers(&mut doc, options)?;
    crate::save(&mut doc, output)?;
    Ok(count)
}

// ── Image stamps (watermarks) ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ImageStamp {
    /// PNG or JPEG data.
    pub image: Vec<u8>,
    pub anchor: Anchor,
    /// Distance from the edges as a fraction of the page's shorter side.
    pub margin: f32,
    /// Width of the image as a fraction of the page width.
    pub width: f32,
    /// Counter-clockwise, in degrees.
    pub rotation: f32,
    /// 0 (invisible) to 1 (opaque).
    pub opacity: f32,
    /// Draw beneath the page content instead of on top.
    pub under: bool,
    /// 1-based pages to stamp; `None` stamps every page.
    pub pages: Option<Vec<usize>>,
}

/// Returns the number of pages stamped.
pub fn add_image_stamp(doc: &mut Document, stamp: &ImageStamp) -> Result<usize, Error> {
    if !(stamp.width > 0.0) {
        return Err(Error::InvalidOption("watermark width must be positive".into()));
    }
    let pages = selected_pages(doc, stamp.pages.as_deref())?;
    let (image_id, pixel_w, pixel_h) = embed_image(doc, &stamp.image)?;
    let opacity = stamp.opacity.clamp(0.0, 1.0);
    let state = doc.add_object(dictionary! { "Type" => "ExtGState", "ca" => opacity, "CA" => opacity });
    let aspect = pixel_h as f32 / pixel_w as f32;
    let (sin, cos) = stamp.rotation.to_radians().sin_cos();

    for (_, page_id) in &pages {
        let frame = page_frame(doc, *page_id);
        let w = stamp.width * frame.width;
        let h = w * aspect;
        // Anchor the rotated image's bounding box, so corners stay inside.
        let bound_w = (w * cos).abs() + (h * sin).abs();
        let bound_h = (w * sin).abs() + (h * cos).abs();
        let margin = stamp.margin * frame.width.min(frame.height);
        let (cx, cy) = stamp.anchor.place(frame.width, frame.height, bound_w, bound_h, margin);

        let resources = [
            Resource { category: b"XObject", prefix: "PdfToolsWm", object: Object::Reference(image_id) },
            Resource { category: b"ExtGState", prefix: "PdfToolsGs", object: Object::Reference(state) },
        ];
        add_overlay(doc, *page_id, &resources, stamp.under, |names| {
            vec![
                frame.cm(),
                Operation::new("gs", vec![Object::Name(names[1].clone())]),
                cm([1.0, 0.0, 0.0, 1.0, cx, cy]),
                cm([cos, sin, -sin, cos, 0.0, 0.0]),
                cm([w, 0.0, 0.0, h, -w / 2.0, -h / 2.0]),
                Operation::new("Do", vec![Object::Name(names[0].clone())]),
            ]
        })?;
    }
    Ok(pages.len())
}

pub fn add_image_stamp_file(input: &Path, stamp: &ImageStamp, output: &Path) -> Result<usize, Error> {
    let mut doc = crate::load(input)?;
    let count = add_image_stamp(&mut doc, stamp)?;
    crate::save(&mut doc, output)?;
    Ok(count)
}

// ── Signatures ───────────────────────────────────────────────────────────

/// Where to put a signature, as fractions of the displayed page with the
/// origin at its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// 1-based.
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Draws the signature image (PNG or JPEG) at each placement.
pub fn add_signature(doc: &mut Document, image: &[u8], placements: &[Placement]) -> Result<usize, Error> {
    if placements.is_empty() {
        return Err(Error::InvalidOption("place the signature on at least one page".into()));
    }
    let pages = doc.get_pages();
    for placement in placements {
        if !pages.contains_key(&(placement.page as u32)) {
            return Err(Error::InvalidPage { input: 0, page: placement.page });
        }
    }
    let (image_id, _, _) = embed_image(doc, image)?;

    for placement in placements {
        let page_id = pages[&(placement.page as u32)];
        let frame = page_frame(doc, page_id);
        let w = placement.width * frame.width;
        let h = placement.height * frame.height;
        let x = placement.x * frame.width;
        let y = frame.height - (placement.y + placement.height) * frame.height;
        let resources = [Resource { category: b"XObject", prefix: "PdfToolsSig", object: Object::Reference(image_id) }];
        add_overlay(doc, page_id, &resources, false, |names| {
            vec![
                frame.cm(),
                cm([w, 0.0, 0.0, h, x, y]),
                Operation::new("Do", vec![Object::Name(names[0].clone())]),
            ]
        })?;
    }
    Ok(placements.len())
}

pub fn add_signature_file(input: &Path, image: &[u8], placements: &[Placement], output: &Path) -> Result<usize, Error> {
    let mut doc = crate::load(input)?;
    let count = add_signature(&mut doc, image, placements)?;
    crate::save(&mut doc, output)?;
    Ok(count)
}

// ── Built-in font support ────────────────────────────────────────────────

/// Encodes text for a standard font with WinAnsiEncoding (Windows-1252).
fn win_ansi(text: &str) -> Result<Vec<u8>, Error> {
    text.chars()
        .map(|c| match c {
            ' '..='~' | '\u{a0}'..='\u{ff}' => Some(c as u8),
            _ => CP1252_EXTRAS.iter().find(|(ch, _)| *ch == c).map(|(_, b)| *b),
        })
        .collect::<Option<Vec<u8>>>()
        .ok_or_else(|| {
            let bad = text.chars().find(|&c| !(' '..='~').contains(&c) && !('\u{a0}'..='\u{ff}').contains(&c) && !CP1252_EXTRAS.iter().any(|(ch, _)| *ch == c));
            Error::InvalidOption(format!(
                "“{}” can’t be drawn with the built-in font; use Latin characters",
                bad.unwrap_or('?')
            ))
        })
}

const CP1252_EXTRAS: [(char, u8); 27] = [
    ('€', 0x80), ('‚', 0x82), ('ƒ', 0x83), ('„', 0x84), ('…', 0x85), ('†', 0x86), ('‡', 0x87),
    ('ˆ', 0x88), ('‰', 0x89), ('Š', 0x8A), ('‹', 0x8B), ('Œ', 0x8C), ('Ž', 0x8E), ('‘', 0x91),
    ('’', 0x92), ('“', 0x93), ('”', 0x94), ('•', 0x95), ('–', 0x96), ('—', 0x97), ('˜', 0x98),
    ('™', 0x99), ('š', 0x9A), ('›', 0x9B), ('œ', 0x9C), ('ž', 0x9E), ('Ÿ', 0x9F),
];

/// Helvetica advance widths (1/1000 em) for characters 32..=126.
const HELVETICA_WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // space../
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0..9
    278, 278, 584, 584, 584, 556, 1015, // :..@
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, // A..M
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // N..Z
    278, 278, 278, 469, 556, 333, // [..`
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, // a..m
    556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // n..z
    334, 260, 334, 584, // {..~
];

fn text_width(bytes: &[u8]) -> f32 {
    bytes
        .iter()
        .map(|&b| match b {
            32..=126 => HELVETICA_WIDTHS[(b - 32) as usize] as f32,
            _ => 556.0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{page_labels, round_trip, sample};
    use crate::{HAlign, VAlign};
    use image::{ImageFormat, Rgba, RgbaImage};

    fn content(doc: &Document, page: u32) -> String {
        String::from_utf8_lossy(&doc.get_page_content(doc.get_pages()[&page]).unwrap()).into_owned()
    }

    fn png() -> Vec<u8> {
        let image = RgbaImage::from_fn(40, 20, |x, _| Rgba([200, 0, 0, if x < 20 { 255 } else { 0 }]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    fn numbers(format: &str) -> PageNumbers {
        PageNumbers {
            anchor: Anchor::new(HAlign::Center, VAlign::Bottom),
            margin: 24.0,
            font_size: 10.0,
            format: format.into(),
            start_at: 1,
            pages: None,
            color: [0.0, 0.0, 0.0],
        }
    }

    #[test]
    fn numbers_every_page() {
        let mut doc = sample("a", 3);
        assert_eq!(add_page_numbers(&mut doc, &numbers("Page {n} of {total}")).unwrap(), 3);
        let doc = round_trip(doc);
        assert!(content(&doc, 1).contains("(Page 1 of 3)"));
        assert!(content(&doc, 3).contains("(Page 3 of 3)"));
        // The original content is still there.
        assert!(content(&doc, 2).contains("(a-2)"));
    }

    #[test]
    fn numbers_selected_pages_from_a_start_value() {
        let mut doc = sample("a", 4);
        let options = PageNumbers { start_at: 5, pages: Some(vec![2, 3, 4]), ..numbers("{n}") };
        add_page_numbers(&mut doc, &options).unwrap();
        let doc = round_trip(doc);
        assert!(!content(&doc, 1).contains("PdfToolsNum"), "page 1 is skipped");
        assert!(content(&doc, 2).contains("(5)"));
        assert!(content(&doc, 4).contains("(7)"));
    }

    #[test]
    fn rejects_text_the_built_in_font_cannot_show() {
        let mut doc = sample("a", 1);
        let err = add_page_numbers(&mut doc, &numbers("Страница {n}")).unwrap_err();
        assert!(err.to_string().contains("С"));
        assert!(win_ansi("Página – 1 €").is_ok());
    }

    #[test]
    fn stamps_a_transparent_image_on_every_page() {
        let mut doc = sample("a", 2);
        let stamp = ImageStamp {
            image: png(),
            anchor: Anchor::new(HAlign::Center, VAlign::Middle),
            margin: 0.05,
            width: 0.5,
            rotation: 45.0,
            opacity: 0.3,
            under: false,
            pages: None,
        };
        assert_eq!(add_image_stamp(&mut doc, &stamp).unwrap(), 2);
        let doc = round_trip(doc);
        for page in [1, 2] {
            let content = content(&doc, page);
            assert!(content.contains("/PdfToolsWm0 Do"), "{content}");
            assert!(content.contains("/PdfToolsGs0 gs"));
        }
        let image = doc
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .find(|s| s.dict.has(b"SMask"))
            .expect("image with a soft mask");
        assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 40);
        assert_eq!(page_labels(&doc), ["a-1", "a-2"]);
    }

    #[test]
    fn under_draws_before_the_page_content() {
        let mut doc = sample("a", 1);
        let stamp = ImageStamp {
            image: png(),
            anchor: Anchor::new(HAlign::Center, VAlign::Middle),
            margin: 0.0,
            width: 0.5,
            rotation: 0.0,
            opacity: 1.0,
            under: true,
            pages: Some(vec![1]),
        };
        add_image_stamp(&mut doc, &stamp).unwrap();
        let content = content(&doc, 1);
        assert!(content.find("Do").unwrap() < content.find("(a-1)").unwrap());
    }

    #[test]
    fn places_signatures() {
        let mut doc = sample("a", 2);
        let placement = Placement { page: 2, x: 0.6, y: 0.8, width: 0.3, height: 0.1 };
        add_signature(&mut doc, &png(), &[placement]).unwrap();
        let doc = round_trip(doc);
        assert!(content(&doc, 2).contains("/PdfToolsSig0 Do"));
        assert!(!content(&doc, 1).contains("PdfToolsSig"));
        let bad = Placement { page: 3, ..placement };
        assert!(matches!(add_signature(&mut sample("a", 2), &png(), &[bad]), Err(Error::InvalidPage { .. })));
    }

    #[test]
    fn measures_text() {
        assert_eq!(text_width(b"10"), 1112.0);
        assert_eq!(HELVETICA_WIDTHS[(b'W' - 32) as usize], 944);
        assert_eq!(HELVETICA_WIDTHS[(b'~' - 32) as usize], 584);
    }
}
