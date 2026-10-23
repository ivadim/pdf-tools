//! Page previews: parsing with hayro (falling back to the core's recovery of
//! damaged files) and rendering the first page, small.
#![no_main]

use libfuzzer_sys::fuzz_target;
use pdftools_render::pages::{render, DocCache, PreviewRequest};

#[path = "common.rs"]
mod common;

fuzz_target!(|data: &[u8]| {
    let request = PreviewRequest { path: common::as_file(data).to_path_buf(), page: 1, width: 64, rotation: 90, gray: true };
    let _ = render(&request, &DocCache::default());
});
