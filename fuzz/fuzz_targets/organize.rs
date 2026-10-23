//! Editing a loaded document and saving it: rotating every page, then
//! splitting off every other page, as Rotate and Split do.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pdftools_core::organize::organize_document;
use pdftools_core::PageSpec;

#[path = "common.rs"]
mod common;

/// Larger page trees only make runs slower.
const MAX_PAGES: usize = 64;

fuzz_target!(|data: &[u8]| {
    let Ok(doc) = pdftools_core::load(common::as_file(data)) else { return };
    let pages = doc.get_pages().len();
    if pages == 0 || pages > MAX_PAGES {
        return;
    }

    let mut rotated = doc.clone();
    let specs: Vec<PageSpec> = (1..=pages).map(|p| PageSpec::new(0, p).rotated(90)).collect();
    if organize_document(&mut rotated, &specs).is_ok() {
        let _ = rotated.save_to(&mut Vec::new());
    }

    let mut odd = doc;
    let specs: Vec<PageSpec> = (1..=pages).step_by(2).map(|p| PageSpec::new(0, p)).collect();
    if organize_document(&mut odd, &specs).is_ok() {
        let _ = odd.save_to(&mut Vec::new());
    }
});
