//! Images to PDF: JPEG and PNG files, one per page, in an order the user
//! chooses, each turned as needed. Previews show each image placed on a
//! blank page of the chosen size, so layout changes show instantly.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use iced::widget::image::Handle;
use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Length, Padding};
use image::imageops::FilterType;
use image::RgbaImage;
use pdftools_core::images::{ImageInfo, ImageLayout, ImagePage, PageOrientation, PageSize};

use crate::app::Message;
use crate::catalog::{self, ToolId};
use crate::icons;
use crate::merge::{natural_key, Context, Effect};
use crate::single::{file_name, plural, Overlay};
use crate::theme;
use crate::tools::move_item;
use crate::widgets::{self, action, muted, run_bar, section, section_title, segmented, Caps, Run, Tile, TileEvent};

/// Longest side of preview thumbnails, in pixels.
const THUMB_SIZE: u32 = 360;
/// Margins offered, in points: none, small (¼ inch), large (½ inch).
const MARGINS: [(&str, f32); 3] = [("none", 0.0), ("small", 18.0), ("large", 36.0)];

pub type ReadResult = Result<(ImageInfo, RgbaImage), String>;

#[derive(Debug, Clone)]
pub enum Msg {
    Add,
    Read(u32, ReadResult),
    Sort,
    Clear,
    Option(&'static str, &'static str),
    Tile(usize, TileEvent),
    RotateAll(i32),
    Run,
    Finished(Result<(String, Option<PathBuf>), String>),
    Reveal,
}

/// Reads an image's size and a small upright thumbnail (off the UI thread).
pub fn read_image(path: &Path) -> ReadResult {
    let info = pdftools_core::images::image_info(path).map_err(|e| e.to_string())?;
    let image = pdftools_core::images::load_image(path, 0).map_err(|e| e.to_string())?;
    Ok((info, image.resize(THUMB_SIZE, THUMB_SIZE, FilterType::Triangle).to_rgba8()))
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageState {
    Loading,
    Ready(ImageInfo),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub id: u32,
    pub path: PathBuf,
    pub name: String,
    pub state: ImageState,
    /// Clockwise, on top of the rotation recorded in the file.
    pub rotation: i32,
}

pub struct Images {
    pub entries: Vec<ImageEntry>,
    next_id: u32,
    pub layout: ImageLayout,
    pub run: Run,
    unread: Vec<(u32, PathBuf)>,
    reading: bool,
    /// Upright thumbnails by entry id.
    thumbs: HashMap<u32, RgbaImage>,
    /// Thumbnails turned for display, by entry id and rotation (filled in
    /// while drawing, hence the cell).
    shown: RefCell<HashMap<(u32, i32), Handle>>,
}

impl Default for Images {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 0,
            layout: ImageLayout { margin: 0.0, ..Default::default() },
            run: Run::Idle,
            unread: Vec::new(),
            reading: false,
            thumbs: HashMap::new(),
            shown: RefCell::new(HashMap::new()),
        }
    }
}

impl Images {
    /// Adds JPEG and PNG files; anything else is ignored.
    pub fn add_files(&mut self, paths: Vec<PathBuf>) -> Option<Effect> {
        for path in paths.into_iter().filter(|p| is_image(p)) {
            self.next_id += 1;
            let id = self.next_id;
            self.unread.push((id, path.clone()));
            self.entries.push(ImageEntry { id, name: file_name(&path), path, state: ImageState::Loading, rotation: 0 });
        }
        if !self.run.busy() {
            self.run = Run::Idle;
        }
        self.next_read()
    }

    /// Images are read one at a time, so large photos are never decoded all
    /// at once.
    fn next_read(&mut self) -> Option<Effect> {
        if self.reading || self.unread.is_empty() {
            return None;
        }
        self.reading = true;
        let (id, path) = self.unread.remove(0);
        Some(Effect::ReadImage(id, path))
    }

    fn rotate(&mut self, index: Option<usize>, delta: i32) {
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if index.is_none_or(|index| index == i) {
                entry.rotation = (entry.rotation + delta).rem_euclid(360);
            }
        }
    }

    /// The thumbnail turned by `rotation`, cached.
    fn thumb(&self, id: u32, rotation: i32) -> Option<Handle> {
        if let Some(handle) = self.shown.borrow().get(&(id, rotation)) {
            return Some(handle.clone());
        }
        let thumb = self.thumbs.get(&id)?;
        let turned = match rotation {
            90 => image::imageops::rotate90(thumb),
            180 => image::imageops::rotate180(thumb),
            270 => image::imageops::rotate270(thumb),
            _ => thumb.clone(),
        };
        let handle = Handle::from_rgba(turned.width(), turned.height(), turned.into_raw());
        self.shown.borrow_mut().insert((id, rotation), handle.clone());
        Some(handle)
    }

    /// The image behind tile `index` and its rotation, for the viewer.
    pub fn source(&self, index: usize) -> Option<(PathBuf, i32)> {
        self.entries.get(index).map(|e| (e.path.clone(), e.rotation))
    }

    pub fn validate(&self) -> Option<String> {
        let failed = self.entries.iter().filter(|e| matches!(e.state, ImageState::Failed(_))).count();
        if failed > 0 {
            return Some(format!("Remove {} can’t be read to continue.", if failed == 1 { "the image that" } else { "images that" }));
        }
        if self.entries.iter().any(|e| e.state == ImageState::Loading) {
            return Some("Reading images…".into());
        }
        if self.entries.is_empty() {
            return Some("Add images to get started.".into());
        }
        None
    }

    pub fn update(&mut self, msg: Msg, ctx: &mut Context) -> Option<Effect> {
        let effect = match msg {
            Msg::Add => self.add_files(ctx.dialogs.pick_images()),
            Msg::Read(id, result) => {
                self.reading = false;
                if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
                    entry.state = match result {
                        Ok((info, thumb)) => {
                            self.thumbs.insert(id, thumb);
                            ImageState::Ready(info)
                        }
                        Err(message) => ImageState::Failed(message),
                    };
                }
                self.next_read()
            }
            Msg::Sort => {
                self.entries.sort_by_key(|e| natural_key(&e.name));
                None
            }
            Msg::Clear => {
                let (next_id, layout) = (self.next_id, self.layout);
                *self = Self { next_id, layout, ..Self::default() };
                None
            }
            Msg::Option(key, value) => {
                match key {
                    "size" => {
                        self.layout.page_size = match value {
                            "letter" => PageSize::Letter,
                            "fit" => PageSize::FitImage,
                            _ => PageSize::A4,
                        }
                    }
                    "orientation" => {
                        self.layout.orientation = match value {
                            "portrait" => PageOrientation::Portrait,
                            "landscape" => PageOrientation::Landscape,
                            _ => PageOrientation::Auto,
                        }
                    }
                    "margin" => self.layout.margin = MARGINS.iter().find(|(n, _)| *n == value).map_or(0.0, |m| m.1),
                    _ => {}
                }
                None
            }
            Msg::Tile(index, event) => match event {
                TileEvent::View => Some(Effect::View(index)),
                TileEvent::RotateLeft => {
                    self.rotate(Some(index), -90);
                    None
                }
                TileEvent::RotateRight => {
                    self.rotate(Some(index), 90);
                    None
                }
                TileEvent::Delete => {
                    if index < self.entries.len() {
                        let entry = self.entries.remove(index);
                        self.thumbs.remove(&entry.id);
                        self.shown.borrow_mut().retain(|(id, _), _| *id != entry.id);
                    }
                    None
                }
                TileEvent::MoveEarlier if index > 0 => {
                    move_item(&mut self.entries, index, index - 1);
                    None
                }
                TileEvent::MoveLater => {
                    move_item(&mut self.entries, index, index + 2);
                    None
                }
                _ => None,
            },
            Msg::RotateAll(delta) => {
                self.rotate(None, delta);
                None
            }
            Msg::Run => {
                if self.validate().is_some() || self.run.busy() {
                    return None;
                }
                let first = &self.entries.first()?.path;
                let stem = first.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let name = if self.entries.len() == 1 { format!("{stem}.pdf") } else { "images.pdf".into() };
                let output = ctx.dialogs.save_pdf("Save PDF", &first.with_file_name(name))?;
                let images: Vec<ImagePage> = self.entries.iter().map(|e| ImagePage::new(&e.path).rotated(e.rotation)).collect();
                let layout = self.layout;
                self.run = Run::Running("Adding images…".into());
                Some(Effect::Job(Box::new(move || {
                    let pages = pdftools_core::images::images_to_pdf_file(&images, &layout, &output, |_| {}).map_err(|e| e.to_string())?;
                    Ok((format!("Saved {} to", plural(pages, "page")), Some(output)))
                })))
            }
            Msg::Finished(result) => {
                self.run = match result {
                    Ok((message, output)) => Run::Done { message, output },
                    Err(message) => Run::Failed(message),
                };
                return None;
            }
            Msg::Reveal => {
                if let Run::Done { output: Some(output), .. } = &self.run {
                    crate::dialogs::reveal(output);
                }
                return None;
            }
        };
        if matches!(self.run, Run::Done { .. } | Run::Failed(_)) && self.validate().is_some() {
            self.run = Run::Idle;
        }
        effect
    }

    pub fn tiles(&self) -> Vec<Tile> {
        (0..self.entries.len())
            .map(|i| {
                let (id, rotation) = (self.entries[i].id, self.entries[i].rotation);
                let handle = self.thumb(id, rotation);
                let entry = &self.entries[i];
                let mut tile = Tile {
                    key: format!("img{id}|{rotation}"),
                    page: i + 1,
                    label: entry.name.clone(),
                    rotation,
                    ..Default::default()
                };
                match &entry.state {
                    ImageState::Loading => {}
                    ImageState::Failed(_) => tile.failed = true,
                    ImageState::Ready(info) => {
                        let shown = if rotation % 180 == 90 { (info.height, info.width) } else { (info.width, info.height) };
                        let place = self.layout.place((shown.0 as f32, shown.1 as f32));
                        let (page_w, page_h) = place.page;
                        tile.blank = Some(page_w / page_h);
                        if let Some(handle) = handle {
                            tile.overlay = Some(Overlay::Image {
                                handle,
                                aspect: place.width / place.height,
                                x: (place.x + place.width / 2.0) / page_w,
                                // Placements count from the bottom; tiles from the top.
                                y: 1.0 - (place.y + place.height / 2.0) / page_h,
                                width: place.width / page_w,
                                opacity: 1.0,
                            });
                        }
                    }
                }
                tile
            })
            .collect()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let tiles = self.tiles();
        let busy = self.run.busy();
        let wrap = Message::Images;
        let mut header = row![container(widgets::tool_header(catalog::info(ToolId::Images))).width(Length::Fill)]
            .spacing(8)
            .align_y(Alignment::Center);
        if self.entries.len() > 1 {
            header = header.push(action("Sort by name", Some(icons::sort()), false, (!busy).then_some(wrap(Msg::Sort))));
        }
        if !self.entries.is_empty() {
            header = header
                .push(action("Clear", Some(icons::trash()), false, (!busy).then_some(wrap(Msg::Clear))))
                .push(action("Add images…", Some(icons::plus()), true, (!busy).then_some(wrap(Msg::Add))));
        }

        let size = match self.layout.page_size {
            PageSize::A4 => "a4",
            PageSize::Letter => "letter",
            PageSize::FitImage => "fit",
        };
        let orientation = match self.layout.orientation {
            PageOrientation::Auto => "auto",
            PageOrientation::Portrait => "portrait",
            PageOrientation::Landscape => "landscape",
        };
        let margin = MARGINS.iter().find(|(_, m)| *m == self.layout.margin).map_or("none", |(n, _)| n);
        let mut options = column![section(
            "Page size",
            [segmented(&[("a4", "A4"), ("letter", "Letter"), ("fit", "Fit image")], size, move |v| wrap(Msg::Option("size", v)))]
        )]
        .spacing(18);
        if size == "fit" {
            options = options.push(muted("Each page takes the size of its image, at 96 pixels per inch."));
        } else {
            options = options.push(section(
                "Orientation",
                [
                    segmented(&[("auto", "Auto"), ("portrait", "Portrait"), ("landscape", "Landscape")], orientation, move |v| {
                        wrap(Msg::Option("orientation", v))
                    }),
                    muted("Auto turns pages sideways for wide images."),
                ],
            ));
        }
        options = options
            .push(section(
                "Margin",
                [segmented(&[("none", "None"), ("small", "Small"), ("large", "Large")], margin, move |v| wrap(Msg::Option("margin", v)))],
            ))
            .push(muted("Images are scaled to fit the page and centred. JPEGs are added without re-compressing, so photos keep their quality."));
        let left = container(options.padding(14)).width(340).height(Length::Fill).style(theme::panel);

        let count = tiles.len();
        let any = count > 0 && !busy;
        let mut grid_header = row![
            section_title("Pages"),
            text(if count == 0 { String::new() } else { plural(count, "image") }).color(theme::MUTED).width(Length::Fill),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 14.0 })
        .height(46);
        if count > 0 {
            grid_header = grid_header
                .push(widgets::small_action("Rotate all left", icons::rotate_ccw(), any.then_some(wrap(Msg::RotateAll(-90)))))
                .push(widgets::small_action("Rotate all right", icons::rotate_cw(), any.then_some(wrap(Msg::RotateAll(90)))));
        }
        let body: Element<'_, Message> = if tiles.is_empty() {
            crate::app::drop_zone("Drop JPEG or PNG images here", "or anywhere on the window", "Add images…", Some(wrap(Msg::Add)))
        } else {
            let caps = Caps { rotate: true, delete: true, reorder: true, ..Default::default() };
            widgets::page_grid(tiles, caps, busy, "", move |i, e| wrap(Msg::Tile(i, e)))
        };
        let right = container(column![grid_header, body]).width(Length::Fill).height(Length::Fill).style(theme::panel);

        column![
            container(header).padding(Padding { top: 20.0, right: 24.0, bottom: 0.0, left: 24.0 }),
            row![left, right].spacing(16).padding(Padding { top: 0.0, right: 24.0, bottom: 0.0, left: 24.0 }).height(Length::Fill),
            run_bar(&self.run, self.validate(), "Create PDF…", icons::images(), wrap(Msg::Run), wrap(Msg::Reveal)),
        ]
        .spacing(16)
        .into()
    }
}

/// JPEG or PNG, judged by the extension.
pub fn is_image(path: &Path) -> bool {
    path.extension().is_some_and(|e| ["jpg", "jpeg", "png"].iter().any(|x| e.eq_ignore_ascii_case(x)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_place_images_on_their_pages() {
        let mut tool = Images::default();
        let _ = tool.add_files(vec!["a.jpg".into(), "notes.txt".into()]);
        assert_eq!(tool.entries.len(), 1, "only images are added");
        let id = tool.entries[0].id;
        tool.reading = false;
        tool.entries[0].state = ImageState::Ready(ImageInfo { width: 400, height: 200, file_size: 1 });
        tool.thumbs.insert(id, RgbaImage::new(40, 20));
        let tile = &tool.tiles()[0];
        assert!(tile.blank.unwrap() > 1.0, "A4, Auto: landscape for a wide image");
        tool.rotate(Some(0), 90);
        assert!(tool.tiles()[0].blank.unwrap() < 1.0);
        assert_eq!(tool.validate(), None);
    }
}
