//! Writes a few sample PDFs for trying out the tools by hand.
//!
//!     cargo run -p pdftools-core --example sample_pdfs -- <output-dir>

use std::path::PathBuf;

use lopdf::{dictionary, Document, Object, Stream};

/// (file name, page count, page size in points, background colour).
const SAMPLES: [(&str, usize, (i64, i64), (f32, f32, f32)); 4] = [
    ("chapter-1.pdf", 3, (595, 842), (0.85, 0.90, 1.0)),
    ("chapter-2.pdf", 5, (595, 842), (1.0, 0.88, 0.86)),
    ("chapter-10.pdf", 2, (595, 842), (0.86, 0.96, 0.89)),
    ("wide-chart.pdf", 2, (842, 595), (1.0, 0.95, 0.82)),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "samples".into()));
    std::fs::create_dir_all(&dir)?;
    for (name, pages, size, color) in SAMPLES {
        let path = dir.join(name);
        sample(name.trim_end_matches(".pdf"), pages, size, color).save(&path)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn sample(title: &str, pages: usize, (width, height): (i64, i64), (r, g, b): (f32, f32, f32)) -> Document {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut kids: Vec<Object> = Vec::new();
    for n in 1..=pages {
        let content = format!(
            "{r} {g} {b} rg 0 0 {width} {height} re f \
             0.15 0.18 0.25 rg BT /F1 40 Tf 60 {top} Td ({title}) Tj ET \
             BT /F1 160 Tf 60 {mid} Td ({n}) Tj ET",
            top = height - 110,
            mid = height / 2 - 60,
        );
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        kids.push(
            doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
            })
            .into(),
        );
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => pages as i64,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc.compress();
    doc
}
