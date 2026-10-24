//! What the apps show when a PDF is opened, read without parsing the whole
//! file: opening a large PDF with lopdf costs several times its size.

use std::path::Path;
use std::sync::Arc;

use hayro::hayro_syntax::Pdf;
use pdftools_core::metadata::parse_pdf_date;
use pdftools_core::{Metadata, PdfInfo};

/// Page count, encryption and document details, read lazily: lopdf reads
/// only the trailer, cross-reference table and Info dictionary, and hayro
/// counts the pages. Image statistics are left at zero
/// ([`pdftools_core::inspect`] counts them). Files either can't read (damaged
/// ones) get the full [`pdftools_core::inspect`], which repairs them and says so.
pub fn quick_inspect(path: &Path) -> Result<PdfInfo, pdftools_core::Error> {
    let full = || pdftools_core::inspect(path);
    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let Ok(bytes) = pdftools_core::read_file(path) else { return full() };
    let Ok(meta) = lopdf::Document::load_metadata_mem(bytes.as_ref()) else { return full() };
    if meta.encrypted && meta.page_count == 0 {
        // The empty password didn't open it.
        return Ok(PdfInfo { needs_password: true, encrypted: true, file_size, ..Default::default() });
    }
    let Ok(pdf) = Pdf::new(Arc::new(bytes)) else { return full() };
    let text = |value: Option<String>| value.filter(|s| !s.trim().is_empty());
    let date = |value: Option<String>| text(value).and_then(|raw| parse_pdf_date(&raw));
    Ok(PdfInfo {
        pages: Some(pdf.pages().len()),
        needs_password: false,
        // Files that only restrict printing, copying… open without a password.
        encrypted: meta.encrypted,
        file_size,
        metadata: Metadata {
            title: text(meta.title),
            author: text(meta.author),
            subject: text(meta.subject),
            keywords: text(meta.keywords),
            creator: text(meta.creator),
            producer: text(meta.producer),
            created: date(meta.creation_date),
            modified: date(meta.modification_date),
        },
        repaired: false,
        images: 0,
        image_bytes: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(dir: &str, name: &str) -> PathBuf {
        let repo =
            if std::env::var_os("TEST_SRCDIR").is_some() { PathBuf::new() } else { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") };
        repo.join("tests/fixtures").join(dir).join(name)
    }

    #[test]
    fn agrees_with_the_full_inspection() {
        for (dir, name) in [
            ("pdfjs", "tracemonkey.pdf"),
            ("pdfjs", "rotation.pdf"),
            ("pdfjs", "TAMReview.pdf"),
            ("pdfjs", "issue5010.pdf"),
            ("encrypted", "aes256-password-test.pdf"),
            ("encrypted", "aes256-restrictions-only.pdf"),
        ] {
            let path = fixture(dir, name);
            let (quick, full) = (quick_inspect(&path).unwrap(), pdftools_core::inspect(&path).unwrap());
            assert_eq!(
                (quick.pages, quick.needs_password, quick.encrypted, quick.file_size, quick.repaired),
                (full.pages, full.needs_password, full.encrypted, full.file_size, full.repaired),
                "{name}"
            );
            assert_eq!(quick.metadata, full.metadata, "{name}");
        }
    }

    #[test]
    fn missing_files_are_errors() {
        assert!(quick_inspect(&fixture("pdfjs", "missing.pdf")).is_err());
    }
}
