//! Regenerate desktop, mobile and web icons from the full-bleed master:
//!
//!     bazel run //crates/pdftools-app:render_icons -- "$PWD"
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

fn parse(data: &str) -> resvg::usvg::Tree {
    resvg::usvg::Tree::from_data(data.as_bytes(), &resvg::usvg::Options::default()).unwrap()
}

fn main() {
    let root = std::env::args().nth(1).expect("repository root");
    let root = Path::new(&root);
    let source = std::fs::read_to_string(root.join("packaging/icon.svg")).unwrap();
    let full_bleed = parse(&source);
    // macOS uses an inset tile; iOS and the web apply their own outer masks.
    let desktop = parse(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
        <defs><clipPath id="desktop-tile"><rect x="100" y="100" width="824" height="824" rx="186"/></clipPath></defs>
        <g clip-path="url(#desktop-tile)"><g transform="translate(100 100) scale(0.8046875)">{source}</g></g></svg>"#
    ));
    // Keep the complete foreground inside Android's circular 66/108 safe zone.
    assert_eq!(source.matches("id=\"background\"").count(), 1, "master needs one background layer");
    let transparent = source.replace("id=\"background\"", "id=\"background\" display=\"none\"");
    let foreground = parse(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
        <g transform="translate(163.84 163.84) scale(0.68)">{transparent}</g></svg>"#
    ));

    render(&desktop, 1024).save(root.join("packaging/icon.png")).unwrap();
    render(&desktop, 256).save(root.join("packaging/icon-256.png")).unwrap();
    let frames: Vec<_> = [16, 24, 32, 48, 64, 128, 256]
        .into_iter()
        .map(|s| {
            let img = render(&desktop, s);
            image::codecs::ico::IcoFrame::as_png(img.as_raw(), s, s, image::ExtendedColorType::Rgba8).unwrap()
        })
        .collect();
    let file = std::fs::File::create(root.join("packaging/icon.ico")).unwrap();
    image::codecs::ico::IcoEncoder::new(file).encode_images(&frames).unwrap();
    // A fully opaque RGBA file still has an alpha channel. Export RGB for iOS.
    image::DynamicImage::ImageRgba8(render(&full_bleed, 1024))
        .into_rgb8()
        .save(root.join("ios/PDFTools/Assets.xcassets/AppIcon.appiconset/icon.png"))
        .unwrap();
    for size in [192, 512] {
        render(&full_bleed, size).save(root.join(format!("web/icon-{size}.png"))).unwrap();
    }
    for (density, size) in [("mdpi", 108), ("hdpi", 162), ("xhdpi", 216), ("xxhdpi", 324), ("xxxhdpi", 432)] {
        render(&foreground, size).save(root.join(format!("android/res/mipmap-{density}/ic_launcher_foreground.png"))).unwrap();
    }
    println!("Rendered platform icons from packaging/icon.svg");
}
