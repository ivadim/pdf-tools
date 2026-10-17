//! Recovering damaged PDFs, most often ones cut short by an interrupted
//! download or copy. lopdf already rebuilds a broken cross-reference table by
//! scanning for objects, but it needs a trailer to find the document's root,
//! and a truncated file has lost it (often along with the catalogue and the
//! page tree, which many writers put at the end). Here we supply what's
//! missing and gather the pages that survive.

use lopdf::{dictionary, Document, Object, ObjectId};

/// Attributes a page may inherit from its parents in the page tree.
const INHERITED: [&[u8]; 4] = [b"MediaBox", b"CropBox", b"Resources", b"Rotate"];

/// Loads what can be saved from a damaged PDF: `None` if no page survives.
pub(crate) fn load(bytes: &[u8]) -> Option<Document> {
    let headers = object_headers(bytes);
    let max_id = headers.iter().map(|(_, (id, _))| *id).max()?;
    // The object holding the last `/Type /Catalog`, if it survived.
    let catalog = find_catalog(bytes).and_then(|at| headers.iter().rev().find(|(offset, _)| *offset < at)).map(|(_, id)| *id);

    let mut patched = bytes.to_vec();
    let root = match catalog {
        Some(id) => id,
        None => {
            let (pages, root) = (max_id + 1, max_id + 2);
            patched.extend_from_slice(
                format!("\n{pages} 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n{root} 0 obj\n<< /Type /Catalog /Pages {pages} 0 R >>\nendobj\n").as_bytes(),
            );
            (root, 0)
        }
    };
    patched.extend_from_slice(format!("\ntrailer\n<< /Root {} {} R /Size {} >>\n%%EOF\n", root.0, root.1, max_id + 3).as_bytes());

    let mut doc = Document::load_mem(&patched).ok()?;
    if doc.get_pages().is_empty() {
        rebuild_page_tree(&mut doc)?;
    }
    (!doc.get_pages().is_empty()).then_some(doc)
}

/// Gives the catalogue a fresh page tree holding every surviving page, in
/// object order (which is page order for almost every writer).
fn rebuild_page_tree(doc: &mut Document) -> Option<()> {
    let mut pages: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| o.as_dict().is_ok_and(|d| d.get(b"Type").and_then(Object::as_name).ok() == Some(b"Page")))
        .map(|(id, _)| *id)
        .collect();
    pages.sort();
    if pages.is_empty() {
        return None;
    }
    let tree = doc.new_object_id();
    for &page in &pages {
        let inherited: Vec<(Vec<u8>, Object)> =
            INHERITED.iter().filter_map(|key| inherited_value(doc, page, key).map(|v| (key.to_vec(), v))).collect();
        let dict = doc.get_object_mut(page).ok()?.as_dict_mut().ok()?;
        for (key, value) in inherited {
            if !dict.has(&key) {
                dict.set(key, value);
            }
        }
        if !dict.has(b"MediaBox") {
            dict.set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
        }
        dict.set("Parent", tree);
    }
    let count = pages.len() as i64;
    let kids: Vec<Object> = pages.into_iter().map(Object::Reference).collect();
    doc.objects.insert(tree, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }));
    let root = doc.trailer.get(b"Root").and_then(Object::as_reference).ok()?;
    doc.get_object_mut(root).ok()?.as_dict_mut().ok()?.set("Pages", tree);
    Some(())
}

/// A page's own value for `key`, or the nearest one up its (surviving) parents.
fn inherited_value(doc: &Document, page: ObjectId, key: &[u8]) -> Option<Object> {
    let mut node = doc.get_dictionary(page).ok()?;
    for _ in 0..32 {
        if let Ok(value) = node.get(key) {
            return Some(value.clone());
        }
        node = doc.get_dictionary(node.get(b"Parent").and_then(Object::as_reference).ok()?).ok()?;
    }
    None
}

/// `(offset, id)` of each `N G obj` that starts a line.
fn object_headers(bytes: &[u8]) -> Vec<(usize, ObjectId)> {
    let mut found = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let end = bytes[start..].iter().position(|&b| b == b'\n' || b == b'\r').map_or(bytes.len(), |i| start + i);
        let line = &bytes[start..end.min(start + 40)];
        let mut words = line.split(|b| b.is_ascii_whitespace()).filter(|w| !w.is_empty());
        let number = |w: Option<&[u8]>| w.and_then(|w| std::str::from_utf8(w).ok()).and_then(|s| s.parse().ok());
        if let (Some(id), Some(generation), Some(b"obj")) = (number(words.next()), number(words.next()), words.next()) {
            found.push((start, (id, generation as u16)));
        }
        start = end + 1;
    }
    found
}

/// Where the last `/Type /Catalog` (any spacing) is.
fn find_catalog(bytes: &[u8]) -> Option<usize> {
    let mut last = None;
    let mut from = 0;
    while let Some(i) = bytes[from..].windows(5).position(|w| w == b"/Type") {
        let at = from + i;
        let rest = &bytes[at + 5..];
        let skip = rest.iter().take_while(|b| b.is_ascii_whitespace()).count();
        if rest[skip..].starts_with(b"/Catalog") {
            last = Some(at);
        }
        from = at + 5;
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_pages_from_a_cut_off_file() {
        // Two pages, then the page tree and catalogue, then the cross-reference
        // table: everything after the pages is cut off.
        let page = |n: u32, contents: u32, size: &str| {
            format!(
                "{n} 0 obj
<< /Type /Page /Parent 9 0 R /MediaBox [0 0 {size}] /Contents {contents} 0 R >>
endobj
"
            )
        };
        let content = |n: u32, text: &str| {
            let ops = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
            format!(
                "{n} 0 obj
<< /Length {} >>
stream
{ops}
endstream
endobj
",
                ops.len()
            )
        };
        let mut pdf = String::from(
            "%PDF-1.4
",
        );
        pdf += &content(1, "one");
        pdf += &page(2, 1, "612 792");
        pdf += &content(3, "two");
        pdf += &page(4, 3, "792 612");
        let cut = pdf.len();
        pdf += "9 0 obj
<< /Type /Pages /Kids [2 0 R 4 0 R] /Count 2 >>
endobj
10 0 obj
<< /Type /Catalog /Pages 9 0 R >>
endobj
";
        let bytes = &pdf.as_bytes()[..cut];
        assert!(Document::load_mem(bytes).is_err(), "should be damaged");

        let repaired = load(bytes).expect("pages recovered");
        let pages: Vec<ObjectId> = repaired.get_pages().into_values().collect();
        assert_eq!(pages, [(2, 0), (4, 0)]);
        assert!(String::from_utf8_lossy(&repaired.get_page_content(pages[1])).contains("(two)"));
        // And it saves as a file that opens normally.
        let saved = Document::load_mem(&crate::repaired_bytes(bytes).unwrap()).unwrap();
        assert_eq!(saved.get_pages().len(), 2);
    }

    #[test]
    fn gives_up_on_files_without_pages() {
        assert!(load(b"Hello, I am not a PDF").is_none());
        assert!(load(b"").is_none());
        assert!(load(b"%PDF-1.4\n1 0 obj\n<< /Type /Font >>\nendobj\n").is_none());
    }
}
