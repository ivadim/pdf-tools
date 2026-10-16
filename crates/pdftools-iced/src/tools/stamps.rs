//! Page numbers and Watermark: options, live previews over the pages, and
//! the stamping jobs.

use std::path::PathBuf;

use iced::widget::image::Handle;
use iced::widget::{button, center, checkbox, column, container, row, slider, text, text_input};
use iced::{Color, Element, Length};
use pdftools_core::stamps::{add_image_stamp, add_page_numbers, ImageStamp, PageNumbers as PageNumbersOptions};
use pdftools_core::{Anchor, HAlign, VAlign};
use pdftools_render::ranges::parse_list;
use pdftools_render::raster::{load_picture, render_text, rotate_picture, Family, Picture};

use super::{set, toggle};
use crate::app::Message;
use crate::catalog::ToolId;
use crate::dialogs::Dialogs;
use crate::icons;
use crate::single::{choose_output, edit, file_name, Job, Overlay, Single, TileSpec, ToolOptions};
use crate::theme;
use crate::widgets::{action, field, muted, section, segmented, BOLD};

/// Six text colours offered by swatches.
const TEXT_COLORS: [[u8; 3]; 6] =
    [[0x00, 0x00, 0x00], [0x1d, 0x2c, 0x6b], [0x2f, 0x5b, 0xd3], [0xc4, 0x3d, 0x3d], [0x23, 0x88, 0x5a], [0x6b, 0x72, 0x83]];
const STAMP_COLORS: [[u8; 3]; 6] =
    [[0xd0, 0x30, 0x30], [0x00, 0x00, 0x00], [0x6b, 0x72, 0x83], [0x2f, 0x5b, 0xd3], [0x23, 0x88, 0x5a], [0xc7, 0x7c, 0x12]];

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}

pub fn parse_anchor(value: &str) -> Anchor {
    let mut parts = value.split_whitespace();
    let v = match parts.next() {
        Some("top") => VAlign::Top,
        Some("middle") => VAlign::Middle,
        _ => VAlign::Bottom,
    };
    let h = match parts.next() {
        Some("left") => HAlign::Left,
        Some("right") => HAlign::Right,
        _ => HAlign::Center,
    };
    Anchor::new(h, v)
}

/// A 3×3 (or 2×3) grid of positions on the page.
fn anchor_picker<'a>(tool: ToolId, anchor: Anchor, middle_row: bool) -> Element<'a, Message> {
    let rows: &[(VAlign, &str)] = if middle_row {
        &[(VAlign::Top, "top"), (VAlign::Middle, "middle"), (VAlign::Bottom, "bottom")]
    } else {
        &[(VAlign::Top, "top"), (VAlign::Bottom, "bottom")]
    };
    let cols = [(HAlign::Left, "left"), (HAlign::Center, "center"), (HAlign::Right, "right")];
    let grid = column(rows.iter().map(|(v, vname)| {
        row(cols.iter().map(|(h, hname)| {
            let chosen = anchor.v == *v && anchor.h == *h;
            let dot = container(iced::widget::space())
                .width(if chosen { 10 } else { 8 })
                .height(if chosen { 10 } else { 8 })
                .style(move |_| container::Style {
                    background: Some((if chosen { theme::ACCENT } else { theme::BORDER }).into()),
                    border: iced::border::rounded(5),
                    ..Default::default()
                });
            button(center(dot))
                .width(40)
                .height(30)
                .padding(0)
                .style(theme::segment(chosen))
                .on_press(Message::Option(tool, "anchor", format!("{vname} {hname}")))
                .into()
        }))
        .spacing(2)
        .into()
    }))
    .spacing(2);
    container(grid).padding(4).style(theme::segmented_track).into()
}

fn swatches<'a>(tool: ToolId, colors: &[[u8; 3]], chosen: [u8; 3]) -> Element<'a, Message> {
    row(colors.iter().enumerate().map(|(i, c)| {
        let selected = *c == chosen;
        let fill = rgb(*c);
        button(iced::widget::space())
            .width(24)
            .height(24)
            .style(move |_, _| button::Style {
                background: Some(fill.into()),
                border: iced::Border {
                    width: if selected { 3.0 } else { 1.0 },
                    color: if selected { theme::ACCENT } else { theme::BORDER },
                    radius: 12.0.into(),
                },
                ..Default::default()
            })
            .on_press(Message::Option(tool, "color", i.to_string()))
            .into()
    }))
    .spacing(6)
    .into()
}

fn labelled_slider<'a>(
    label: &'a str,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    suffix: &'a str,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        row![text(label).size(13).font(BOLD).width(Length::Fill), muted(format!("{}{suffix}", value.round()))],
        slider(range, value, on_change).step(step),
    ]
    .spacing(4)
    .into()
}

// ── Page numbers ─────────────────────────────────────────────────────────

pub struct PageNumbers {
    anchor: Anchor,
    format: String,
    size: f32,
    margin: f32,
    color: [u8; 3],
    custom: bool,
    pages_text: String,
    start_at: String,
}

impl Default for PageNumbers {
    fn default() -> Self {
        Self {
            anchor: Anchor::new(HAlign::Center, VAlign::Bottom),
            format: "{n}".into(),
            size: 11.0,
            margin: 28.0,
            color: TEXT_COLORS[0],
            custom: false,
            pages_text: "2-".into(),
            start_at: "1".into(),
        }
    }
}

/// Characters the built-in PDF font (Helvetica, WinAnsiEncoding) can show.
pub fn win_ansi(text: &str) -> bool {
    text.chars().all(|c| matches!(c, ' '..='~' | '\u{a0}'..='\u{ff}') || "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ".contains(c))
}

impl PageNumbers {
    fn start_at(&self) -> i64 {
        self.start_at.trim().parse().unwrap_or(1)
    }

    /// Original page numbers that get a number; left-out pages never do.
    fn numbered(&self, file: &Single) -> Vec<usize> {
        if !self.custom {
            return file.kept_pages();
        }
        parse_list(&self.pages_text, file.page_count())
            .map(|pages| pages.into_iter().filter(|p| !file.excluded.contains(p)).collect())
            .unwrap_or_default()
    }

    /// The text for an original page, if it's numbered.
    fn label(&self, file: &Single, page: usize) -> Option<String> {
        let numbered = self.numbered(file);
        let i = numbered.iter().position(|p| *p == page)?;
        let last = self.start_at() + numbered.len() as i64 - 1;
        Some(self.format.replace("{n}", &(self.start_at() + i as i64).to_string()).replace("{total}", &last.to_string()))
    }
}

impl ToolOptions for PageNumbers {
    fn id(&self) -> ToolId {
        ToolId::PageNumbers
    }

    fn view<'a>(&'a self, _file: &'a Single) -> Element<'a, Message> {
        let t = ToolId::PageNumbers;
        let presets = ["{n}", "Page {n}", "Page {n} of {total}", "{n} / {total}"];
        let preset_buttons = presets.chunks(2).map(|pair| {
            row(pair.iter().map(|p| action(*p, None, false, Some(Message::Option(t, "format", (*p).into())))))
                .spacing(6)
                .into()
        });
        let mut pages = column![segmented(
            &[("all", "All pages"), ("custom", "Some pages")],
            if self.custom { "custom" } else { "all" },
            move |v| Message::Option(t, "which", v.into()),
        )]
        .spacing(10);
        if self.custom {
            pages = pages.push(column![
                field("Pages to number", text_input("e.g. 2-", &self.pages_text).on_input(set(t, "pages")).padding(8).style(theme::input)),
                muted("“2-” skips the cover page."),
            ]
            .spacing(5));
        }
        column![
            section("Position", [anchor_picker(t, self.anchor, false)]),
            section(
                "Text",
                [
                    column![
                        field("Format", text_input("{n}", &self.format).on_input(set(t, "format")).padding(8).style(theme::input)),
                        muted("{n} is the page number, {total} the last number."),
                    ]
                    .spacing(5)
                    .into(),
                    column(preset_buttons).spacing(6).into(),
                    labelled_slider("Size", self.size, 6.0..=36.0, 1.0, " pt", move |v| Message::Option(t, "size", v.to_string())),
                    labelled_slider("Margin", self.margin, 0.0..=100.0, 2.0, " pt", move |v| Message::Option(t, "margin", v.to_string())),
                    field("Colour", swatches(t, &TEXT_COLORS, self.color)),
                ]
            ),
            section("Pages", [pages.into()]),
            field("First number", text_input("1", &self.start_at).on_input(set(t, "start")).padding(8).style(theme::input)),
        ]
        .spacing(18)
        .into()
    }

    fn set(&mut self, key: &str, value: String, _file: &Single, _dialogs: &dyn Dialogs) {
        match key {
            "anchor" => self.anchor = parse_anchor(&value),
            "format" => self.format = value,
            "size" => self.size = value.parse().unwrap_or(self.size),
            "margin" => self.margin = value.parse().unwrap_or(self.margin),
            "color" => self.color = TEXT_COLORS[value.parse::<usize>().unwrap_or(0).min(TEXT_COLORS.len() - 1)],
            "which" => self.custom = value == "custom",
            "pages" => self.pages_text = value,
            "start" => self.start_at = value,
            _ => {}
        }
    }

    fn tiles(&self, file: &Single) -> Vec<TileSpec> {
        let numbered = self.numbered(file);
        (1..=file.page_count())
            .map(|page| {
                let excluded = file.excluded.contains(&page);
                TileSpec {
                    page,
                    label: format!("p. {page}"),
                    excluded,
                    dimmed: !excluded && !numbered.contains(&page),
                    ..Default::default()
                }
            })
            .collect()
    }

    fn overlay(&self, file: &Single, tile: &TileSpec, (width, height): (f32, f32)) -> Option<Overlay> {
        let text = self.label(file, tile.page)?;
        let (mx, my) = (self.margin / width, self.margin / height);
        let (x, align_x) = match self.anchor.h {
            HAlign::Left => (mx, 0.0),
            HAlign::Center => (0.5, 0.5),
            HAlign::Right => (1.0 - mx, 1.0),
        };
        let (y, align_y) = match self.anchor.v {
            VAlign::Top => (my, 0.0),
            VAlign::Middle => (0.5, 0.5),
            VAlign::Bottom => (1.0 - my, 1.0),
        };
        Some(Overlay::Text { text, x, y, align_x, align_y, size: self.size / width, color: rgb(self.color) })
    }

    fn validate(&self, file: &Single) -> Option<String> {
        if self.format.trim().is_empty() {
            return Some("Enter a format, e.g. Page {n}.".into());
        }
        if !win_ansi(&self.format) {
            return Some("Page numbers support Latin characters only.".into());
        }
        if self.start_at.trim().parse::<i64>().is_err() {
            return Some("The first number must be a whole number.".into());
        }
        if self.custom {
            if let Err(e) = parse_list(&self.pages_text, file.page_count()) {
                return Some(e);
            }
            if self.numbered(file).is_empty() {
                return Some("None of the chosen pages are kept.".into());
            }
        }
        None
    }

    fn run_label(&self) -> &'static str {
        "Add page numbers…"
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let input = file.path()?.to_path_buf();
        let output = choose_output(dialogs, file, "numbered")?;
        let options = PageNumbersOptions {
            anchor: self.anchor,
            margin: self.margin,
            font_size: self.size,
            format: self.format.clone(),
            start_at: self.start_at(),
            // Positions in the document after left-out pages are removed.
            pages: self.custom.then(|| self.numbered(file).into_iter().filter_map(|p| file.final_position(p)).collect()),
            color: self.color.map(|c| c as f32 / 255.0),
        };
        let (keep, note) = (file.keep(), file.removed_note());
        Some(Box::new(move || {
            let count = edit(&input, keep.as_deref(), &output, |doc| add_page_numbers(doc, &options))?;
            let pages = if count == 1 { "1 page".to_owned() } else { format!("{count} pages") };
            Ok((format!("Numbered {pages}{note} in"), Some(output)))
        }))
    }
}

// ── Watermark ────────────────────────────────────────────────────────────

/// Fraction of the page's shorter side kept clear at the edges.
const MARGIN: f32 = 0.05;

pub struct Watermark {
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
    /// The stamp as it will be embedded, and as drawn.
    stamp: Option<(Vec<u8>, Picture)>,
    /// The stamp pre-rotated for previews, and its width relative to the
    /// unrotated stamp.
    rotated: Option<(Handle, f32, f32)>,
}

impl Default for Watermark {
    fn default() -> Self {
        let mut tool = Self {
            image_mode: false,
            text: "CONFIDENTIAL".into(),
            family: Family::Sans,
            bold: true,
            color: STAMP_COLORS[0],
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
}

/// Centre of the stamp as fractions of the page (top-down), mirroring
/// `pdftools_core::stamps::add_image_stamp`.
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

impl Watermark {
    fn refresh_stamp(&mut self) {
        self.image_error = None;
        self.stamp = if self.image_mode {
            match self.image_path.as_deref().map(load_picture) {
                Some(Ok(stamp)) => Some(stamp),
                Some(Err(e)) => {
                    self.image_error = Some(format!("Couldn’t read the image: {e}"));
                    None
                }
                None => None,
            }
        } else {
            render_text(&self.text, self.family, self.bold, self.color, 160.0).map(|p| (p.to_png(), p))
        };
        self.refresh_rotated();
    }

    fn refresh_rotated(&mut self) {
        self.rotated = self.stamp.as_ref().map(|(_, picture)| {
            let rotated = rotate_picture(picture, self.angle);
            let scale = rotated.width as f32 / picture.width.max(1) as f32;
            let aspect = rotated.aspect();
            (Handle::from_rgba(rotated.width, rotated.height, rotated.pixels), scale, aspect)
        });
    }

    fn stamped(&self, file: &Single) -> Option<Vec<usize>> {
        self.custom.then(|| parse_list(&self.pages_text, file.page_count()).unwrap_or_default())
    }
}

impl ToolOptions for Watermark {
    fn id(&self) -> ToolId {
        ToolId::Watermark
    }

    fn view<'a>(&'a self, _file: &'a Single) -> Element<'a, Message> {
        let t = ToolId::Watermark;
        let mut stamp = column![segmented(
            &[("text", "Text"), ("image", "Image")],
            if self.image_mode { "image" } else { "text" },
            move |v| Message::Option(t, "kind", v.into()),
        )]
        .spacing(10);
        if self.image_mode {
            let chosen = self.image_path.as_deref().map(file_name);
            stamp = stamp.push(action(
                chosen.unwrap_or_else(|| "Choose image…".into()),
                Some(icons::image()),
                false,
                Some(Message::Option(t, "choose-image", String::new())),
            ));
            stamp = stamp.push(match &self.image_error {
                Some(e) => text(e.clone()).size(12).color(theme::ERROR).into(),
                None => muted("PNG with transparency works best."),
            });
        } else {
            let family = match self.family {
                Family::Sans => "sans",
                Family::Serif => "serif",
                Family::Mono => "mono",
                Family::Script => "script",
            };
            stamp = stamp
                .push(field("Text", text_input("e.g. DRAFT", &self.text).on_input(set(t, "text")).padding(8).style(theme::input)))
                .push(field(
                    "Font",
                    segmented(
                        &[("sans", "Sans"), ("serif", "Serif"), ("mono", "Mono"), ("script", "Script")],
                        family,
                        move |v| Message::Option(t, "font", v.into()),
                    ),
                ))
                .push(checkbox(self.bold).label("Bold").on_toggle(toggle(t, "bold")))
                .push(field("Colour", swatches(t, &STAMP_COLORS, self.color)));
        }
        let mut pages = column![segmented(
            &[("all", "All pages"), ("custom", "Some pages")],
            if self.custom { "custom" } else { "all" },
            move |v| Message::Option(t, "which", v.into()),
        )]
        .spacing(10);
        if self.custom {
            pages = pages.push(text_input("e.g. 1, 3-5", &self.pages_text).on_input(set(t, "pages")).padding(8).style(theme::input));
        }
        column![
            section("Watermark", [stamp.into()]),
            section(
                "Placement",
                [
                    anchor_picker(t, self.anchor, true),
                    labelled_slider("Size", self.width * 100.0, 5.0..=100.0, 1.0, "% of width", move |v| Message::Option(t, "width", v.to_string())),
                    labelled_slider("Angle", self.angle, -90.0..=90.0, 5.0, "°", move |v| Message::Option(t, "angle", v.to_string())),
                    labelled_slider("Opacity", self.opacity * 100.0, 5.0..=100.0, 5.0, "%", move |v| Message::Option(t, "opacity", v.to_string())),
                    checkbox(self.under).label("Behind page content").on_toggle(toggle(t, "under")).into(),
                ]
            ),
            section("Pages", [pages.into()]),
        ]
        .spacing(18)
        .into()
    }

    fn set(&mut self, key: &str, value: String, _file: &Single, dialogs: &dyn Dialogs) {
        match key {
            "kind" => self.image_mode = value == "image",
            "text" => self.text = value,
            "font" => {
                self.family = match value.as_str() {
                    "serif" => Family::Serif,
                    "mono" => Family::Mono,
                    "script" => Family::Script,
                    _ => Family::Sans,
                }
            }
            "bold" => self.bold = value == "1",
            "color" => self.color = STAMP_COLORS[value.parse::<usize>().unwrap_or(0).min(STAMP_COLORS.len() - 1)],
            "choose-image" => match dialogs.pick_image() {
                Some(path) => self.image_path = Some(path),
                None => return,
            },
            "anchor" => self.anchor = parse_anchor(&value),
            "width" => self.width = value.parse::<f32>().unwrap_or(70.0) / 100.0,
            "angle" => {
                self.angle = value.parse().unwrap_or(45.0);
                self.refresh_rotated();
            }
            "opacity" => self.opacity = value.parse::<f32>().unwrap_or(30.0) / 100.0,
            "under" => self.under = value == "1",
            "which" => self.custom = value == "custom",
            "pages" => self.pages_text = value,
            _ => {}
        }
        if matches!(key, "kind" | "text" | "font" | "bold" | "color" | "choose-image") {
            self.refresh_stamp();
        }
    }

    fn tiles(&self, file: &Single) -> Vec<TileSpec> {
        let stamped = self.stamped(file);
        (1..=file.page_count())
            .map(|page| {
                let excluded = file.excluded.contains(&page);
                TileSpec {
                    page,
                    label: format!("p. {page}"),
                    excluded,
                    dimmed: !excluded && stamped.as_ref().is_some_and(|s| !s.contains(&page)),
                    ..Default::default()
                }
            })
            .collect()
    }

    fn overlay(&self, file: &Single, tile: &TileSpec, page: (f32, f32)) -> Option<Overlay> {
        let ((_, picture), (handle, scale, aspect)) = (self.stamp.as_ref()?, self.rotated.as_ref()?);
        if self.stamped(file).is_some_and(|s| !s.contains(&tile.page)) {
            return None;
        }
        let (x, y) = stamp_center(self.anchor, page, self.width, picture.aspect(), self.angle);
        // The image is already rotated; its bounds are wider than the stamp.
        Some(Overlay::Image { handle: handle.clone(), aspect: *aspect, x, y, width: self.width * scale, opacity: self.opacity })
    }

    fn validate(&self, file: &Single) -> Option<String> {
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
            match parse_list(&self.pages_text, file.page_count()) {
                Err(e) => return Some(e),
                Ok(pages) if pages.iter().all(|p| file.excluded.contains(p)) => {
                    return Some("None of the chosen pages are kept.".into())
                }
                Ok(_) => {}
            }
        }
        None
    }

    fn run_label(&self) -> &'static str {
        "Add watermark…"
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let input = file.path()?.to_path_buf();
        let (bytes, _) = self.stamp.as_ref()?;
        let output = choose_output(dialogs, file, "watermarked")?;
        let stamp = ImageStamp {
            image: bytes.clone(),
            anchor: self.anchor,
            margin: MARGIN,
            width: self.width,
            rotation: self.angle,
            opacity: self.opacity,
            under: self.under,
            // Positions in the document after left-out pages are removed.
            pages: self.stamped(file).map(|pages| pages.into_iter().filter_map(|p| file.final_position(p)).collect()),
        };
        let (keep, note) = (file.keep(), file.removed_note());
        Some(Box::new(move || {
            let count = edit(&input, keep.as_deref(), &output, |doc| add_image_stamp(doc, &stamp))?;
            let pages = if count == 1 { "1 page".to_owned() } else { format!("{count} pages") };
            Ok((format!("Watermarked {pages}{note} in"), Some(output)))
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centres_the_stamp_like_the_backend() {
        let page = (600.0, 800.0);
        assert_eq!(stamp_center(Anchor::new(HAlign::Center, VAlign::Middle), page, 0.5, 2.0, 0.0), (0.5, 0.5));
        let (x, y) = stamp_center(Anchor::new(HAlign::Left, VAlign::Top), page, 0.5, 2.0, 0.0);
        assert!((x - (30.0 + 150.0) / 600.0).abs() < 1e-5);
        assert!((y - (30.0 + 75.0) / 800.0).abs() < 1e-5);
        assert!(win_ansi("Page 1 – café") && !win_ansi("страница"));
        assert_eq!(parse_anchor("top left").h, HAlign::Left);
    }
}
