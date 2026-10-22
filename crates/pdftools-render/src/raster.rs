//! Producing the images that get stamped into PDFs: text (watermarks, typed
//! signatures) laid out with the system's fonts, drawn signatures, and
//! uploaded pictures. Everything ends up as straight-alpha RGBA plus PNG.

use std::path::Path;

use parley::{Alignment, AlignmentOptions, FontContext, FontFamily, FontWeight, LayoutContext, PositionedLayoutItem, StyleProperty};
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};
use swash::FontRef;

/// Straight (not premultiplied) RGBA pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Picture {
    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height.max(1) as f32
    }

    pub fn to_png(&self) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_raw(self.width, self.height, self.pixels.clone())
            .expect("pixel buffer matches size")
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("PNG encoding to memory");
        bytes.into_inner()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Sans,
    Serif,
    Mono,
    Script,
}

impl Family {
    pub fn from_label(label: &str) -> Self {
        match label {
            "Serif" => Self::Serif,
            "Monospace" => Self::Mono,
            "Handwriting" => Self::Script,
            _ => Self::Sans,
        }
    }

    fn css(self) -> &'static str {
        match self {
            Self::Sans => "\"Segoe UI\", \"Helvetica Neue\", Arial, sans-serif",
            Self::Serif => "Georgia, \"Times New Roman\", serif",
            Self::Mono => "Consolas, \"SF Mono\", \"Courier New\", monospace",
            Self::Script => "\"Segoe Script\", \"Brush Script MT\", \"Snell Roundhand\", \"Lucida Handwriting\", cursive",
        }
    }
}

/// Lays out and draws `text` on a transparent background, tightly padded.
/// Uses the system's fonts with full shaping, so any script works.
pub fn render_text(text: &str, family: Family, bold: bool, color: [u8; 3], size: f32) -> Option<Picture> {
    if text.trim().is_empty() {
        return None;
    }
    let mut fonts = FontContext::new();
    let mut layouts: LayoutContext<()> = LayoutContext::new();
    let mut builder = layouts.ranged_builder(&mut fonts, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(size));
    builder.push_default(StyleProperty::FontFamily(FontFamily::from(family.css())));
    builder.push_default(StyleProperty::FontWeight(if bold { FontWeight::BOLD } else { FontWeight::NORMAL }));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(Alignment::Start, AlignmentOptions::default());

    let pad = (size * 0.12).ceil();
    let width = (layout.width() + 2.0 * pad).ceil() as u32;
    let height = (layout.height() + 2.0 * pad).ceil() as u32;
    if width <= 2 * pad as u32 || height == 0 {
        return None;
    }
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let mut scale = ScaleContext::new();

    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
            let run = glyph_run.run();
            let font = run.font();
            let Some(font_ref) = FontRef::from_index(font.data.as_ref(), font.index as usize) else { continue };
            let mut scaler = scale.builder(font_ref).size(run.font_size()).hint(false).normalized_coords(run.normalized_coords()).build();
            for glyph in glyph_run.positioned_glyphs() {
                let (x, y) = (glyph.x + pad, glyph.y + pad);
                let Some(image) = Render::new(&[Source::ColorOutline(0), Source::ColorBitmap(StrikeWith::BestFit), Source::Outline])
                    .format(Format::Alpha)
                    .offset(Vector::new(x.fract(), y.fract()))
                    .render(&mut scaler, glyph.id as u16)
                else {
                    continue;
                };
                let left = x.floor() as i32 + image.placement.left;
                let top = y.floor() as i32 - image.placement.top;
                let (gw, gh) = (image.placement.width as i32, image.placement.height as i32);
                let channels = image.data.len() / (gw * gh).max(1) as usize;
                for gy in 0..gh {
                    for gx in 0..gw {
                        let (px, py) = (left + gx, top + gy);
                        if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                            continue;
                        }
                        let src = ((gy * gw + gx) as usize) * channels;
                        let coverage = if channels == 4 { image.data[src + 3] } else { image.data[src] };
                        let dst = ((py as u32 * width + px as u32) * 4) as usize;
                        if coverage > pixels[dst + 3] {
                            pixels[dst..dst + 3].copy_from_slice(&color);
                            pixels[dst + 3] = coverage;
                        }
                    }
                }
            }
        }
    }
    Some(Picture { width, height, pixels })
}

/// Draws pen strokes (points in pad pixels) cropped to their extent, at
/// `scale` times the pad resolution.
pub fn render_strokes(strokes: &[Vec<(f32, f32)>], color: [u8; 3], width: f32, scale: f32) -> Option<Picture> {
    let points: Vec<(f32, f32)> = strokes.iter().flatten().copied().collect();
    if points.len() < 2 {
        return None;
    }
    let pad = width * 2.0;
    let min_x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min) - pad;
    let min_y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min) - pad;
    let max_x = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max) + pad;
    let max_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max) + pad;
    let (w, h) = (((max_x - min_x) * scale).ceil() as u32, ((max_y - min_y) * scale).ceil() as u32);
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], 255);
    paint.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width: width * scale,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    };
    let at = |(x, y): (f32, f32)| ((x - min_x) * scale, (y - min_y) * scale);
    for stroke_points in strokes.iter().filter(|s| !s.is_empty()) {
        let mut path = tiny_skia::PathBuilder::new();
        let (x0, y0) = at(stroke_points[0]);
        path.move_to(x0, y0);
        if stroke_points.len() == 1 {
            path.line_to(x0 + 0.01, y0);
        }
        // Smooth through the midpoints of successive samples.
        for pair in stroke_points.windows(2) {
            let (cx, cy) = at(pair[0]);
            let (nx, ny) = at(pair[1]);
            path.quad_to(cx, cy, (cx + nx) / 2.0, (cy + ny) / 2.0);
        }
        let (lx, ly) = at(*stroke_points.last().expect("non-empty"));
        path.line_to(lx, ly);
        if let Some(path) = path.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, tiny_skia::Transform::identity(), None);
        }
    }
    // tiny-skia stores premultiplied pixels.
    let pixels = pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    Some(Picture { width: pixmap.width(), height: pixmap.height(), pixels: pixels.collect() })
}

/// SVG path commands for drawing the strokes in the pad.
pub fn strokes_to_svg(strokes: &[Vec<(f32, f32)>]) -> String {
    let mut out = String::new();
    for stroke in strokes.iter().filter(|s| !s.is_empty()) {
        let (x, y) = stroke[0];
        out.push_str(&format!("M {x:.1} {y:.1} "));
        if stroke.len() == 1 {
            out.push_str(&format!("L {:.1} {y:.1} ", x + 0.1));
        }
        for (x, y) in &stroke[1..] {
            out.push_str(&format!("L {x:.1} {y:.1} "));
        }
    }
    out.trim_end().to_owned()
}

/// Reads a PNG or JPEG file: its bytes (for embedding) and pixels (for previews).
pub fn load_picture(path: &Path) -> Result<(Vec<u8>, Picture), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let decoded = image::load_from_memory(&bytes).map_err(|e| e.to_string())?.to_rgba8();
    let picture = Picture { width: decoded.width(), height: decoded.height(), pixels: decoded.into_raw() };
    Ok((bytes, picture))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inked(p: &Picture) -> usize {
        p.pixels.as_chunks::<4>().0.iter().filter(|px| px[3] > 0).count()
    }

    #[test]
    fn renders_text_in_every_family_and_script() {
        for family in [Family::Sans, Family::Serif, Family::Mono, Family::Script] {
            let picture = render_text("Draft — Черновик", family, true, [200, 30, 30], 48.0).expect("text renders");
            assert!(picture.width > picture.height, "{family:?}");
            assert!(inked(&picture) > 100, "{family:?} drew nothing");
            let png = picture.to_png();
            assert_eq!(&png[1..4], b"PNG");
        }
        assert!(render_text("   ", Family::Sans, false, [0, 0, 0], 20.0).is_none());
        assert_eq!(Family::from_label("Serif"), Family::Serif);
        assert_eq!(Family::from_label("Monospace"), Family::Mono);
        assert_eq!(Family::from_label("Handwriting"), Family::Script);
        assert_eq!(Family::from_label("other"), Family::Sans);
    }

    #[test]
    fn renders_strokes_cropped_to_their_extent() {
        let strokes = vec![vec![(10.0, 10.0), (60.0, 30.0), (110.0, 10.0)], vec![(40.0, 40.0)]];
        let picture = render_strokes(&strokes, [0, 0, 128], 3.0, 2.0).unwrap();
        assert!(picture.width > picture.height);
        assert!(inked(&picture) > 50);
        assert!(render_strokes(&[vec![(1.0, 1.0)]], [0, 0, 0], 3.0, 1.0).is_none());
        assert_eq!(strokes_to_svg(&strokes), "M 10.0 10.0 L 60.0 30.0 L 110.0 10.0 M 40.0 40.0 L 40.1 40.0");
    }

    #[test]
    fn loads_pictures() {
        let dir = std::env::temp_dir().join(format!("pdftools-raster-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.png");
        std::fs::write(&path, Picture { width: 2, height: 1, pixels: vec![255, 0, 0, 255, 0, 0, 0, 0] }.to_png()).unwrap();
        let (bytes, picture) = load_picture(&path).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!((picture.width, picture.height), (2, 1));
        assert_eq!(picture.aspect(), 2.0);
        assert!(load_picture(&dir.join("missing.png")).is_err());
        std::fs::write(dir.join("bad.png"), b"nope").unwrap();
        assert!(load_picture(&dir.join("bad.png")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// Rotates a picture counter-clockwise by `degrees` onto a transparent
/// canvas just large enough to hold it (previews can't rotate images on
/// every renderer, so the stamp preview is pre-rotated).
pub fn rotate_picture(picture: &Picture, degrees: f32) -> Picture {
    if degrees.rem_euclid(360.0) == 0.0 {
        return picture.clone();
    }
    let (w, h) = (picture.width as f32, picture.height as f32);
    let (sin, cos) = degrees.to_radians().sin_cos();
    // The tolerance keeps rounding noise (cos 90° ≈ -4e-8) from adding a pixel.
    let out_w = ((w * cos).abs() + (h * sin).abs() - 1e-3).ceil().max(1.0) as u32;
    let out_h = ((w * sin).abs() + (h * cos).abs() - 1e-3).ceil().max(1.0) as u32;
    // tiny-skia wants premultiplied pixels.
    let premultiplied: Vec<u8> = picture
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = p[3] as u16;
            [(p[0] as u16 * a / 255) as u8, (p[1] as u16 * a / 255) as u8, (p[2] as u16 * a / 255) as u8, p[3]]
        })
        .collect();
    let (Some(source), Some(mut target)) = (
        tiny_skia::Pixmap::from_vec(premultiplied, tiny_skia::IntSize::from_wh(picture.width, picture.height).expect("non-empty")),
        tiny_skia::Pixmap::new(out_w, out_h),
    ) else {
        return picture.clone();
    };
    // Screen y points down, so counter-clockwise is a negative angle here.
    let transform =
        tiny_skia::Transform::from_translate(out_w as f32 / 2.0, out_h as f32 / 2.0).pre_rotate(-degrees).pre_translate(-w / 2.0, -h / 2.0);
    let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
    target.draw_pixmap(0, 0, source.as_ref(), &paint, transform, None);
    let pixels = target.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    Picture { width: out_w, height: out_h, pixels: pixels.collect() }
}

#[cfg(test)]
mod rotate_tests {
    use super::*;

    #[test]
    fn rotated_pictures_fit_their_bounds() {
        let picture = Picture { width: 40, height: 10, pixels: vec![255; 40 * 10 * 4] };
        let quarter = rotate_picture(&picture, 90.0);
        assert_eq!((quarter.width, quarter.height), (10, 40));
        let tilted = rotate_picture(&picture, 45.0);
        assert_eq!(tilted.width, tilted.height);
        assert!(tilted.pixels.as_chunks::<4>().0.iter().any(|p| p[3] == 0), "corners are transparent");
        assert_eq!(rotate_picture(&picture, 0.0), picture);
    }
}
