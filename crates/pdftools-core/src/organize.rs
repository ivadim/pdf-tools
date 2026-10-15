//! Rearrange, rotate, delete, duplicate and split the pages of one document.
//!
//! Unlike merging, these edit the document in place, so outlines, forms and
//! other document-level features survive (entries pointing at deleted pages
//! simply stop working).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lopdf::{dictionary, Document, Object, ObjectId};

use crate::merge::{apply_rotation, flattened_pages, type_name};
use crate::{Error, PageSpec};

/// Replaces the document's pages with `pages` (all with `source == 0`), in
/// that order. Pages may be repeated or left out.
pub fn organize_document(doc: &mut Document, pages: &[PageSpec]) -> Result<(), Error> {
    if pages.is_empty() {
        return Err(Error::NoPages);
    }
    let originals = flattened_pages(doc)?;
    let root_id = doc
        .catalog()?
        .get(b"Pages")
        .and_then(Object::as_reference)?;

    // Validate before touching anything.
    for spec in pages {
        if spec.source != 0 || spec.page == 0 || spec.page > originals.len() {
            return Err(Error::InvalidPage { input: spec.source, page: spec.page });
        }
        if spec.rotation % 90 != 0 {
            return Err(Error::InvalidRotation(spec.rotation));
        }
    }

    // The page tree is rebuilt flat under the existing root node.
    let tree_nodes: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, object)| type_name(object) == Some(b"Pages"))
        .map(|(id, _)| *id)
        .collect();
    for id in tree_nodes {
        doc.objects.remove(&id);
    }
    for (id, _) in &originals {
        doc.objects.remove(id);
    }

    let mut used = HashSet::new();
    let mut kids = Vec::with_capacity(pages.len());
    for spec in pages {
        let (original_id, page) = &originals[spec.page - 1];
        let mut page = page.clone();
        apply_rotation(&mut page, spec.rotation)?;
        page.set("Parent", Object::Reference(root_id));
        let id = if used.insert(*original_id) { *original_id } else { doc.new_object_id() };
        doc.objects.insert(id, Object::Dictionary(page));
        kids.push(Object::Reference(id));
    }

    let count = kids.len() as i64;
    doc.objects.insert(
        root_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
        }),
    );
    doc.prune_objects();
    Ok(())
}

/// Rewrites `input` with its pages arranged as `pages` describes.
pub fn organize_file(input: &Path, pages: &[PageSpec], output: &Path) -> Result<usize, Error> {
    let mut doc = crate::load(input)?;
    organize_document(&mut doc, pages)?;
    crate::save(&mut doc, output)?;
    Ok(pages.len())
}

/// One output file of a split.
#[derive(Debug, Clone)]
pub struct SplitPart {
    pub pages: Vec<PageSpec>,
    pub output: PathBuf,
}

/// Writes each part of `input` to its own file. `progress` receives the
/// index of the part about to be written.
pub fn split_file(
    input: &Path,
    parts: &[SplitPart],
    mut progress: impl FnMut(usize),
) -> Result<usize, Error> {
    if parts.is_empty() {
        return Err(Error::NoPages);
    }
    let doc = crate::load(input)?;
    for (index, part) in parts.iter().enumerate() {
        progress(index);
        let mut piece = doc.clone();
        organize_document(&mut piece, &part.pages)?;
        crate::save(&mut piece, &part.output)?;
    }
    Ok(parts.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{page_labels, rotations, round_trip, sample, temp_dir};

    fn specs(pages: &[usize]) -> Vec<PageSpec> {
        pages.iter().map(|&p| PageSpec::new(0, p)).collect()
    }

    #[test]
    fn reorders_deletes_and_duplicates_pages() {
        let mut doc = sample("a", 4);
        organize_document(&mut doc, &specs(&[4, 1, 1, 3])).unwrap();
        let doc = round_trip(doc);
        assert_eq!(page_labels(&doc), ["a-4", "a-1", "a-1", "a-3"]);
    }

    #[test]
    fn rotates_in_place() {
        let mut doc = sample("a", 2);
        organize_document(&mut doc, &[PageSpec::new(0, 1).rotated(90), PageSpec::new(0, 2).rotated(-90)]).unwrap();
        assert_eq!(rotations(&round_trip(doc)), [90, 270]);
    }

    #[test]
    fn keeps_document_level_entries() {
        let mut doc = sample("a", 2);
        let info = doc.add_object(dictionary! { "Title" => Object::string_literal("Kept") });
        doc.trailer.set("Info", info);
        let outlines = doc.add_object(dictionary! { "Type" => "Outlines", "Count" => 0 });
        doc.catalog_mut().unwrap().set("Outlines", outlines);

        organize_document(&mut doc, &specs(&[2])).unwrap();
        let doc = round_trip(doc);
        assert!(doc.catalog().unwrap().has(b"Outlines"));
        assert!(doc.trailer.has(b"Info"));
    }

    #[test]
    fn removed_pages_are_dropped_from_the_file() {
        let mut doc = sample("a", 3);
        organize_document(&mut doc, &specs(&[2])).unwrap();
        let doc = round_trip(doc);
        assert_eq!(page_labels(&doc), ["a-2"]);
        let streams: Vec<String> = doc
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .filter(|s| !s.dict.has_type(b"XRef"))
            .map(|s| String::from_utf8_lossy(&s.decompressed_content().unwrap_or(s.content.clone())).into_owned())
            .collect();
        assert_eq!(streams.len(), 1, "content of removed pages should be pruned: {streams:?}");
    }

    #[test]
    fn rejects_bad_specs_without_changing_the_document() {
        let mut doc = sample("a", 2);
        assert!(matches!(organize_document(&mut doc, &specs(&[3])), Err(Error::InvalidPage { .. })));
        assert!(matches!(organize_document(&mut doc, &[PageSpec::new(1, 1)]), Err(Error::InvalidPage { .. })));
        assert!(matches!(organize_document(&mut doc, &[]), Err(Error::NoPages)));
        assert_eq!(page_labels(&doc), ["a-1", "a-2"]);
    }

    #[test]
    fn loads_only_kept_pages() {
        let dir = temp_dir("keep");
        let input = dir.join("in.pdf");
        sample("a", 4).save(&input).unwrap();
        let doc = crate::load_pages(&input, Some(&[4, 2, 2])).unwrap();
        assert_eq!(page_labels(&doc), ["a-2", "a-4"], "kept pages stay in original order");
        let all = crate::load_pages(&input, Some(&[1, 2, 3, 4])).unwrap();
        assert_eq!(page_labels(&all).len(), 4);
        assert!(matches!(crate::load_pages(&input, Some(&[])), Err(Error::NoPages)));
        assert!(matches!(crate::load_pages(&input, Some(&[9])), Err(Error::InvalidPage { .. })));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn splits_into_files() {
        let dir = temp_dir("split");
        let input = dir.join("in.pdf");
        sample("a", 5).save(&input).unwrap();
        let parts = vec![
            SplitPart { pages: specs(&[1, 2]), output: dir.join("p1.pdf") },
            SplitPart { pages: specs(&[3]), output: dir.join("p2.pdf") },
            SplitPart { pages: specs(&[5, 4]), output: dir.join("p3.pdf") },
        ];
        let mut seen = Vec::new();
        assert_eq!(split_file(&input, &parts, |i| seen.push(i)).unwrap(), 3);
        assert_eq!(seen, [0, 1, 2]);
        assert_eq!(page_labels(&crate::load(&dir.join("p1.pdf")).unwrap()), ["a-1", "a-2"]);
        assert_eq!(page_labels(&crate::load(&dir.join("p2.pdf")).unwrap()), ["a-3"]);
        assert_eq!(page_labels(&crate::load(&dir.join("p3.pdf")).unwrap()), ["a-5", "a-4"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
