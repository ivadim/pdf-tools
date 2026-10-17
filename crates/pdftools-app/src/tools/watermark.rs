use std::path::PathBuf;

use pdftools_core::stamps::{add_image_stamp, ImageStamp};
use pdftools_core::{Anchor, HAlign, VAlign};

use super::page_numbers::parse_anchor;
use super::{choose_output, edit, file_name, Job, Outcome, Shared, SingleFile, Tool};
use crate::ToImage;
use crate::ranges::parse_list;
use crate::raster::{load_picture, render_text, Family, Picture};
use crate::{single_file_plumbing, PageTile};

const COLORS: [[u8; 3]; 6] =
    [[0xd0, 0x30, 0x30], [0x00, 0x00, 0x00], [0x6b, 0x72, 0x83], [0x2f, 0x5b, 0xd3], [0x23, 0x88, 0x5a], [0xc7, 0x7c, 0x12]];

/// Fraction of the page's shorter side kept clear at the edges.
const MARGIN: f32 = 0.05;

pub struct WatermarkTool {
    base: SingleFile,
    image_mode: bool,
    text: String,
    family: Family,
    bold: bool,
    color: [u8; 3],
    image_path: Option<PathBuf>,
    image_error: Option<String>,
    anchor: Anchor,
    width: f32,
    angle: f32,
    opacity: f32,
    under: bool,
    custom: bool,
    pages_text: String,
    /// The stamp as it will be embedded, shared by preview and output.
    stamp: Option<(Vec<u8>, Picture, slint::Image)>,
    /// The stamp pre-rotated for previews: image and its width relative to
    /// the unrotated stamp.
    rotated: Option<(slint::Image, f32)>,
}

impl WatermarkTool {
    pub fn new() -> Self {
        let mut tool = Self {
            base: SingleFile::new(super::id::WATERMARK),
            image_mode: false,
            text: "CONFIDENTIAL".into(),
            family: Family::Sans,
            bold: true,
            color: COLORS[0],
            image_path: None,
            image_error: None,
            anchor: Anchor::new(HAlign::Center, VAlign::Middle),
            width: 0.7,
            angle: 45.0,
            opacity: 0.3,
            under: false,
            custom: false,
            pages_text: "1".into(),
            stamp: None,
            rotated: None,
        };
        tool.refresh_stamp();
        tool
    }

    fn refresh_stamp(&mut self) {
        self.image_error = None;
        let picture = if self.image_mode {
            match self.image_path.as_deref().map(load_picture) {
                Some(Ok((bytes, picture))) => Some((bytes, picture)),
                Some(Err(e)) => {
                    self.image_error = Some(format!("Couldn’t read the image: {e}"));
                    None
                }
                None => None,
            }
        } else {
            render_text(&self.text, self.family, self.bold, self.color, 160.0).map(|p| (p.to_png(), p))
        };
        self.stamp = picture.map(|(bytes, picture)| {
            let image = picture.to_image();
            (bytes, picture, image)
        });
        self.refresh_rotated();
    }

    fn refresh_rotated(&mut self) {
        self.rotated = self.stamp.as_ref().map(|(_, picture, _)| {
            let rotated = crate::raster::rotate_picture(picture, self.angle);
            let scale = rotated.width as f32 / picture.width.max(1) as f32;
            (rotated.to_image(), scale)
        });
    }

    fn stamped(&self) -> Option<Vec<usize>> {
        self.custom.then(|| parse_list(&self.pages_text, self.base.page_count()).unwrap_or_default())
    }
}

impl Default for WatermarkTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Centre of the stamp as fractions of the page (top-down), mirroring the
/// layout of `pdftools_core::stamps::add_image_stamp`.
pub fn stamp_center(anchor: Anchor, page: (f32, f32), width: f32, aspect: f32, angle: f32) -> (f32, f32) {
    let (pw, ph) = page;
    let w = width * pw;
    let h = w / aspect;
    let (sin, cos) = angle.to_radians().sin_cos();
    let bound_w = (w * cos).abs() + (h * sin).abs();
    let bound_h = (w * sin).abs() + (h * cos).abs();
    let margin = MARGIN * pw.min(ph);
    let x = match anchor.h {
        HAlign::Left => margin + bound_w / 2.0,
        HAlign::Center => pw / 2.0,
        HAlign::Right => pw - margin - bound_w / 2.0,
    };
    let y = match anchor.v {
        VAlign::Top => margin + bound_h / 2.0,
        VAlign::Middle => ph / 2.0,
        VAlign::Bottom => ph - margin - bound_h / 2.0,
    };
    (x / pw, y / ph)
}

impl Tool for WatermarkTool {
    single_file_plumbing!();

    fn option(&mut self, s: &mut Shared, key: &str, value: &str) {
        match key {
            "kind" => self.image_mode = value == "image",
            "text" => self.text = value.into(),
            "font" => self.family = Family::from_label(value),
            "bold" => self.bold = value == "1",
            "color" => self.color = COLORS[value.parse::<usize>().unwrap_or(0).min(COLORS.len() - 1)],
            "choose-image" => match s.dialogs.pick_image() {
                Some(path) => self.image_path = Some(path),
                None => return,
            },
            "anchor" => {
                self.anchor = parse_anchor(value);
                return;
            }
            "width" => self.width = value.parse::<f32>().unwrap_or(70.0) / 100.0,
            "angle" => {
                self.angle = value.parse().unwrap_or(45.0);
                self.refresh_rotated();
            }
            "opacity" => self.opacity = value.parse::<f32>().unwrap_or(30.0) / 100.0,
            "under" => self.under = value == "1",
            "which" => self.custom = value == "custom",
            "pages" => self.pages_text = value.into(),
            _ => {}
        }
        if matches!(key, "kind" | "text" | "font" | "bold" | "color" | "choose-image") {
            self.refresh_stamp();
        }
    }

    /// [0] chosen image name, [1] image error.
    fn texts(&self) -> Vec<String> {
        vec![self.image_path.as_deref().map(file_name).unwrap_or_default(), self.image_error.clone().unwrap_or_default()]
    }

    fn tiles(&self) -> Vec<PageTile> {
        let stamped = self.stamped();
        self.base
            .base_tiles()
            .into_iter()
            .map(|t| PageTile {
                dimmed: !t.excluded && stamped.as_ref().is_some_and(|s| !s.contains(&(t.page as usize))),
                ..t
            })
            .collect()
    }

    fn decorate(&self, tile: &mut PageTile, page: (f32, f32)) {
        let (Some((_, picture, _)), Some((image, scale))) = (&self.stamp, &self.rotated) else { return };
        if self.stamped().is_some_and(|s| !s.contains(&(tile.page as usize))) {
            return;
        }
        let (x, y) = stamp_center(self.anchor, page, self.width, picture.aspect(), self.angle);
        tile.overlay_image = image.clone();
        tile.has_overlay_image = true;
        tile.overlay_image_x = x;
        tile.overlay_image_y = y;
        // The image is already rotated; its bounds are wider than the stamp.
        tile.overlay_image_width = self.width * scale;
        tile.overlay_image_angle = 0.0;
        tile.overlay_image_opacity = self.opacity;
    }

    fn run_label(&self) -> String {
        "Add watermark…".into()
    }

    fn validate(&self) -> Option<String> {
        if !self.image_mode && self.text.trim().is_empty() {
            return Some("Enter the watermark text.".into());
        }
        if self.image_mode && self.image_path.is_none() {
            return Some("Choose an image.".into());
        }
        if self.stamp.is_none() {
            return Some(self.image_error.clone().unwrap_or_else(|| "Preparing the watermark…".into()));
        }
        if self.custom {
            match parse_list(&self.pages_text, self.base.page_count()) {
                Err(e) => return Some(e),
                Ok(pages) if pages.iter().all(|p| self.base.excluded.contains(p)) => {
                    return Some("None of the chosen pages are kept.".into())
                }
                Ok(_) => {}
            }
        }
        None
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let (bytes, _, _) = self.stamp.as_ref()?;
        let output = choose_output(&*s.dialogs, &input, "watermarked")?;
        let stamp = ImageStamp {
            image: bytes.clone(),
            anchor: self.anchor,
            margin: MARGIN,
            width: self.width,
            rotation: self.angle,
            opacity: self.opacity,
            under: self.under,
            // Positions in the document after left-out pages are removed.
            pages: self.stamped().map(|pages| pages.into_iter().filter_map(|p| self.base.final_position(p)).collect()),
        };
        let (keep, note) = (self.base.keep(), self.base.removed_note());
        Some(Box::new(move |_| {
            let count = edit(&input, keep.as_deref(), &output, |doc| add_image_stamp(doc, &stamp))?;
            let pages = if count == 1 { "1 page".to_owned() } else { format!("{count} pages") };
            Ok(Outcome { message: format!("Watermarked {pages}{note} in"), output: Some(output) })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centres_the_stamp_like_the_backend() {
        let page = (600.0, 800.0);
        let centre = stamp_center(Anchor::new(HAlign::Center, VAlign::Middle), page, 0.5, 2.0, 0.0);
        assert_eq!(centre, (0.5, 0.5));
        // Unrotated 300×150 box, 30pt margin, top-left.
        let (x, y) = stamp_center(Anchor::new(HAlign::Left, VAlign::Top), page, 0.5, 2.0, 0.0);
        assert!((x - (30.0 + 150.0) / 600.0).abs() < 1e-5);
        assert!((y - (30.0 + 75.0) / 800.0).abs() < 1e-5);
        let (x, _) = stamp_center(Anchor::new(HAlign::Right, VAlign::Bottom), page, 0.5, 2.0, 90.0);
        assert!((x - (600.0 - 30.0 - 75.0) / 600.0).abs() < 1e-4, "rotated box is anchored by its bounds");
    }
}
