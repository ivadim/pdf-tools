//! Combine pages from several PDFs into one, in file order or any order the
//! caller chooses, optionally rotating individual pages.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};

use crate::Error;

/// Page attributes a page may inherit from its ancestors in the page tree.
/// Page trees are rebuilt flat, so these are copied onto each page first.
const INHERITABLE_PAGE_KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Ids reserved for the new page tree root and catalog of the merged file.
const PAGES_ID: ObjectId = (1, 0);
const CATALOG_ID: ObjectId = (2, 0);

/// Guards against cyclic `/Parent` chains in malformed files.
const MAX_TREE_DEPTH: usize = 64;

#[derive(Debug, Clone)]
pub enum MergeProgress {
    Loading { index: usize, total: usize, path: PathBuf },
    Merging,
    Writing,
}

impl fmt::Display for MergeProgress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loading { index, total, path } => {
                let name = path.file_name().unwrap_or(path.as_os_str());
                write!(f, "Reading file {} of {total} ({})…", index + 1, name.to_string_lossy())
            }
            Self::Merging => f.write_str("Combining pages…"),
            Self::Writing => f.write_str("Writing output…"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergeSummary {
    pub files: usize,
    pub pages: usize,
}

/// One page of an output document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSpec {
    /// Index into the list of input documents.
    pub source: usize,
    /// 1-based page number within that document.
    pub page: usize,
    /// Extra clockwise rotation in degrees; must be a multiple of 90.
    pub rotation: i64,
}

impl PageSpec {
    pub fn new(source: usize, page: usize) -> Self {
        Self { source, page, rotation: 0 }
    }

    pub fn rotated(self, rotation: i64) -> Self {
        Self { rotation, ..self }
    }
}

/// Merges `inputs` and writes the result to `output`.
///
/// With `pages: None` every page of every input is included, in file order;
/// otherwise the output contains exactly the listed pages, in list order.
///
/// All inputs are read before anything is written, and the output is written
/// to a temporary file first, so `output` may safely be one of the inputs.
pub fn merge_files<P: AsRef<Path>>(
    inputs: &[P],
    pages: Option<&[PageSpec]>,
    output: &Path,
    mut progress: impl FnMut(MergeProgress),
) -> Result<MergeSummary, Error> {
    let total = inputs.len();
    let mut docs = Vec::with_capacity(total);
    for (index, path) in inputs.iter().enumerate() {
        let path = path.as_ref();
        progress(MergeProgress::Loading { index, total, path: path.to_path_buf() });
        docs.push(crate::load(path)?);
    }

    progress(MergeProgress::Merging);
    let mut merged = merge_documents(docs, pages)?;
    let pages = merged.get_pages().len();

    progress(MergeProgress::Writing);
    crate::save(&mut merged, output)?;

    Ok(MergeSummary { files: total, pages })
}

/// Builds a new document from pages of `docs`; see [`merge_files`] for the
/// meaning of `pages`.
///
/// Document-level features of the inputs (outlines, form fields, named
/// destinations) are not carried over; pages and their content are.
pub fn merge_documents(docs: Vec<Document>, pages: Option<&[PageSpec]>) -> Result<Document, Error> {
    if docs.is_empty() {
        return Err(Error::NoInput);
    }

    let mut merged = Document::with_version("1.7");
    let mut next_id = CATALOG_ID.0 + 1;
    // Per input, its pages in page-number order with inherited attributes
    // made explicit.
    let mut source_pages: Vec<Vec<(ObjectId, Dictionary)>> = Vec::with_capacity(docs.len());

    for mut doc in docs {
        doc.renumber_objects_with(next_id);
        next_id = doc.max_id + 1;

        let doc_pages = flattened_pages(&doc)?;

        // Pages are added below, only if selected. The old catalog and page
        // tree nodes are replaced by new ones.
        let page_ids: HashSet<ObjectId> = doc_pages.iter().map(|(id, _)| *id).collect();
        for (id, object) in doc.objects {
            let replaced = matches!(type_name(&object), Some(b"Catalog") | Some(b"Pages"));
            if !replaced && !page_ids.contains(&id) {
                merged.objects.insert(id, object);
            }
        }
        source_pages.push(doc_pages);
    }

    let natural_order: Vec<PageSpec>;
    let order = match pages {
        Some(pages) => pages,
        None => {
            natural_order = source_pages
                .iter()
                .enumerate()
                .flat_map(|(source, pages)| (1..=pages.len()).map(move |page| PageSpec::new(source, page)))
                .collect();
            &natural_order
        }
    };
    if order.is_empty() {
        return Err(Error::NoPages);
    }

    let mut page_ids = Vec::with_capacity(order.len());
    let mut used = HashSet::new();
    for spec in order {
        let (id, page) = spec
            .page
            .checked_sub(1)
            .and_then(|index| source_pages.get(spec.source)?.get(index))
            .ok_or(Error::InvalidPage { input: spec.source, page: spec.page })?;

        let mut page = page.clone();
        apply_rotation(&mut page, spec.rotation)?;
        page.set("Parent", Object::Reference(PAGES_ID));
        // A page object may appear only once in the page tree, so repeats
        // get their own copy.
        let id = if used.insert(*id) {
            *id
        } else {
            next_id += 1;
            (next_id - 1, 0)
        };
        merged.objects.insert(id, Object::Dictionary(page));
        page_ids.push(id);
    }

    let count = page_ids.len() as i64;
    let kids: Vec<Object> = page_ids.into_iter().map(Object::Reference).collect();
    merged.objects.insert(
        PAGES_ID,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
        }),
    );
    merged.objects.insert(
        CATALOG_ID,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(PAGES_ID),
        }),
    );
    merged.trailer.set("Root", Object::Reference(CATALOG_ID));
    merged.max_id = next_id - 1;

    // Drop objects only the discarded catalogs referenced (outlines etc.).
    merged.prune_objects();
    merged.renumber_objects();
    merged.compress();
    Ok(merged)
}

/// The document's pages in order, each with inherited attributes copied in.
pub(crate) fn flattened_pages(doc: &Document) -> Result<Vec<(ObjectId, Dictionary)>, Error> {
    // get_pages() is keyed by page number, so this preserves page order.
    doc.get_pages()
        .into_values()
        .map(|page_id| {
            let mut page = doc.get_dictionary(page_id)?.clone();
            for key in INHERITABLE_PAGE_KEYS {
                if !page.has(key) {
                    if let Some(value) = find_inherited(doc, page_id, key) {
                        page.set(key, value);
                    }
                }
            }
            page.set("Type", "Page");
            Ok((page_id, page))
        })
        .collect()
}

/// Adds `rotation` degrees clockwise to the page's `/Rotate`.
pub(crate) fn apply_rotation(page: &mut Dictionary, rotation: i64) -> Result<(), Error> {
    if rotation % 90 != 0 {
        return Err(Error::InvalidRotation(rotation));
    }
    if rotation != 0 {
        let current = page.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
        page.set("Rotate", (current + rotation).rem_euclid(360));
    }
    Ok(())
}

/// Looks `key` up on the page or, failing that, its page tree ancestors.
pub(crate) fn inherited_attribute(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Object> {
    let page = doc.get_dictionary(page_id).ok()?;
    match page.get(key) {
        Ok(value) => Some(value.clone()),
        Err(_) => find_inherited(doc, page_id, key),
    }
}

fn find_inherited(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut node = doc.get_dictionary(page_id).ok()?;
    for _ in 0..MAX_TREE_DEPTH {
        let parent_id = node.get(b"Parent").and_then(Object::as_reference).ok()?;
        node = doc.get_dictionary(parent_id).ok()?;
        if let Ok(value) = node.get(key) {
            return Some(value.clone());
        }
    }
    None
}

pub(crate) fn type_name(object: &Object) -> Option<&[u8]> {
    let dict = match object {
        Object::Dictionary(dict) => dict,
        Object::Stream(stream) => &stream.dict,
        _ => return None,
    };
    dict.get(b"Type").and_then(Object::as_name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{page_labels, rotations, round_trip, sample, temp_dir};

    #[test]
    fn merges_pages_in_file_order() {
        let merged = merge_documents(vec![sample("a", 2), sample("b", 3), sample("c", 1)], None).unwrap();
        let merged = round_trip(merged);
        assert_eq!(page_labels(&merged), ["a-1", "a-2", "b-1", "b-2", "b-3", "c-1"]);
    }

    #[test]
    fn merges_pages_in_custom_order() {
        let order = [PageSpec::new(1, 2), PageSpec::new(0, 1), PageSpec::new(1, 1), PageSpec::new(0, 2)];
        let merged = round_trip(merge_documents(vec![sample("a", 2), sample("b", 2)], Some(&order)).unwrap());
        assert_eq!(page_labels(&merged), ["b-2", "a-1", "b-1", "a-2"]);
    }

    #[test]
    fn leaves_out_unselected_pages() {
        let order = [PageSpec::new(0, 3), PageSpec::new(0, 1)];
        let merged = round_trip(merge_documents(vec![sample("a", 3)], Some(&order)).unwrap());
        assert_eq!(page_labels(&merged), ["a-3", "a-1"]);
    }

    #[test]
    fn repeated_pages_get_separate_objects() {
        let order = [PageSpec::new(0, 1), PageSpec::new(0, 1).rotated(90)];
        let merged = round_trip(merge_documents(vec![sample("a", 1)], Some(&order)).unwrap());
        assert_eq!(page_labels(&merged), ["a-1", "a-1"]);
        assert_eq!(rotations(&merged), [0, 90]);
    }

    #[test]
    fn rotates_pages_relative_to_their_current_rotation() {
        let mut doc = sample("a", 3);
        let first = doc.get_pages()[&1];
        doc.get_object_mut(first).unwrap().as_dict_mut().unwrap().set("Rotate", 90);

        let order = [PageSpec::new(0, 1).rotated(90), PageSpec::new(0, 2).rotated(-90), PageSpec::new(0, 3).rotated(450)];
        let merged = round_trip(merge_documents(vec![doc], Some(&order)).unwrap());
        assert_eq!(rotations(&merged), [180, 270, 90]);
    }

    #[test]
    fn rejects_invalid_page_specs() {
        let missing = [PageSpec::new(0, 4)];
        assert!(matches!(merge_documents(vec![sample("a", 3)], Some(&missing)), Err(Error::InvalidPage { input: 0, page: 4 })));
        let zero = [PageSpec::new(0, 0)];
        assert!(matches!(merge_documents(vec![sample("a", 1)], Some(&zero)), Err(Error::InvalidPage { .. })));
        let no_source = [PageSpec::new(2, 1)];
        assert!(matches!(merge_documents(vec![sample("a", 1)], Some(&no_source)), Err(Error::InvalidPage { .. })));
        let skewed = [PageSpec::new(0, 1).rotated(45)];
        assert!(matches!(merge_documents(vec![sample("a", 1)], Some(&skewed)), Err(Error::InvalidRotation(45))));
        assert!(matches!(merge_documents(vec![sample("a", 1)], Some(&[])), Err(Error::NoPages)));
    }

    #[test]
    fn pages_keep_inherited_attributes() {
        let merged = round_trip(merge_documents(vec![sample("a", 1), sample("b", 1)], None).unwrap());
        for id in merged.get_pages().into_values() {
            let page = merged.get_dictionary(id).unwrap();
            assert!(page.has(b"Resources"), "page {id:?} lost its resources");
            assert!(page.has(b"MediaBox"), "page {id:?} lost its media box");
        }
    }

    #[test]
    fn rejects_empty_input() {
        assert!(matches!(merge_documents(Vec::new(), None), Err(Error::NoInput)));
    }

    #[test]
    fn output_may_overwrite_an_input() {
        let dir = temp_dir("merge");
        let first = dir.join("first.pdf");
        let second = dir.join("second.pdf");
        sample("a", 1).save(&first).unwrap();
        sample("b", 2).save(&second).unwrap();

        let summary = merge_files(&[&first, &second], None, &first, |_| {}).unwrap();
        assert_eq!(summary, MergeSummary { files: 2, pages: 3 });
        assert_eq!(page_labels(&crate::load(&first).unwrap()), ["a-1", "b-1", "b-2"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
