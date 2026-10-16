//! Images to PDF: JPEG and PNG files, one per page, in an order the user
//! chooses, each turned as needed.
//!
//! Previews are drawn from small thumbnails placed on a blank page of the
//! chosen size, so layout changes show instantly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use image::imageops::FilterType;
use image::RgbaImage;
use pdftools_core::images::{ImageInfo, ImageLayout, ImagePage, PageOrientation, PageSize};
use slint::{ModelRc, VecModel};

use super::merge::natural_key;
use super::rearrange::move_item;
use super::{file_name, normalize_rotation, plural, run_state, Job, Outcome, Run, Shared, Tool};
use crate::ToImage;
use crate::raster::Picture;
use crate::{GridCaps, PageTile, ToolView};

/// Longest side of preview thumbnails, in pixels.
pub const THUMB_SIZE: u32 = 360;

/// Margins offered, in points: none, small (¼ inch), large (½ inch).
const MARGINS: [(&str, f32); 3] = [("none", 0.0), ("small", 18.0), ("large", 36.0)];

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

/// What the background reader sends back for one image.
pub type ReadResult = Result<(ImageInfo, RgbaImage), String>;

/// Reads an image's size and a small upright thumbnail (off the UI thread).
pub fn read_image(path: &Path) -> ReadResult {
    let info = pdftools_core::images::image_info(path).map_err(|e| e.to_string())?;
    let image = pdftools_core::images::load_image(path, 0).map_err(|e| e.to_string())?;
    Ok((info, image.resize(THUMB_SIZE, THUMB_SIZE, FilterType::Triangle).to_rgba8()))
}

pub struct ImagesTool {
    pub entries: Vec<ImageEntry>,
    next_id: u32,
    pub layout: ImageLayout,
    run: Run,
    pages: Rc<VecModel<PageTile>>,
    unread: Vec<(u32, PathBuf)>,
    /// Upright thumbnails by entry id.
    thumbs: HashMap<u32, RgbaImage>,
    /// Thumbnails turned for display, by entry id and rotation.
    shown: HashMap<(u32, i32), slint::Image>,
}

impl ImagesTool {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            layout: ImageLayout { margin: MARGINS[0].1, ..Default::default() },
            run: Run::default(),
            pages: Rc::new(VecModel::default()),
            unread: Vec::new(),
            thumbs: HashMap::new(),
            shown: HashMap::new(),
        }
    }

    /// Adds JPEG and PNG files; anything else is ignored.
    pub fn add_files(&mut self, paths: Vec<PathBuf>) {
        for path in paths.into_iter().filter(|p| super::is_image(p)) {
            let id = self.next_id;
            self.next_id += 1;
            self.unread.push((id, path.clone()));
            self.entries.push(ImageEntry { id, name: file_name(&path), path, state: ImageState::Loading, rotation: 0 });
        }
        if self.run.kind != 1 {
            self.run = Run::default();
        }
    }

    /// Images still to be read in the background.
    pub fn take_unread(&mut self) -> Vec<(u32, PathBuf)> {
        std::mem::take(&mut self.unread)
    }

    pub fn image_read(&mut self, id: u32, result: ReadResult) {
        let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) else { return };
        entry.state = match result {
            Ok((info, thumb)) => {
                self.thumbs.insert(id, thumb);
                ImageState::Ready(info)
            }
            Err(message) => ImageState::Failed(message),
        };
    }

    fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            let entry = self.entries.remove(index);
            self.thumbs.remove(&entry.id);
            self.shown.retain(|(id, _), _| *id != entry.id);
        }
    }

    pub fn clear(&mut self) {
        let next_id = self.next_id;
        *self = Self { pages: self.pages.clone(), layout: self.layout, next_id, ..Self::new() };
    }

    pub fn sort_by_name(&mut self) {
        self.entries.sort_by_key(|e| natural_key(&e.name));
    }

    fn rotate(&mut self, index: Option<usize>, delta: i32) {
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if index.is_none_or(|index| index == i) {
                entry.rotation = normalize_rotation(entry.rotation + delta);
            }
        }
    }

    /// The image behind tile `index` and its rotation, for the viewer.
    pub fn source(&self, index: usize) -> Option<(PathBuf, i32)> {
        self.entries.get(index).map(|e| (e.path.clone(), e.rotation))
    }

    /// The thumbnail turned by `rotation`, cached.
    fn thumb(&mut self, id: u32, rotation: i32) -> Option<slint::Image> {
        if let Some(image) = self.shown.get(&(id, rotation)) {
            return Some(image.clone());
        }
        let thumb = self.thumbs.get(&id)?;
        let turned = match rotation {
            90 => image::imageops::rotate90(thumb),
            180 => image::imageops::rotate180(thumb),
            270 => image::imageops::rotate270(thumb),
            _ => thumb.clone(),
        };
        let image = Picture { width: turned.width(), height: turned.height(), pixels: turned.into_raw() }.to_image();
        self.shown.insert((id, rotation), image.clone());
        Some(image)
    }

    fn tile(&self, index: usize, image: Option<slint::Image>) -> PageTile {
        let entry = &self.entries[index];
        let total = self.entries.len();
        let mut tile = PageTile {
            key: format!("img{}|{}", entry.id, entry.rotation).into(),
            page: index as i32 + 1,
            rotation: entry.rotation,
            aspect: 1.0 / 1.3,
            label: entry.name.clone().into(),
            caption: format!("Page {} of {total} · {}", index + 1, entry.name).into(),
            ..Default::default()
        };
        match &entry.state {
            ImageState::Loading => {}
            ImageState::Failed(message) => {
                tile.failed = true;
                tile.caption = format!("{} · {message}", entry.name).into();
            }
            ImageState::Ready(info) => {
                let shown = if entry.rotation % 180 == 90 { (info.height, info.width) } else { (info.width, info.height) };
                decorate(&mut tile, &self.layout, (shown.0 as f32, shown.1 as f32), image);
            }
        }
        tile
    }

    pub fn refresh(&mut self, s: &mut Shared) {
        if matches!(self.run.kind, 2 | 3) && self.validate().is_some() {
            self.run = Run::default();
        }
        let tiles: Vec<PageTile> = (0..self.entries.len())
            .map(|i| {
                let (id, rotation) = (self.entries[i].id, self.entries[i].rotation);
                let image = self.thumb(id, rotation);
                self.tile(i, image)
            })
            .collect();
        self.pages.set_vec(tiles);
        let total = self.entries.len();
        s.set_view(
            super::id::IMAGES,
            ToolView {
                pages: ModelRc::from(self.pages.clone()),
                caps: self.caps(),
                run: run_state(&self.run, self.run_label(), self.validate()),
                summary: if total == 0 { String::new() } else { plural(total, "image") }.into(),
                empty_text: "Images will appear here, one per page. Drag them to reorder.".into(),
                texts: super::strings(vec![String::new(); 8]),
                flags: super::bools(vec![false; 4]),
                ..Default::default()
            },
        );
    }
}

impl Default for ImagesTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Shows the image placed on its page: a blank page of the right shape with
/// the thumbnail on top.
pub fn decorate(tile: &mut PageTile, layout: &ImageLayout, shown: (f32, f32), image: Option<slint::Image>) {
    let place = layout.place(shown);
    let (page_w, page_h) = place.page;
    tile.aspect = page_w / page_h;
    tile.overlay_image_x = (place.x + place.width / 2.0) / page_w;
    // Placements count from the bottom; tiles from the top.
    tile.overlay_image_y = 1.0 - (place.y + place.height / 2.0) / page_h;
    tile.overlay_image_width = place.width / page_w;
    tile.overlay_image_opacity = 1.0;
    if let Some(image) = image {
        tile.overlay_image = image;
        tile.has_overlay_image = true;
        tile.loaded = true;
    }
}

impl Tool for ImagesTool {
    fn index(&self) -> usize {
        super::id::IMAGES
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn run_mut(&mut self) -> &mut Run {
        &mut self.run
    }

    fn run_ref(&self) -> &Run {
        &self.run
    }

    fn caps(&self) -> GridCaps {
        GridCaps { reorder: true, rotate: true, delete: true, ..Default::default() }
    }

    fn tiles(&self) -> Vec<PageTile> {
        (0..self.entries.len()).map(|i| self.tile(i, None)).collect()
    }

    fn option(&mut self, _s: &mut Shared, key: &str, value: &str) {
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
            "margin" => self.layout.margin = MARGINS.iter().find(|(name, _)| *name == value).map_or(0.0, |m| m.1),
            _ => {}
        }
    }

    fn tile_action(&mut self, _s: &mut Shared, action: &str, index: usize) {
        match action {
            "rotate-left" => self.rotate(Some(index), -90),
            "rotate-right" => self.rotate(Some(index), 90),
            "delete" => self.remove(index),
            _ => {}
        }
    }

    fn tile_moved(&mut self, _s: &mut Shared, from: usize, to: usize) {
        move_item(&mut self.entries, from, to);
    }

    fn grid_action(&mut self, _s: &mut Shared, action: &str) {
        match action {
            "rotate-all-left" => self.rotate(None, -90),
            "rotate-all-right" => self.rotate(None, 90),
            "sort" => self.sort_by_name(),
            "clear" => self.clear(),
            _ => {}
        }
    }

    fn run_label(&self) -> String {
        "Create PDF…".into()
    }

    fn validate(&self) -> Option<String> {
        let failed = self.entries.iter().filter(|e| matches!(e.state, ImageState::Failed(_))).count();
        if failed > 0 {
            return Some(format!(
                "Remove {} can’t be read to continue.",
                if failed == 1 { "the image that" } else { "images that" }
            ));
        }
        if self.entries.iter().any(|e| e.state == ImageState::Loading) {
            return Some("Reading images…".into());
        }
        if self.entries.is_empty() {
            return Some("Add images to get started.".into());
        }
        None
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let first = &self.entries.first()?.path;
        let name = if self.entries.len() == 1 { format!("{}.pdf", super::stem(first)) } else { "images.pdf".into() };
        let output = s.dialogs.save_pdf("Save PDF", &first.with_file_name(name))?;
        let images: Vec<ImagePage> =
            self.entries.iter().map(|e| ImagePage::new(&e.path).rotated(e.rotation)).collect();
        let layout = self.layout;
        Some(Box::new(move |report| {
            let total = images.len();
            let pages = pdftools_core::images::images_to_pdf_file(&images, &layout, &output, |i| {
                report(format!("Adding image {} of {total}…", i + 1), i as f32 / total as f32);
            })
            .map_err(|e| e.to_string())?;
            Ok(Outcome { message: format!("Saved {} to", plural(pages, "page")), output: Some(output) })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(tool: &mut ImagesTool, sizes: &[(u32, u32)]) {
        let paths: Vec<PathBuf> = (0..sizes.len()).map(|i| PathBuf::from(format!("img{i}.jpg"))).collect();
        tool.add_files(paths);
        for ((id, _), (w, h)) in tool.take_unread().into_iter().zip(sizes) {
            let info = ImageInfo { width: *w, height: *h, file_size: 1 };
            tool.image_read(id, Ok((info, RgbaImage::new(*w / 10, *h / 10))));
        }
    }

    #[test]
    fn keeps_images_in_order_with_their_rotation() {
        let mut tool = ImagesTool::new();
        assert_eq!(tool.validate().as_deref(), Some("Add images to get started."));
        tool.add_files(vec!["b10.png".into(), "notes.txt".into(), "B2.JPG".into()]);
        assert_eq!(tool.entries.len(), 2, "only images are added");
        assert_eq!(tool.validate().as_deref(), Some("Reading images…"));
        tool.sort_by_name();
        let names: Vec<&str> = tool.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["B2.JPG", "b10.png"]);

        let unread = tool.take_unread();
        tool.image_read(unread[0].0, Err("broken".into()));
        tool.image_read(unread[1].0, Ok((ImageInfo { width: 40, height: 20, file_size: 9 }, RgbaImage::new(4, 2))));
        assert!(tool.validate().unwrap().contains("can’t be read"));
        assert!(tool.tiles()[1].failed);

        tool.remove(1);
        assert_eq!(tool.validate(), None);
        tool.rotate(Some(0), 90);
        tool.rotate(None, 90);
        assert_eq!(tool.entries[0].rotation, 180);
        assert_eq!(tool.source(0).unwrap().1, 180);
        tool.clear();
        assert!(tool.entries.is_empty());
    }

    #[test]
    fn previews_place_images_on_their_pages() {
        let mut tool = ImagesTool::new();
        ready(&mut tool, &[(400, 200), (200, 400)]);
        let tiles = tool.tiles();
        // A4 with no margin: the wide image fills a landscape page.
        assert!((tiles[0].aspect - 841.89 / 595.28).abs() < 1e-3);
        assert!((tiles[0].overlay_image_width - 1.0).abs() < 1e-4);
        assert!((tiles[0].overlay_image_x - 0.5).abs() < 1e-4 && (tiles[0].overlay_image_y - 0.5).abs() < 1e-4);
        assert!(!tiles[0].loaded, "thumbnail images are attached on refresh");

        // Turning the wide image makes it tall, so its page turns too.
        tool.rotate(Some(0), -90);
        assert!(tool.tiles()[0].aspect < 1.0);
        assert_eq!(tool.thumb(tool.entries[0].id, 270).map(|i| i.size().width), Some(20));

        tool.layout.margin = 36.0;
        tool.layout.orientation = PageOrientation::Landscape;
        let tile = &tool.tiles()[1];
        assert!(tile.aspect > 1.0, "forced landscape");
        assert!(tile.overlay_image_width < 0.5);

        move_item(&mut tool.entries, 1, 0);
        assert_eq!(tool.entries[0].name, "img1.jpg");
    }

}
