//! Signature: create a signature image, then place boxes of it on pages.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use pdftools_core::stamps::Placement;
use slint::{ComponentHandle, ModelRc, VecModel};

use super::{choose_output, color_of, file_name, plural, Job, Outcome, Shared, SingleFile, Tool};
use crate::app::App;
use crate::previews::PreviewRequest;
use crate::raster::{load_picture, render_strokes, render_text, strokes_to_svg, Family, Picture};
use crate::{single_file_plumbing, GridCaps, PageTile, Sign, SignBox};

const INKS: [[u8; 3]; 4] = [[0x1d, 0x2c, 0x6b], [0x00, 0x00, 0x00], [0x2f, 0x5b, 0xd3], [0xc4, 0x3d, 0x3d]];
const PAGE_WIDTH: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Draw,
    Type,
    Upload,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SigBox {
    pub id: i32,
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub struct SignatureTool {
    base: SingleFile,
    pub source: Source,
    strokes: Vec<Vec<(f32, f32)>>,
    ink: [u8; 3],
    name: String,
    family: Family,
    upload: Option<(PathBuf, Vec<u8>, Picture)>,
    /// PNG bytes and pixels of the current signature.
    signature: Option<(Vec<u8>, Picture)>,
    pub boxes: Vec<SigBox>,
    next_box: i32,
    pub page: usize,
    /// Displayed width / height per page, learned from rendering.
    aspects: HashMap<usize, f32>,
    boxes_model: Rc<VecModel<SignBox>>,
}

impl SignatureTool {
    pub fn new() -> Self {
        Self {
            base: SingleFile::new(super::id::SIGNATURE),
            source: Source::Draw,
            strokes: Vec::new(),
            ink: INKS[0],
            name: String::new(),
            family: Family::Script,
            upload: None,
            signature: None,
            boxes: Vec::new(),
            next_box: 1,
            page: 1,
            aspects: HashMap::new(),
            boxes_model: Rc::new(VecModel::default()),
        }
    }

    fn page_aspect(&self, page: usize) -> f32 {
        self.aspects.get(&page).copied().unwrap_or(1.0 / 1.414)
    }

    fn signature_aspect(&self) -> f32 {
        self.signature.as_ref().map_or(3.0, |(_, p)| p.aspect())
    }

    fn box_height(&self, width: f32, page: usize) -> f32 {
        width * self.page_aspect(page) / self.signature_aspect()
    }

    /// Re-creates the signature from the current source.
    fn rebuild(&mut self, s: &Shared) {
        let picture = match self.source {
            Source::Draw => render_strokes(&self.strokes, self.ink, 3.0, 3.0),
            Source::Type => render_text(&self.name, self.family, false, self.ink, 140.0),
            Source::Upload => self.upload.as_ref().map(|(_, _, p)| p.clone()),
        };
        self.signature = picture.map(|p| {
            let bytes = match (&self.source, &self.upload) {
                (Source::Upload, Some((_, bytes, _))) => bytes.clone(),
                _ => p.to_png(),
            };
            (bytes, p)
        });
        // Keep placed signatures in proportion with the new image.
        for i in 0..self.boxes.len() {
            let (width, page) = (self.boxes[i].width, self.boxes[i].page);
            self.boxes[i].height = self.box_height(width, page);
        }
        let sign = s.ui.global::<Sign>();
        sign.set_has_signature(self.signature.is_some());
        sign.set_signature(self.signature.as_ref().map(|(_, p)| p.to_image()).unwrap_or_default());
        sign.set_signature_aspect(self.signature_aspect());
        sign.set_ink_path(strokes_to_svg(&self.strokes).into());
        sign.set_ink(color_of(self.ink));
        sign.set_upload_name(self.upload.as_ref().map(|(p, _, _)| file_name(p)).unwrap_or_default().into());
    }

    pub fn pad_down(&mut self, _s: &mut Shared, x: f32, y: f32) {
        self.strokes.push(vec![(x, y)]);
    }

    pub fn pad_move(&mut self, s: &mut Shared, x: f32, y: f32) {
        if let Some(stroke) = self.strokes.last_mut() {
            stroke.push((x, y));
        }
        s.ui.global::<Sign>().set_ink_path(strokes_to_svg(&self.strokes).into());
    }

    pub fn pad_up(&mut self, s: &mut Shared) {
        self.rebuild(s);
    }

    pub fn pad_clear(&mut self, s: &mut Shared) {
        self.strokes.clear();
        self.rebuild(s);
    }

    pub fn choose_image(&mut self, s: &mut Shared) {
        let Some(path) = s.dialogs.pick_image() else { return };
        match load_picture(&path) {
            Ok((bytes, picture)) => self.upload = Some((path, bytes, picture)),
            Err(_) => self.upload = None,
        }
        self.rebuild(s);
    }

    pub fn add_box(&mut self, s: &mut Shared, every: bool) {
        if self.signature.is_none() || self.base.file.is_none() {
            return;
        }
        let pages: Vec<usize> = if every { (1..=self.base.page_count()).collect() } else { vec![self.page] };
        for page in pages {
            let width = 0.28;
            let height = self.box_height(width, page);
            self.boxes.push(SigBox { id: self.next_box, page, x: 1.0 - width - 0.07, y: (1.0 - height - 0.08).max(0.0), width, height });
            self.next_box += 1;
        }
        self.show_boxes(s);
    }

    pub fn box_changed(&mut self, s: &mut Shared, id: i32, x: f32, y: f32, width: f32, _height: f32) {
        let height = self.box_height(width, self.page);
        if let Some(b) = self.boxes.iter_mut().find(|b| b.id == id) {
            b.width = width.clamp(0.05, 1.0);
            b.height = height.min(1.0);
            b.x = x.clamp(0.0, 1.0 - b.width);
            b.y = y.clamp(0.0, 1.0 - b.height);
        }
        self.show_boxes(s);
    }

    pub fn box_removed(&mut self, s: &mut Shared, id: i32) {
        self.boxes.retain(|b| b.id != id);
        self.show_boxes(s);
    }

    pub fn page_step(&mut self, _s: &mut Shared, delta: i32) {
        let target = self.page as i32 + delta;
        if target >= 1 && target as usize <= self.base.page_count() {
            self.page = target as usize;
        }
    }

    fn show_boxes(&self, s: &Shared) {
        let boxes: Vec<SignBox> = self
            .boxes
            .iter()
            .filter(|b| b.page == self.page)
            .map(|b| SignBox { id: b.id, x: b.x, y: b.y, width: b.width, height: b.height })
            .collect();
        self.boxes_model.set_vec(boxes);
        let sign = s.ui.global::<Sign>();
        sign.set_boxes(ModelRc::from(self.boxes_model.clone()));
        let on_page = self.boxes.iter().filter(|b| b.page == self.page).count();
        let count = self.base.page_count();
        sign.set_page_label(if count == 0 {
            "".into()
        } else if on_page > 0 {
            format!("Page {} of {count} · {}", self.page, plural(on_page, "signature")).into()
        } else {
            format!("Page {} of {count}", self.page).into()
        });
        sign.set_can_prev(self.page > 1);
        sign.set_can_next(self.page < count);
    }

    /// Updates the placement view and renders the current page.
    pub fn refresh_page(&mut self, s: &mut Shared) {
        self.show_boxes(s);
        let sign = s.ui.global::<Sign>();
        sign.set_page_aspect(self.page_aspect(self.page));
        let Some(path) = self.base.path().map(PathBuf::from) else {
            sign.set_page_loaded(false);
            return;
        };
        let request = PreviewRequest { path, page: self.page, width: PAGE_WIDTH, rotation: 0, gray: false };
        if let Some((image, aspect, _)) = s.previews.cached(&request) {
            sign.set_page_image(image);
            sign.set_page_aspect(aspect);
            sign.set_page_loaded(true);
            self.aspects.insert(self.page, aspect);
            return;
        }
        sign.set_page_loaded(false);
        let page = self.page;
        s.previews.request(request, true, move |app: &mut App, result| {
            if let Ok((_, aspect, _)) = result {
                app.tool::<SignatureTool>(super::id::SIGNATURE).aspects.insert(page, aspect);
                app.refresh(super::id::SIGNATURE);
            }
        });
    }
}

impl Default for SignatureTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for SignatureTool {
    single_file_plumbing!();

    fn file_changed(&mut self, _s: &mut Shared) {
        self.boxes.clear();
        self.page = 1;
        self.aspects.clear();
    }

    fn option(&mut self, s: &mut Shared, key: &str, value: &str) {
        match key {
            "source" => {
                self.source = match value {
                    "type" => Source::Type,
                    "upload" => Source::Upload,
                    _ => Source::Draw,
                }
            }
            "name" => self.name = value.into(),
            "font" => self.family = Family::from_label(value),
            "ink" => self.ink = INKS[value.parse::<usize>().unwrap_or(0).min(INKS.len() - 1)],
            _ => return,
        }
        self.rebuild(s);
    }

    fn caps(&self) -> GridCaps {
        GridCaps::default()
    }

    /// The placement view replaces the page grid.
    fn tiles(&self) -> Vec<PageTile> {
        Vec::new()
    }

    fn run_label(&self) -> String {
        "Sign PDF…".into()
    }

    fn validate(&self) -> Option<String> {
        if self.signature.is_none() {
            return Some(
                match self.source {
                    Source::Draw => "Draw your signature.",
                    Source::Type => "Type your name.",
                    Source::Upload => "Choose a signature image.",
                }
                .into(),
            );
        }
        self.boxes.is_empty().then(|| "Add the signature to a page.".into())
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let (bytes, _) = self.signature.clone()?;
        let output = choose_output(&*s.dialogs, &input, "signed")?;
        let placements: Vec<Placement> = self
            .boxes
            .iter()
            .map(|b| Placement { page: b.page, x: b.x, y: b.y, width: b.width, height: b.height })
            .collect();
        let pages = {
            let mut pages: Vec<usize> = placements.iter().map(|p| p.page).collect();
            pages.dedup();
            pages.sort_unstable();
            pages.dedup();
            pages.len()
        };
        Some(Box::new(move |_| {
            pdftools_core::stamps::add_signature_file(&input, &bytes, &placements, &output).map_err(|e| e.to_string())?;
            Ok(Outcome { message: format!("Signed {} in", plural(pages, "page")), output: Some(output) })
        }))
    }
}
