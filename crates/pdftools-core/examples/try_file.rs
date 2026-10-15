//! Runs the single-file operations on any PDF and reports sizes and timings.
//!
//!     cargo run --release -p pdftools-core --example try_file -- <input.pdf> <output-dir>

use std::path::{Path, PathBuf};
use std::time::Instant;

use pdftools_core::compress::{compress_file, Level};
use pdftools_core::grayscale::grayscale_file;
use pdftools_core::organize::organize_file;
use pdftools_core::security::{protect_file, unlock_file, Protection};
use pdftools_core::stamps::{add_page_numbers_file, PageNumbers};
use pdftools_core::{Anchor, HAlign, PageSpec, VAlign};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = PathBuf::from(args.next().ok_or("usage: try_file <input.pdf> <output-dir>")?);
    let out = PathBuf::from(args.next().unwrap_or_else(|| "target/try".into()));
    std::fs::create_dir_all(&out)?;

    let info = pdftools_core::inspect(&input)?;
    println!("{}: {} pages, {}", input.display(), info.pages.unwrap_or(0), mb(info.file_size));
    let pages = info.pages.unwrap_or(0);

    time("compress (balanced)", &out.join("compressed.pdf"), || {
        let summary = compress_file(&input, Level::Balanced, &out.join("compressed.pdf"))?;
        Ok(format!("{} images re-encoded", summary.images_recompressed))
    });
    time("grayscale", &out.join("grayscale.pdf"), || {
        Ok(format!("{:?}", grayscale_file(&input, true, &out.join("grayscale.pdf"))?))
    });
    time("page numbers", &out.join("numbered.pdf"), || {
        let options = PageNumbers {
            anchor: Anchor::new(HAlign::Center, VAlign::Bottom),
            margin: 24.0,
            font_size: 12.0,
            format: "Page {n} of {total}".into(),
            start_at: 1,
            pages: None,
            color: [0.0, 0.0, 0.0],
        };
        Ok(format!("{} numbered", add_page_numbers_file(&input, &options, &out.join("numbered.pdf"))?))
    });
    time("reverse pages", &out.join("reversed.pdf"), || {
        let order: Vec<PageSpec> = (1..=pages).rev().map(|p| PageSpec::new(0, p)).collect();
        Ok(format!("{} pages", organize_file(&input, &order, &out.join("reversed.pdf"))?))
    });
    time("protect + unlock", &out.join("unlocked.pdf"), || {
        protect_file(&input, &Protection::with_password("pw"), &out.join("protected.pdf"))?;
        unlock_file(&out.join("protected.pdf"), "pw", &out.join("unlocked.pdf"))?;
        let check = pdftools_core::inspect(&out.join("unlocked.pdf"))?;
        Ok(format!("{} pages after round trip", check.pages.unwrap_or(0)))
    });
    Ok(())
}

fn time(label: &str, output: &Path, run: impl FnOnce() -> Result<String, pdftools_core::Error>) {
    let start = Instant::now();
    match run() {
        Ok(detail) => println!(
            "  {label:<20} {:>7.2}s  {:>9}  {detail}",
            start.elapsed().as_secs_f64(),
            mb(std::fs::metadata(output).map(|m| m.len()).unwrap_or(0))
        ),
        Err(e) => println!("  {label:<20} FAILED: {e}"),
    }
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}
