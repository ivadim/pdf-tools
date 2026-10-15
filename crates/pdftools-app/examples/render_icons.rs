//! Renders `packaging/icon.svg` to the icon files the app ships: a 1024 px
//! PNG (made into the macOS `.icns` by CI), a 256 px PNG for the window and
//! a multi-size Windows `.ico`. Run after changing the SVG:
//!
//!     cargo run -p pdftools-app --example render_icons -- packaging
use std::path::Path;

fn render(svg: &resvg::usvg::Tree, size: u32) -> image::RgbaImage {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).unwrap();
    let scale = size as f32 / svg.size().width();
    resvg::render(svg, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let pixels = pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    image::RgbaImage::from_raw(size, size, pixels.collect()).unwrap()
}

fn main() {
    let dir = Path::new(&std::env::args().nth(1).expect("packaging dir")).to_path_buf();
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let svg = resvg::usvg::Tree::from_data(&std::fs::read(dir.join("icon.svg")).unwrap(), &options).unwrap();
    render(&svg, 1024).save(dir.join("icon.png")).unwrap();
    render(&svg, 256).save(dir.join("icon-256.png")).unwrap();
    // Windows: one PNG-compressed image per common size.
    let frames: Vec<_> = [16, 24, 32, 48, 64, 128, 256]
        .into_iter()
        .map(|s| {
            let img = render(&svg, s);
            image::codecs::ico::IcoFrame::as_png(img.as_raw(), s, s, image::ExtendedColorType::Rgba8).unwrap()
        })
        .collect();
    let file = std::fs::File::create(dir.join("icon.ico")).unwrap();
    image::codecs::ico::IcoEncoder::new(file).encode_images(&frames).unwrap();
    println!("rendered icons into {}", dir.display());
}
