//! Writes a few sample images for trying out Images to PDF by hand: a wide
//! and a tall JPEG "photo", and a PNG with a transparent background.
//!
//!     cargo run -p pdftools-core --example sample_images -- <output-dir>

use std::path::PathBuf;

use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "samples".into()));
    std::fs::create_dir_all(&dir)?;

    let wide = landscape(1600, 1000, [250, 170, 90], [70, 110, 200]);
    let tall = landscape(900, 1400, [250, 220, 140], [40, 140, 110]);
    let logo = RgbaImage::from_fn(600, 600, |x, y| {
        let (dx, dy) = (x as f32 - 300.0, y as f32 - 300.0);
        if dx * dx + dy * dy < 250.0 * 250.0 {
            Rgba([20, 184, 166, 255])
        } else {
            Rgba([0, 0, 0, 0])
        }
    });
    for (name, image) in [
        ("photo-1-sunset.jpg", DynamicImage::ImageRgb8(wide)),
        ("photo-2-meadow.jpg", DynamicImage::ImageRgb8(tall)),
        ("photo-10-logo.png", DynamicImage::ImageRgba8(logo)),
    ] {
        let path = dir.join(name);
        image.save(&path)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// A sky-to-ground gradient with a sun, so orientation is easy to see.
fn landscape(width: u32, height: u32, sky: [u8; 3], ground: [u8; 3]) -> RgbImage {
    let horizon = height * 2 / 3;
    RgbImage::from_fn(width, height, |x, y| {
        let (dx, dy) = (x as f32 - width as f32 * 0.7, y as f32 - height as f32 * 0.3);
        if dx * dx + dy * dy < (height as f32 * 0.1).powi(2) {
            return Rgb([255, 240, 120]);
        }
        let base = if y < horizon { sky } else { ground };
        let shade = 1.0 - 0.4 * (y as f32 / height as f32);
        Rgb(base.map(|c| (c as f32 * shade) as u8))
    })
}
