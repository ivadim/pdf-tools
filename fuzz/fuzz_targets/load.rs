//! Opening a file as the tools do: `inspect` first (page count, metadata,
//! images), then `load`, both recovering what they can from damaged files.
#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "common.rs"]
mod common;

fuzz_target!(|data: &[u8]| {
    let path = common::as_file(data);
    let _ = pdftools_core::inspect(path);
    let _ = pdftools_core::load(path);
    let _ = pdftools_core::repaired_bytes(data);
});
