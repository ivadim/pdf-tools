//! Core PDF operations shared by the PDF Tools front-ends.
//!
//! Nothing in this crate depends on a GUI, so every operation can also be
//! driven from a CLI or tests.

pub mod compress;
mod error;
pub mod grayscale;
pub mod images;
mod image_data;
pub mod merge;
pub mod metadata;
pub mod organize;
pub mod security;
mod stamp;
pub mod stamps;

use std::path::{Path, PathBuf};

use lopdf::Document;

pub use error::Error;
pub use merge::{merge_documents, merge_files, MergeProgress, MergeSummary, PageSpec};
pub use metadata::Metadata;
pub use stamp::{Anchor, HAlign, VAlign};

/// Loads a PDF from disk, rejecting documents we cannot read the content of.
///
/// Files that only restrict permissions (an owner password without a user
/// password) open fine; their restrictions are not carried into outputs.
pub fn load(path: &Path) -> Result<Document, Error> {
    let doc = load_unchecked(path)?;
    if doc.is_encrypted() {
        return Err(Error::Encrypted(path.to_path_buf()));
    }
    Ok(doc)
}

/// Loads a PDF without requiring it to be readable; the caller checks
/// [`Document::is_encrypted`].
pub(crate) fn load_unchecked(path: &Path) -> Result<Document, Error> {
    Document::load(path).map_err(|source| Error::Load {
        path: path.to_path_buf(),
        source,
    })
}

/// Loads a PDF keeping only the given 1-based pages (in their original
/// order); `None` keeps every page.
pub fn load_pages(path: &Path, keep: Option<&[usize]>) -> Result<Document, Error> {
    let mut doc = load(path)?;
    if let Some(keep) = keep {
        let total = doc.get_pages().len();
        let mut pages: Vec<usize> = keep.to_vec();
        pages.sort_unstable();
        pages.dedup();
        if pages.len() != total || pages.iter().enumerate().any(|(i, &p)| p != i + 1) {
            let specs: Vec<PageSpec> = pages.into_iter().map(|p| PageSpec::new(0, p)).collect();
            organize::organize_document(&mut doc, &specs)?;
        }
    }
    Ok(doc)
}

/// Returns the number of pages in the PDF at `path`.
pub fn page_count(path: &Path) -> Result<usize, Error> {
    Ok(load(path)?.get_pages().len())
}

/// What the tools need to know about a file before working on it.
#[derive(Debug, Clone, Default)]
pub struct PdfInfo {
    /// `None` while the file is locked with a password.
    pub pages: Option<usize>,
    /// A password is needed to open the file.
    pub needs_password: bool,
    /// The file was encrypted, e.g. to restrict printing or copying.
    pub encrypted: bool,
    pub file_size: u64,
    pub metadata: Metadata,
    /// Images in the file and their stored size, which is what lossy
    /// compression can shrink.
    pub images: usize,
    pub image_bytes: u64,
}

pub fn inspect(path: &Path) -> Result<PdfInfo, Error> {
    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let doc = load_unchecked(path)?;
    if doc.is_encrypted() {
        return Ok(PdfInfo { needs_password: true, encrypted: true, file_size, ..Default::default() });
    }
    Ok(PdfInfo {
        pages: Some(doc.get_pages().len()),
        needs_password: false,
        // Loading transparently decrypts files that have no user password.
        encrypted: doc.encryption_state.is_some(),
        file_size,
        metadata: metadata::read(&doc),
        images: doc.objects.values().filter(|o| image_data::is_image(o)).count(),
        image_bytes: doc
            .objects
            .values()
            .filter(|o| image_data::is_image(o))
            .filter_map(|o| o.as_stream().ok())
            .map(|s| s.content.len() as u64)
            .sum(),
    })
}

/// Writes `doc` to `output` via a temporary file, so `output` may be the file
/// the document was loaded from.
pub fn save(doc: &mut Document, output: &Path) -> Result<(), Error> {
    let save_error = |message: String| Error::Save { path: output.to_path_buf(), message };

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).map_err(|e| save_error(e.to_string()))?;

    let mut temp_name = output.as_os_str().to_owned();
    temp_name.push(".part");
    let temp = PathBuf::from(temp_name);
    std::fs::write(&temp, &bytes)
        .and_then(|()| std::fs::rename(&temp, output))
        .map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            save_error(e.to_string())
        })
}

/// Size of the file at `path`, or 0 if it can't be read.
pub fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

#[cfg(test)]
pub(crate) mod test_support {
    use lopdf::{dictionary, Document, Object, Stream};

    /// A document whose pages inherit Resources and MediaBox from the page
    /// tree root, with each page's content stream naming its origin.
    pub fn sample(label: &str, pages: usize) -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let mut kids: Vec<Object> = Vec::new();
        for n in 1..=pages {
            let content = format!("BT /F1 24 Tf 72 720 Td ({label}-{n}) Tj ET");
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
            });
            kids.push(page_id.into());
        }

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => pages as i64,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    pub fn round_trip(mut doc: Document) -> Document {
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        Document::load_mem(&bytes).unwrap()
    }

    /// The text label drawn on each page, in page order.
    pub fn page_labels(doc: &Document) -> Vec<String> {
        doc.get_pages()
            .into_values()
            .map(|id| {
                let content = String::from_utf8_lossy(&doc.get_page_content(id)).into_owned();
                let start = content.find('(').unwrap() + 1;
                let end = content[start..].find(')').unwrap() + start;
                content[start..end].to_owned()
            })
            .collect()
    }

    pub fn rotations(doc: &Document) -> Vec<i64> {
        doc.get_pages()
            .into_values()
            .map(|id| {
                let page = doc.get_dictionary(id).unwrap();
                page.get(b"Rotate").and_then(Object::as_i64).unwrap_or(0)
            })
            .collect()
    }

    pub fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pdftools-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
