//! Drawing on top of (or under) existing pages.
//!
//! Overlays are drawn in *display space*: the page as a viewer shows it,
//! after `/Rotate`, with the origin at the bottom-left of the visible area.

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::merge::inherited_attribute;
use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

/// A position on the page, e.g. bottom-center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub h: HAlign,
    pub v: VAlign,
}

impl Anchor {
    pub const fn new(h: HAlign, v: VAlign) -> Self {
        Self { h, v }
    }

    /// Centre of a `width`×`height` box placed at this anchor on a
    /// `page_w`×`page_h` page, `margin` away from the edges.
    pub(crate) fn place(self, page_w: f32, page_h: f32, width: f32, height: f32, margin: f32) -> (f32, f32) {
        let x = match self.h {
            HAlign::Left => margin + width / 2.0,
            HAlign::Center => page_w / 2.0,
            HAlign::Right => page_w - margin - width / 2.0,
        };
        let y = match self.v {
            VAlign::Bottom => margin + height / 2.0,
            VAlign::Middle => page_h / 2.0,
            VAlign::Top => page_h - margin - height / 2.0,
        };
        (x, y)
    }
}

/// The visible page and how to map display space onto the page's own
/// coordinate system.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PageFrame {
    pub width: f32,
    pub height: f32,
    /// `cm` operands mapping display space to user space.
    pub matrix: [f32; 6],
}

impl PageFrame {
    pub fn cm(&self) -> Operation {
        cm(self.matrix)
    }
}

pub(crate) fn page_frame(doc: &Document, page_id: ObjectId) -> PageFrame {
    let rect = ["CropBox", "MediaBox"]
        .iter()
        .find_map(|key| rectangle(doc, inherited_attribute(doc, page_id, key.as_bytes())?))
        .unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let [x0, y0, x1, y1] = rect;
    let (w, h) = (x1 - x0, y1 - y0);
    let rotation = inherited_attribute(doc, page_id, b"Rotate").and_then(|r| r.as_i64().ok()).unwrap_or(0).rem_euclid(360);
    match rotation {
        90 => PageFrame { width: h, height: w, matrix: [0.0, 1.0, -1.0, 0.0, x1, y0] },
        180 => PageFrame { width: w, height: h, matrix: [-1.0, 0.0, 0.0, -1.0, x1, y1] },
        270 => PageFrame { width: h, height: w, matrix: [0.0, -1.0, 1.0, 0.0, x0, y1] },
        _ => PageFrame { width: w, height: h, matrix: [1.0, 0.0, 0.0, 1.0, x0, y0] },
    }
}

fn rectangle(doc: &Document, object: Object) -> Option<[f32; 4]> {
    let items = doc.dereference(&object).ok()?.1.as_array().ok()?;
    let mut values = [0.0; 4];
    for (value, item) in values.iter_mut().zip(items) {
        *value = doc.dereference(item).ok()?.1.as_float().ok()?;
    }
    let [a, b, c, d] = values;
    (items.len() == 4).then(|| [a.min(c), b.min(d), a.max(c), b.max(d)])
}

pub(crate) fn cm(m: [f32; 6]) -> Operation {
    Operation::new("cm", m.iter().map(|&v| Object::Real(v)).collect())
}

/// A shared resource (font, image, graphics state) an overlay refers to.
pub(crate) struct Resource {
    /// Resource category, e.g. `b"XObject"`.
    pub category: &'static [u8],
    /// Name prefix; a free name is chosen per page.
    pub prefix: &'static str,
    pub object: Object,
}

/// Draws operations on a page, isolated from the page's own graphics state.
/// `build` receives the resource names chosen for `resources`, in order.
pub(crate) fn add_overlay(
    doc: &mut Document,
    page_id: ObjectId,
    resources: &[Resource],
    under: bool,
    build: impl FnOnce(&[Vec<u8>]) -> Vec<Operation>,
) -> Result<(), Error> {
    let mut page_resources = inherited_attribute(doc, page_id, b"Resources")
        .and_then(|r| doc.dereference(&r).ok()?.1.as_dict().ok().cloned())
        .unwrap_or_default();

    let mut names = Vec::with_capacity(resources.len());
    for resource in resources {
        let mut category: Dictionary =
            page_resources.get(resource.category).ok().and_then(|c| doc.dereference(c).ok()?.1.as_dict().ok().cloned()).unwrap_or_default();
        let name = (0..).map(|n| format!("{}{n}", resource.prefix).into_bytes()).find(|name| !category.has(name)).expect("unbounded range");
        category.set(name.clone(), resource.object.clone());
        page_resources.set(resource.category, Object::Dictionary(category));
        names.push(name);
    }

    let overlay = Content { operations: build(&names) }.encode()?;
    let existing = existing_contents(doc, page_id);

    let mut contents = Vec::with_capacity(existing.len() + 2);
    if under {
        let mut first = b"q\n".to_vec();
        first.extend_from_slice(&overlay);
        first.extend_from_slice(b"\nQ\nq\n");
        contents.push(add_stream(doc, first));
        contents.extend(existing);
        contents.push(add_stream(doc, b"\nQ\n".to_vec()));
    } else {
        contents.push(add_stream(doc, b"q\n".to_vec()));
        contents.extend(existing);
        let mut last = b"\nQ\nq\n".to_vec();
        last.extend_from_slice(&overlay);
        last.extend_from_slice(b"\nQ\n");
        contents.push(add_stream(doc, last));
    }

    let page = doc.get_dictionary_mut(page_id)?;
    page.set("Resources", Object::Dictionary(page_resources));
    page.set("Contents", Object::Array(contents));
    Ok(())
}

fn existing_contents(doc: &Document, page_id: ObjectId) -> Vec<Object> {
    let Ok(contents) = doc.get_dictionary(page_id).and_then(|p| p.get(b"Contents")) else {
        return Vec::new();
    };
    match contents {
        Object::Array(items) => items.clone(),
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Array(items)) => items.clone(),
            _ => vec![contents.clone()],
        },
        _ => Vec::new(),
    }
}

fn add_stream(doc: &mut Document, content: Vec<u8>) -> Object {
    Object::Reference(doc.add_object(Stream::new(Dictionary::new(), content)))
}

/// Resolves an optional 1-based page selection against the document,
/// returning page ids in order.
pub(crate) fn selected_pages(doc: &Document, pages: Option<&[usize]>) -> Result<Vec<(usize, ObjectId)>, Error> {
    let all = doc.get_pages();
    match pages {
        None => Ok(all.into_iter().map(|(n, id)| (n as usize, id)).collect()),
        Some(pages) => {
            pages.iter().map(|&n| all.get(&(n as u32)).map(|&id| (n, id)).ok_or(Error::InvalidPage { input: 0, page: n })).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::sample;

    fn frame_for(rotate: i64) -> PageFrame {
        let mut doc = sample("a", 1);
        let id = doc.get_pages()[&1];
        let page = doc.get_dictionary_mut(id).unwrap();
        page.set("Rotate", rotate);
        page.set("CropBox", vec![10.into(), 20.into(), 110.into(), 220.into()]);
        page_frame(&doc, id)
    }

    fn apply(m: [f32; 6], (x, y): (f32, f32)) -> (f32, f32) {
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }

    #[test]
    fn frame_maps_display_corners_onto_the_crop_box() {
        // Display bottom-left must land on the corner a viewer shows there.
        let cases = [(0, (10.0, 20.0)), (90, (110.0, 20.0)), (180, (110.0, 220.0)), (270, (10.0, 220.0))];
        for (rotate, expected_origin) in cases {
            let frame = frame_for(rotate);
            assert_eq!(apply(frame.matrix, (0.0, 0.0)), expected_origin, "rotate {rotate}");
            // The opposite corner maps to the opposite corner of the box.
            let far = apply(frame.matrix, (frame.width, frame.height));
            let expected_far = (120.0 - expected_origin.0, 240.0 - expected_origin.1);
            assert_eq!(far, expected_far, "rotate {rotate}");
        }
        assert_eq!((frame_for(90).width, frame_for(90).height), (200.0, 100.0));
    }

    #[test]
    fn overlay_keeps_the_original_content_and_isolates_state() {
        let mut doc = sample("a", 1);
        let id = doc.get_pages()[&1];
        add_overlay(&mut doc, id, &[], false, |_| vec![Operation::new("n", vec![])]).unwrap();
        let content = String::from_utf8(doc.get_page_content(id)).unwrap();
        let original = content.find("(a-1)").unwrap();
        let overlay = content.rfind("\nn\n").unwrap();
        assert!(content.starts_with("q\n") && original < overlay);
    }

    #[test]
    fn resource_names_never_collide() {
        let mut doc = sample("a", 1);
        let id = doc.get_pages()[&1];
        let resource = || Resource { category: b"XObject", prefix: "Im", object: Object::Null };
        let mut first = Vec::new();
        add_overlay(&mut doc, id, &[resource()], false, |n| {
            first = n.to_vec();
            vec![]
        })
        .unwrap();
        let mut second = Vec::new();
        add_overlay(&mut doc, id, &[resource()], false, |n| {
            second = n.to_vec();
            vec![]
        })
        .unwrap();
        assert_ne!(first, second);
    }
}
