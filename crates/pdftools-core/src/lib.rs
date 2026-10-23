//! Core PDF operations shared by the PDF Tools front-ends.
//!
//! Nothing in this crate depends on a GUI, so every operation can also be
//! driven from a CLI or tests.

pub mod compress;
mod error;
pub mod grayscale;
mod image_data;
pub mod images;
pub mod merge;
pub mod metadata;
pub mod organize;
mod parallel;
mod repair;
pub mod security;
mod stamp;
pub mod stamps;

use std::path::{Path, PathBuf};

use lopdf::{Document, Object, ObjectId};

pub use error::Error;
pub use merge::{merge_documents, merge_files, MergeProgress, MergeSummary, PageSpec};
pub use metadata::Metadata;
pub use parallel::{max_threads, set_max_threads, DEFAULT_THREADS};
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
    load_or_repair(path).map(|(doc, _)| doc)
}

/// Loads a PDF, recovering what it can from a damaged one; the flag says
/// whether it had to.
fn load_or_repair(path: &Path) -> Result<(Document, bool), Error> {
    let bytes = read_pdf(path)?;
    match Document::load_mem(bytes.as_ref()) {
        Ok(doc) => Ok((doc, false)),
        // Missing files, passwords and the like aren't damage.
        Err(source @ (lopdf::Error::IO(_) | lopdf::Error::Decryption(_))) => Err(Error::Load { path: path.to_path_buf(), source }),
        Err(source) => repair::load(bytes.as_ref()).map(|doc| (doc, true)).ok_or(Error::Load { path: path.to_path_buf(), source }),
    }
}

/// A PDF's bytes for parsing (mapped on iOS, see [`read_file`]), once it's
/// clear there's memory to parse it.
pub(crate) fn read_pdf(path: &Path) -> Result<FileBytes, Error> {
    let bytes = read_file(path).map_err(|e| Error::Load { path: path.to_path_buf(), source: lopdf::Error::IO(e) })?;
    check_memory(path, bytes.as_ref().len() as u64, available_memory())?;
    Ok(bytes)
}

/// Parsing copies every object of a PDF: it takes about this many times the
/// file's size in memory (measured: 3 GB for an 877 MB file, including the file).
const PARSE_FACTOR: (u64, u64) = (5, 2);

/// Fails, instead of being stopped by the system, when parsing a `size`-byte
/// PDF wouldn't fit in the `available` memory (unknown: always tries).
fn check_memory(path: &Path, size: u64, available: Option<u64>) -> Result<(), Error> {
    let needed = size.saturating_mul(PARSE_FACTOR.0) / PARSE_FACTOR.1;
    match available {
        Some(available) if needed > available => Err(Error::TooLarge { path: path.to_path_buf(), needed, available }),
        _ => Ok(()),
    }
}

/// Memory the app may still use before the system stops it. Only iOS limits
/// apps this way (and says how much is left).
fn available_memory() -> Option<u64> {
    #[cfg(target_os = "ios")]
    {
        extern "C" {
            // <os/proc.h>, iOS 13 and later.
            fn os_proc_available_memory() -> usize;
        }
        // SAFETY: takes no arguments and only reads the process's memory limit.
        let available = unsafe { os_proc_available_memory() } as u64;
        if available > 0 {
            return Some(available);
        }
    }
    None
}

/// A damaged PDF's recoverable pages as a fresh, readable file (for viewers
/// that can't repair it themselves); `None` if nothing can be saved.
pub fn repaired_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut doc = repair::load(bytes)?;
    let mut out = Vec::new();
    doc.save_to(&mut out).ok()?;
    Some(out)
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
    /// The file was damaged and only what could be recovered was read.
    pub repaired: bool,
    /// Images in the file and their stored size, which is what lossy
    /// compression can shrink.
    pub images: usize,
    pub image_bytes: u64,
}

pub fn inspect(path: &Path) -> Result<PdfInfo, Error> {
    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let (doc, repaired) = load_or_repair(path)?;
    if doc.is_encrypted() {
        return Ok(PdfInfo { needs_password: true, encrypted: true, file_size, ..Default::default() });
    }
    Ok(PdfInfo {
        pages: Some(doc.get_pages().len()),
        repaired,
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
/// A file's bytes, as [`read_file`] returns them.
pub enum FileBytes {
    Read(Vec<u8>),
    #[cfg(target_os = "ios")]
    Mapped(memmap2::Mmap),
}

impl AsRef<[u8]> for FileBytes {
    fn as_ref(&self) -> &[u8] {
        match self {
            FileBytes::Read(bytes) => bytes,
            #[cfg(target_os = "ios")]
            FileBytes::Mapped(map) => map,
        }
    }
}

/// A file's bytes. On iOS the file is memory-mapped: its pages are read on
/// demand, and as clean file-backed memory the system can drop and re-read
/// them, so a large PDF doesn't count against the app's memory limit.
/// Elsewhere it is read into memory (another program could shorten a mapped
/// file, which would crash the reader).
pub fn read_file(path: &Path) -> std::io::Result<FileBytes> {
    #[cfg(target_os = "ios")]
    {
        let file = std::fs::File::open(path)?;
        if file.metadata()?.len() > 0 {
            // SAFETY: the iOS app only maps its own copies of files, in its
            // temporary folder, which nothing changes while they're open.
            if let Ok(map) = unsafe { memmap2::Mmap::map(&file) } {
                return Ok(FileBytes::Mapped(map));
            }
        }
    }
    std::fs::read(path).map(FileBytes::Read)
}

/// Deflates every stream that may be and isn't compressed yet, like lopdf's
/// `Document::compress` (same level, same output) but on several threads.
pub fn compress_streams(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, object)| matches!(object, Object::Stream(stream) if stream.allows_compression && stream.dict.get(b"Filter").is_err()))
        .map(|(id, _)| *id)
        .collect();
    let shared = &*doc;
    let compressed = parallel::map(
        &ids,
        |id| match shared.objects.get(id) {
            Some(Object::Stream(stream)) => deflated(&stream.content, flate2::Compression::best()),
            _ => None,
        },
        |_| {},
    );
    for (id, content) in ids.iter().zip(compressed) {
        if let (Some(content), Some(Object::Stream(stream))) = (content, doc.objects.get_mut(id)) {
            stream.dict.set("Filter", "FlateDecode");
            stream.set_content(content);
        }
    }
}

/// `data` deflated (zlib), if that saves anything: lopdf's rule, which leaves
/// streams alone unless compressing beats the bytes `/Filter /FlateDecode` adds.
pub(crate) fn deflated(data: &[u8], level: flate2::Compression) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::with_capacity(data.len() / 2), level);
    encoder.write_all(data).ok()?;
    let compressed = encoder.finish().ok()?;
    (compressed.len() + 19 < data.len()).then_some(compressed)
}

pub fn save(doc: &mut Document, output: &Path) -> Result<(), Error> {
    let save_error = |message: String| Error::Save { path: output.to_path_buf(), message };

    let io_error = |e: std::io::Error| save_error(error::io_reason(&e));

    // Written straight to the file (a whole PDF in memory first would cost its
    // size again), as a temporary file that replaces `output` when complete.
    let mut temp_name = output.as_os_str().to_owned();
    temp_name.push(".part");
    let temp = PathBuf::from(temp_name);
    let result = std::fs::File::create(&temp).map_err(io_error).and_then(|file| {
        let mut writer = std::io::BufWriter::with_capacity(1 << 20, file);
        doc.save_to(&mut writer).map_err(io_error)?;
        // Flushed, not synced: some file systems (the web version's in-memory
        // one) can't sync, and the rename below only happens once it's written.
        writer.into_inner().map_err(|e| io_error(e.into_error()))?;
        std::fs::rename(&temp, output).map_err(io_error)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    #[test]
    fn refuses_files_too_large_for_the_memory_left() {
        let path = Path::new("big.pdf");
        assert!(check_memory(path, 1_000, None).is_ok(), "no limit known");
        assert!(check_memory(path, 1_000, Some(2_500)).is_ok());
        assert!(matches!(check_memory(path, 1_000, Some(2_499)), Err(Error::TooLarge { needed: 2_500, available: 2_499, .. })));
        assert!(check_memory(path, u64::MAX, Some(u64::MAX / 4)).is_err(), "saturates rather than overflowing");
    }

    #[test]
    fn saves_through_a_temporary_file() {
        let dir = std::env::temp_dir().join(format!("pdftools-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("out.pdf");
        let mut doc = test_support::sample("a", 3);
        save(&mut doc, &output).unwrap();
        assert_eq!(Document::load(&output).unwrap().get_pages().len(), 3);
        assert!(!dir.join("out.pdf.part").exists());
        // A folder that doesn't exist: an error, and nothing left behind.
        let missing = dir.join("missing/out.pdf");
        assert!(matches!(save(&mut doc, &missing), Err(Error::Save { .. })));
        assert!(!dir.join("missing").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn compresses_streams_like_lopdf() {
        let mut doc = Document::with_version("1.7");
        let text = b"BT /F1 12 Tf 72 712 Td (Hello) Tj ET\n".repeat(200);
        doc.add_object(Stream::new(dictionary! {}, text.clone()));
        doc.add_object(Stream::new(dictionary! {}, text.clone()).with_compression(false));
        doc.add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));
        doc.add_object(Stream::new(dictionary! { "Filter" => "DCTDecode" }, text.clone()));
        doc.add_object(Stream::new(dictionary! {}, (0..20_000u32).map(|i| (i * 7 % 251) as u8).collect()));

        let mut theirs = doc.clone();
        theirs.compress();
        set_max_threads(4);
        compress_streams(&mut doc);
        assert_eq!(doc.objects, theirs.objects);
        // The long text was compressed; the rest stayed as it was.
        assert!(matches!(&doc.objects[&(1, 0)], Object::Stream(s) if s.content.len() < text.len() / 10));
        assert!(matches!(&doc.objects[&(2, 0)], Object::Stream(s) if s.content == text));
    }
}
