//! Runs every operation on the sample PDFs, for checking results by eye.
//!
//!     cargo run -p pdftools-core --example sample_pdfs -- target/samples
//!     cargo run -p pdftools-core --example smoke -- target/samples target/smoke

use std::path::PathBuf;

use pdftools_core::compress::{compress_file, Level};
use pdftools_core::grayscale::grayscale_file;
use pdftools_core::organize::{organize_file, split_file, SplitPart};
use pdftools_core::security::{protect_file, unlock_file, Protection};
use pdftools_core::stamps::{add_image_stamp_file, add_page_numbers_file, add_signature_file, ImageStamp, PageNumbers, Placement};
use pdftools_core::{metadata, Anchor, HAlign, Metadata, PageSpec, VAlign};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let samples = PathBuf::from(args.next().unwrap_or_else(|| "target/samples".into()));
    let out = PathBuf::from(args.next().unwrap_or_else(|| "target/smoke".into()));
    std::fs::create_dir_all(&out)?;
    let chapter = samples.join("chapter-2.pdf");
    let wide = samples.join("wide-chart.pdf");
    let logo = logo_png();

    let rotated = out.join("rotated.pdf");
    organize_file(&chapter, &[PageSpec::new(0, 1).rotated(90), PageSpec::new(0, 2), PageSpec::new(0, 3).rotated(180)], &rotated)?;

    split_file(
        &chapter,
        &[
            SplitPart { pages: vec![PageSpec::new(0, 1), PageSpec::new(0, 2)], output: out.join("split-1.pdf") },
            SplitPart { pages: vec![PageSpec::new(0, 5)], output: out.join("split-2.pdf") },
        ],
        |_| {},
    )?;

    let numbers = PageNumbers {
        anchor: Anchor::new(HAlign::Right, VAlign::Bottom),
        margin: 28.0,
        font_size: 18.0,
        format: "Page {n} of {total}".into(),
        start_at: 1,
        pages: None,
        color: [0.1, 0.2, 0.6],
    };
    add_page_numbers_file(&chapter, &numbers, &out.join("numbered.pdf"))?;
    // Numbers must stay upright and in the corner on rotated pages too.
    add_page_numbers_file(&rotated, &numbers, &out.join("numbered-rotated.pdf"))?;

    let stamp = ImageStamp {
        image: logo.clone(),
        anchor: Anchor::new(HAlign::Center, VAlign::Middle),
        margin: 0.05,
        width: 0.6,
        rotation: 30.0,
        opacity: 0.35,
        under: false,
        pages: None,
    };
    add_image_stamp_file(&wide, &stamp, &out.join("watermarked.pdf"))?;
    add_image_stamp_file(&rotated, &stamp, &out.join("watermarked-rotated.pdf"))?;

    let signature = [Placement { page: 1, x: 0.6, y: 0.82, width: 0.3, height: 0.08 }];
    add_signature_file(&chapter, &logo, &signature, &out.join("signed.pdf"))?;
    add_signature_file(&rotated, &logo, &signature, &out.join("signed-rotated.pdf"))?;

    let summary = grayscale_file(&chapter, true, &out.join("grayscale.pdf"))?;
    println!("grayscale: {summary:?}");

    let compressed = compress_file(&chapter, Level::Strong, &out.join("compressed.pdf"))?;
    println!("compress: {compressed:?}");

    protect_file(&chapter, &Protection::with_password("secret"), &out.join("protected.pdf"))?;
    unlock_file(&out.join("protected.pdf"), "secret", &out.join("unlocked.pdf"))?;

    metadata::write_file(
        &chapter,
        &Metadata { title: Some("Smoke test".into()), author: Some("PDF Tools".into()), ..Default::default() },
        &out.join("metadata.pdf"),
    )?;
    println!("metadata: {:?}", pdftools_core::inspect(&out.join("metadata.pdf"))?.metadata);

    println!("wrote results to {}", out.display());
    Ok(())
}

/// A semi-transparent green badge, standing in for a logo or signature.
fn logo_png() -> Vec<u8> {
    let image = image::RgbaImage::from_fn(300, 100, |x, y| {
        let border = x < 6 || y < 6 || x >= 294 || y >= 94;
        let stripe = (x / 20) % 2 == 0;
        match (border, stripe) {
            (true, _) => image::Rgba([20, 120, 60, 255]),
            (false, true) => image::Rgba([40, 170, 90, 200]),
            (false, false) => image::Rgba([0, 0, 0, 0]),
        }
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}
