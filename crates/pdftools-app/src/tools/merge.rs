//! Merge: several files, a page order (file order by default), per-page
//! rotation and pages left out.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use pdftools_core::{MergeProgress, PageSpec, PdfInfo};
use slint::{ComponentHandle, ModelRc, VecModel};

use super::rearrange::move_item;
use super::{color_of, file_name, normalize_rotation, plural, preview_request, run_state, tile_key, Job, Outcome, Run, Shared, Tool};
use crate::{GridCaps, Merge, MergeFile, PageTile, ToolView};

const SOURCE_COLORS: [[u8; 3]; 8] = [
    [0x3b, 0x6f, 0xe0], [0xd9, 0x53, 0x4f], [0x2e, 0x9d, 0x6a], [0xc7, 0x7c, 0x12],
    [0x8b, 0x55, 0xd6], [0x15, 0x97, 0xa8], [0xc2, 0x44, 0x8f], [0x6b, 0x7a, 0x8f],
];

#[derive(Debug, Clone, PartialEq)]
pub enum EntryState {
    Loading,
    Ready(usize),
    Failed { message: String, locked: bool },
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: u32,
    pub path: PathBuf,
    pub name: String,
    pub color: [u8; 3],
    pub state: EntryState,
}

/// A page of a source file: (entry id, 1-based page).
type PageKey = (u32, usize);

/// A page as it appears in the preview.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedPage {
    pub entry: usize,
    pub page: usize,
    pub rotation: i32,
    pub excluded: bool,
}

pub struct MergeTool {
    pub entries: Vec<Entry>,
    next_id: u32,
    /// Page order set by the user; `None` means file order.
    pub custom_order: Option<Vec<PageKey>>,
    rotations: HashMap<PageKey, i32>,
    excluded: HashSet<PageKey>,
    run: Run,
    pages: Rc<VecModel<PageTile>>,
    files: Rc<VecModel<MergeFile>>,
    highlight: Option<u32>,
    unread: Vec<(u32, PathBuf)>,
}

impl MergeTool {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            custom_order: None,
            rotations: HashMap::new(),
            excluded: HashSet::new(),
            run: Run::default(),
            pages: Rc::new(VecModel::default()),
            files: Rc::new(VecModel::default()),
            highlight: None,
            unread: Vec::new(),
        }
    }

    pub fn add_files(&mut self, paths: Vec<PathBuf>) {
        for path in paths.into_iter().filter(|p| super::is_pdf(p)) {
            let id = self.next_id;
            self.next_id += 1;
            self.unread.push((id, path.clone()));
            self.entries.push(Entry {
                id,
                name: file_name(&path),
                color: SOURCE_COLORS[(id as usize - 1) % SOURCE_COLORS.len()],
                path,
                state: EntryState::Loading,
            });
        }
        if self.run.kind != 1 {
            self.run = Run::default();
        }
    }

    /// Files whose page counts still need reading.
    pub fn take_unread(&mut self) -> Vec<(u32, PathBuf)> {
        std::mem::take(&mut self.unread)
    }

    pub fn file_read(&mut self, id: u32, result: Result<PdfInfo, String>) {
        let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) else { return };
        entry.state = match result {
            Ok(info) => match info.pages {
                Some(pages) => EntryState::Ready(pages),
                None => EntryState::Failed { message: "Password-protected".into(), locked: true },
            },
            Err(message) => EntryState::Failed { message, locked: false },
        };
        // In a custom order, pages of newly read files go at the end.
        if let (Some(order), EntryState::Ready(pages)) = (&mut self.custom_order, &entry.state) {
            for page in 1..=*pages {
                if !order.contains(&(id, page)) {
                    order.push((id, page));
                }
            }
        }
    }

    pub fn path_of(&self, index: usize) -> Option<PathBuf> {
        self.entries.get(index).map(|e| e.path.clone())
    }

    fn natural_order(&self) -> Vec<PageKey> {
        self.entries
            .iter()
            .flat_map(|e| match e.state {
                EntryState::Ready(pages) => (1..=pages).map(|p| (e.id, p)).collect(),
                _ => Vec::new(),
            })
            .collect()
    }

    /// Every page in result order, including ones left out.
    pub fn ordered(&self) -> Vec<OrderedPage> {
        let order = self.custom_order.clone().unwrap_or_else(|| self.natural_order());
        order
            .into_iter()
            .filter_map(|(id, page)| {
                let entry = self.entries.iter().position(|e| e.id == id)?;
                match self.entries[entry].state {
                    EntryState::Ready(pages) if page <= pages => Some(OrderedPage {
                        entry,
                        page,
                        rotation: self.rotations.get(&(id, page)).copied().unwrap_or(0),
                        excluded: self.excluded.contains(&(id, page)),
                    }),
                    _ => None,
                }
            })
            .collect()
    }

    fn key_of(&self, page: &OrderedPage) -> PageKey {
        (self.entries[page.entry].id, page.page)
    }

    /// Stores a custom order, or file order again if it matches it.
    fn set_order(&mut self, order: Vec<PageKey>) {
        self.custom_order = (order != self.natural_order()).then_some(order);
    }

    pub fn move_page(&mut self, from: usize, to: usize) {
        let mut order: Vec<PageKey> = self.ordered().iter().map(|p| self.key_of(p)).collect();
        if move_item(&mut order, from, to) {
            self.set_order(order);
        }
    }

    fn rotate(&mut self, keys: Vec<PageKey>, delta: i32) {
        for key in keys {
            let rotation = normalize_rotation(self.rotations.get(&key).copied().unwrap_or(0) + delta);
            if rotation == 0 {
                self.rotations.remove(&key);
            } else {
                self.rotations.insert(key, rotation);
            }
        }
    }

    pub fn include_range(&mut self, from: usize, to: usize, include: bool) {
        let ordered = self.ordered();
        for page in ordered.iter().skip(from).take(to.saturating_sub(from) + 1) {
            let key = self.key_of(page);
            if include {
                self.excluded.remove(&key);
            } else {
                self.excluded.insert(key);
            }
        }
    }

    pub fn sort_by_name(&mut self) {
        self.entries.sort_by_key(|e| natural_key(&e.name));
    }

    pub fn move_file(&mut self, from: usize, to: usize) {
        if self.custom_order.is_none() {
            move_item(&mut self.entries, from, to);
        }
    }

    pub fn file_action(&mut self, s: &mut Shared, action: &str, index: usize) {
        if index >= self.entries.len() {
            return;
        }
        match action {
            "up" if index > 0 => self.move_file(index, index - 1),
            "down" => self.move_file(index, index + 2),
            "remove" => {
                let entry = self.entries.remove(index);
                s.previews.forget(&entry.path);
                self.rotations.retain(|(id, _), _| *id != entry.id);
                self.excluded.retain(|(id, _)| *id != entry.id);
                if let Some(order) = &self.custom_order {
                    let order = order.iter().copied().filter(|(id, _)| *id != entry.id).collect();
                    self.set_order(order);
                }
            }
            "hover" => {
                self.highlight = Some(self.entries[index].id);
                self.refresh_highlight();
            }
            "unhover" => {
                self.highlight = None;
                self.refresh_highlight();
            }
            _ => {}
        }
    }

    pub fn clear(&mut self, s: &mut Shared) {
        for entry in &self.entries {
            s.previews.forget(&entry.path);
        }
        *self = Self { pages: self.pages.clone(), files: self.files.clone(), next_id: self.next_id, ..Self::new() };
    }

    fn refresh_highlight(&self) {
        use slint::Model;
        for row in 0..self.pages.row_count() {
            if let Some(mut tile) = self.pages.row_data(row) {
                let selected = self.highlight.is_some_and(|id| tile.key.starts_with(&format!("{id}#")));
                if tile.selected != selected {
                    tile.selected = selected;
                    self.pages.set_row_data(row, tile);
                }
            }
        }
    }

    fn tile(&self, page: &OrderedPage, index: usize, total: usize) -> PageTile {
        let entry = &self.entries[page.entry];
        PageTile {
            // "<id>#" prefix lets hovering a file highlight its pages.
            key: format!("{}#{}", entry.id, tile_key(&entry.path, page.page, page.rotation)).into(),
            page: page.page as i32,
            rotation: page.rotation,
            aspect: 1.0 / 1.3,
            excluded: page.excluded,
            stripe: color_of(entry.color),
            has_stripe: true,
            selected: self.highlight == Some(entry.id),
            label: format!("{} · p. {}", entry.name, page.page).into(),
            caption: format!(
                "Page {} of {total} · {}, page {}{}",
                index + 1,
                entry.name,
                page.page,
                if page.excluded { " · left out" } else { "" }
            )
            .into(),
            ..Default::default()
        }
    }

    pub fn refresh(&mut self, s: &mut Shared) {
        if matches!(self.run.kind, 2 | 3) && self.validate().is_some() {
            self.run = Run::default();
        }
        let ordered = self.ordered();
        let total = ordered.len();
        let mut tiles: Vec<PageTile> = ordered.iter().enumerate().map(|(i, p)| self.tile(p, i, total)).collect();
        for (tile, page) in tiles.iter_mut().zip(&ordered) {
            let entry = &self.entries[page.entry];
            if let Some((image, aspect, _)) = s.previews.cached(&preview_request(&entry.path, page.page, page.rotation, false)) {
                tile.image = image;
                tile.aspect = aspect;
                tile.loaded = true;
            }
        }
        self.pages.set_vec(tiles);

        let files: Vec<MergeFile> = self
            .entries
            .iter()
            .map(|e| {
                let thumb = s.previews.cached(&preview_request(&e.path, 1, 0, false)).map(|(img, _, _)| img);
                let (meta, error, locked) = match &e.state {
                    EntryState::Loading => ("Reading…".to_owned(), String::new(), false),
                    EntryState::Ready(pages) => (plural(*pages, "page"), String::new(), false),
                    EntryState::Failed { message, locked } => (String::new(), message.clone(), *locked),
                };
                MergeFile {
                    name: e.name.clone().into(),
                    path: e.path.display().to_string().into(),
                    tint: color_of(e.color),
                    meta: meta.into(),
                    error: error.into(),
                    locked,
                    has_thumb: thumb.is_some(),
                    thumb: thumb.unwrap_or_default(),
                }
            })
            .collect();
        self.files.set_vec(files);
        let merge = s.ui.global::<Merge>();
        merge.set_files(ModelRc::from(self.files.clone()));
        merge.set_custom_order(self.custom_order.is_some());

        let kept = ordered.iter().filter(|p| !p.excluded).count();
        let summary = if total == 0 {
            String::new()
        } else if kept == total {
            plural(total, "page")
        } else {
            format!("{kept} of {} kept", plural(total, "page"))
        };
        s.set_view(
            super::id::MERGE,
            ToolView {
                pages: ModelRc::from(self.pages.clone()),
                caps: self.caps(),
                run: run_state(&self.run, self.run_label(), self.validate()),
                summary: summary.into(),
                include_all: !self.excluded.is_empty(),
                empty_text: if self.entries.iter().any(|e| e.state == EntryState::Loading) {
                    "Reading files…".into()
                } else {
                    "Pages of the merged PDF will appear here. Drag them to reorder.".into()
                },
                texts: super::strings(vec![String::new(); 8]),
                flags: super::bools(vec![self.highlight.is_some(), false, false, false]),
                ..Default::default()
            },
        );
    }
}

impl Default for MergeTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Sort key that orders "page2" before "page10", ignoring case.
pub fn natural_key(name: &str) -> Vec<(String, u64)> {
    let mut key = Vec::new();
    let mut text = String::new();
    let mut digits = String::new();
    for c in name.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            if !digits.is_empty() {
                key.push((std::mem::take(&mut text), digits.parse().unwrap_or(u64::MAX)));
                digits.clear();
            }
            text.extend(c.to_lowercase());
        }
    }
    key.push((text, if digits.is_empty() { 0 } else { digits.parse().unwrap_or(u64::MAX) }));
    key
}

impl Tool for MergeTool {
    fn index(&self) -> usize {
        super::id::MERGE
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
        GridCaps { reorder: true, rotate: true, include: true, ..Default::default() }
    }

    fn tiles(&self) -> Vec<PageTile> {
        let ordered = self.ordered();
        ordered.iter().enumerate().map(|(i, p)| self.tile(p, i, ordered.len())).collect()
    }

    fn tile_action(&mut self, _s: &mut Shared, action: &str, index: usize) {
        let Some(page) = self.ordered().get(index).cloned() else { return };
        let key = self.key_of(&page);
        match action {
            "rotate-left" => self.rotate(vec![key], -90),
            "rotate-right" => self.rotate(vec![key], 90),
            "toggle-include" => {
                if !self.excluded.remove(&key) {
                    self.excluded.insert(key);
                }
            }
            _ => {}
        }
    }

    fn tile_moved(&mut self, _s: &mut Shared, from: usize, to: usize) {
        self.move_page(from, to);
    }

    fn grid_action(&mut self, _s: &mut Shared, action: &str) {
        let all: Vec<PageKey> = self.ordered().iter().map(|p| self.key_of(p)).collect();
        match action {
            "rotate-all-left" => self.rotate(all, -90),
            "rotate-all-right" => self.rotate(all, 90),
            "reset-order" => self.custom_order = None,
            "include-all" => self.excluded.clear(),
            _ => {}
        }
    }

    fn run_label(&self) -> String {
        if self.entries.len() > 1 {
            format!("Merge {}…", plural(self.entries.len(), "file"))
        } else {
            "Save PDF…".into()
        }
    }

    fn validate(&self) -> Option<String> {
        let errors = self.entries.iter().filter(|e| matches!(e.state, EntryState::Failed { .. })).count();
        if errors > 0 {
            return Some(format!("Remove {} can’t be merged to continue.", if errors == 1 { "the file that" } else { "files that" }));
        }
        if self.entries.iter().any(|e| e.state == EntryState::Loading) {
            return Some("Reading files…".into());
        }
        if self.entries.is_empty() {
            return Some("Add PDF files to get started.".into());
        }
        if self.ordered().iter().all(|p| p.excluded) {
            return Some(if self.excluded.is_empty() {
                "There are no pages to save.".into()
            } else {
                "Every page is left out — tick at least one to keep it.".into()
            });
        }
        None
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let first = self.entries.first()?.path.clone();
        let output = s.dialogs.save_pdf("Save merged PDF", &first.with_file_name("merged.pdf"))?;
        let inputs: Vec<PathBuf> = self.entries.iter().map(|e| e.path.clone()).collect();
        let pages: Vec<PageSpec> = self
            .ordered()
            .into_iter()
            .filter(|p| !p.excluded)
            .map(|p| PageSpec::new(p.entry, p.page).rotated(p.rotation as i64))
            .collect();
        let left_out = self.excluded.len();
        Some(Box::new(move |report| {
            let steps = (inputs.len() + 2) as f32;
            let summary = pdftools_core::merge_files(&inputs, Some(&pages), &output, |progress| {
                let step = match &progress {
                    MergeProgress::Loading { index, .. } => *index as f32,
                    MergeProgress::Merging => steps - 2.0,
                    MergeProgress::Writing => steps - 1.0,
                };
                report(progress.to_string(), step / steps);
            })
            .map_err(|e| e.to_string())?;
            let removed = if left_out > 0 { format!(" ({} left out)", plural(left_out, "page")) } else { String::new() };
            Ok(Outcome {
                message: format!("Saved {} from {}{removed} to", plural(summary.pages, "page"), plural(summary.files, "file")),
                output: Some(output),
            })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(tool: &mut MergeTool, pages: &[usize]) {
        let paths: Vec<PathBuf> = (0..pages.len()).map(|i| PathBuf::from(format!("f{i}.pdf"))).collect();
        tool.add_files(paths);
        for (entry, count) in tool.take_unread().into_iter().zip(pages) {
            tool.file_read(entry.0, Ok(PdfInfo { pages: Some(*count), ..Default::default() }));
        }
    }

    #[test]
    fn natural_order_follows_files_and_custom_order_sticks() {
        let mut tool = MergeTool::new();
        ready(&mut tool, &[2, 1]);
        let order = |t: &MergeTool| t.ordered().iter().map(|p| (p.entry, p.page)).collect::<Vec<_>>();
        assert_eq!(order(&tool), [(0, 1), (0, 2), (1, 1)]);
        tool.move_page(2, 0);
        assert!(tool.custom_order.is_some());
        assert_eq!(order(&tool), [(1, 1), (0, 1), (0, 2)]);
        tool.move_page(0, 3);
        assert!(tool.custom_order.is_none(), "back to file order");
        tool.move_file(1, 0);
        // Entry 0 is now the one-page file.
        assert_eq!(order(&tool), [(0, 1), (1, 1), (1, 2)]);
    }

    #[test]
    fn validates_file_states() {
        let mut tool = MergeTool::new();
        assert_eq!(tool.validate().as_deref(), Some("Add PDF files to get started."));
        tool.add_files(vec!["a.pdf".into(), "b.txt".into()]);
        assert_eq!(tool.entries.len(), 1, "non-PDFs are ignored");
        assert_eq!(tool.validate().as_deref(), Some("Reading files…"));
        let (id, _) = tool.take_unread()[0].clone();
        tool.file_read(id, Ok(PdfInfo { pages: None, needs_password: true, ..Default::default() }));
        assert!(tool.validate().unwrap().contains("can’t be merged"));
        tool.file_read(id, Err("bad".into()));
        assert!(matches!(tool.entries[0].state, EntryState::Failed { locked: false, .. }));
    }

    #[test]
    fn sorts_names_naturally() {
        let mut names = vec!["page10.pdf", "Page2.pdf", "page1.pdf", "appendix.pdf"];
        names.sort_by_key(|n| natural_key(n));
        assert_eq!(names, ["appendix.pdf", "page1.pdf", "Page2.pdf", "page10.pdf"]);
    }
}
