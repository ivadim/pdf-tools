//! Merge: several files, a page order (file order by default), per-page
//! rotation and pages left out.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use iced::widget::{column, container, image, row, scrollable, space, text};
use iced::{color, Alignment, Color, ContentFit, Element, Length, Padding};
use pdftools_core::{MergeProgress, PageSpec, PdfInfo};
use pdftools_render::pages::PreviewRequest;

use crate::app::{Message, THUMB_WIDTH};
use crate::catalog::{self, ToolId};
use crate::icons;
use crate::previews::{self, Previews};
use crate::single::{file_name, plural};
use crate::theme;
use crate::tools::move_item;
use crate::widgets::{self, action, icon_button, muted, run_bar, section_title, Caps, Run, Tile, TileEvent, BOLD};

const SOURCE_COLORS: [Color; 8] = [
    color!(0x3b6fe0), color!(0xd9534f), color!(0x2e9d6a), color!(0xc77c12),
    color!(0x8b55d6), color!(0x1597a8), color!(0xc2448f), color!(0x6b7a8f),
];

#[derive(Debug, Clone)]
pub enum Msg {
    Add,
    Read(u32, Result<PdfInfo, String>),
    Sort,
    Clear,
    FileUp(usize),
    FileDown(usize),
    FileRemove(usize),
    Unlock(usize),
    Tile(usize, TileEvent),
    RotateAll(i32),
    ResetOrder,
    IncludeAll,
    Run,
    Finished(Result<(String, Option<PathBuf>), String>),
    Reveal,
}

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
    pub color: Color,
    pub state: EntryState,
}

/// A page of a source file: (entry id, 1-based page).
type PageKey = (u32, usize);

#[derive(Debug, Clone, PartialEq)]
pub struct OrderedPage {
    pub entry: usize,
    pub page: usize,
    pub rotation: i32,
    pub excluded: bool,
}

#[derive(Default)]
pub struct Merge {
    pub entries: Vec<Entry>,
    next_id: u32,
    /// Page order set by the user; `None` means file order.
    pub custom_order: Option<Vec<PageKey>>,
    rotations: HashMap<PageKey, i32>,
    excluded: HashSet<PageKey>,
    pub run: Run,
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

impl Merge {
    /// Adds PDF files; returns the ones to read.
    pub fn add_files(&mut self, paths: Vec<PathBuf>) -> Vec<(u32, PathBuf)> {
        let mut unread = Vec::new();
        for path in paths.into_iter().filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))) {
            self.next_id += 1;
            let id = self.next_id;
            unread.push((id, path.clone()));
            self.entries.push(Entry {
                id,
                name: file_name(&path),
                color: SOURCE_COLORS[(id as usize - 1) % SOURCE_COLORS.len()],
                path,
                state: EntryState::Loading,
            });
        }
        if !self.run.busy() {
            self.run = Run::Idle;
        }
        unread
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
            let rotation = (self.rotations.get(&key).copied().unwrap_or(0) + delta).rem_euclid(360);
            if rotation == 0 {
                self.rotations.remove(&key);
            } else {
                self.rotations.insert(key, rotation);
            }
        }
    }

    pub fn move_file(&mut self, from: usize, to: usize) {
        if self.custom_order.is_none() {
            move_item(&mut self.entries, from, to);
        }
    }

    fn remove(&mut self, index: usize, previews: &mut Previews) {
        if index >= self.entries.len() {
            return;
        }
        let entry = self.entries.remove(index);
        previews.forget(&entry.path);
        self.rotations.retain(|(id, _), _| *id != entry.id);
        self.excluded.retain(|(id, _)| *id != entry.id);
        if let Some(order) = &self.custom_order {
            let order = order.iter().copied().filter(|(id, _)| *id != entry.id).collect();
            self.set_order(order);
        }
    }

    pub fn validate(&self) -> Option<String> {
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

    pub fn request(&self, page: &OrderedPage, width: u32) -> PreviewRequest {
        PreviewRequest { path: self.entries[page.entry].path.clone(), page: page.page, width, rotation: page.rotation, gray: false }
    }

    /// Handles a message; returns files to read, previews to render, or a
    /// job to run, for the app to start.
    pub fn update(&mut self, msg: Msg, ctx: &mut Context) -> Option<Effect> {
        match msg {
            Msg::Add => return Some(Effect::Read(self.add_files(ctx.dialogs.pick_pdfs()))),
            Msg::Read(id, result) => {
                let readable = matches!(&result, Ok(info) if info.pages.is_some_and(|p| p > 0));
                self.file_read(id, result);
                // Page 1 doubles as the file's thumbnail in the list.
                if readable {
                    if let Some(entry) = self.entries.iter().find(|e| e.id == id) {
                        let request = PreviewRequest { path: entry.path.clone(), page: 1, width: THUMB_WIDTH, rotation: 0, gray: false };
                        return Some(Effect::Render(request));
                    }
                }
            }
            Msg::Sort => self.entries.sort_by_key(|e| natural_key(&e.name)),
            Msg::Clear => {
                for entry in &self.entries {
                    ctx.previews.forget(&entry.path);
                }
                let next_id = self.next_id;
                *self = Self { next_id, ..Self::default() };
            }
            Msg::FileUp(i) if i > 0 => self.move_file(i, i - 1),
            Msg::FileDown(i) => self.move_file(i, i + 2),
            Msg::FileRemove(i) => self.remove(i, ctx.previews),
            Msg::Unlock(i) => return self.entries.get(i).map(|e| Effect::Unlock(e.path.clone())),
            Msg::Tile(index, event) => {
                let ordered = self.ordered();
                let Some(page) = ordered.get(index).cloned() else { return None };
                let key = self.key_of(&page);
                match event {
                    TileEvent::Shown => return Some(Effect::Render(self.request(&page, THUMB_WIDTH))),
                    TileEvent::View => return Some(Effect::View(index)),
                    TileEvent::RotateLeft => self.rotate(vec![key], -90),
                    TileEvent::RotateRight => self.rotate(vec![key], 90),
                    TileEvent::ToggleInclude => {
                        if !self.excluded.remove(&key) {
                            self.excluded.insert(key);
                        }
                    }
                    TileEvent::MoveEarlier if index > 0 => self.move_page(index, index - 1),
                    TileEvent::MoveLater => self.move_page(index, index + 2),
                    _ => {}
                }
            }
            Msg::RotateAll(delta) => {
                let all = self.ordered().iter().map(|p| self.key_of(p)).collect();
                self.rotate(all, delta);
            }
            Msg::ResetOrder => self.custom_order = None,
            Msg::IncludeAll => self.excluded.clear(),
            Msg::Run => {
                if self.validate().is_some() || self.run.busy() {
                    return None;
                }
                let first = self.entries.first()?.path.clone();
                let output = ctx.dialogs.save_pdf("Save merged PDF", &first.with_file_name("merged.pdf"))?;
                let inputs: Vec<PathBuf> = self.entries.iter().map(|e| e.path.clone()).collect();
                let pages: Vec<PageSpec> = self
                    .ordered()
                    .into_iter()
                    .filter(|p| !p.excluded)
                    .map(|p| PageSpec::new(p.entry, p.page).rotated(p.rotation as i64))
                    .collect();
                let left_out = self.excluded.len();
                self.run = Run::Running("Merging…".into());
                return Some(Effect::Job(Box::new(move || {
                    let summary =
                        pdftools_core::merge_files(&inputs, Some(&pages), &output, |_: MergeProgress| {}).map_err(|e| e.to_string())?;
                    let removed = if left_out > 0 { format!(" ({} left out)", plural(left_out, "page")) } else { String::new() };
                    Ok((format!("Saved {} from {}{removed} to", plural(summary.pages, "page"), plural(summary.files, "file")), Some(output)))
                })));
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
            _ => {}
        }
        if matches!(self.run, Run::Done { .. } | Run::Failed(_)) && self.validate().is_some() {
            self.run = Run::Idle;
        }
        None
    }

    pub fn tiles(&self, previews: &Previews) -> Vec<Tile> {
        self.ordered()
            .iter()
            .map(|p| {
                let entry = &self.entries[p.entry];
                let request = self.request(p, THUMB_WIDTH);
                Tile {
                    key: previews::key(&request),
                    page: p.page,
                    label: format!("{} · p. {}", entry.name, p.page),
                    rotation: p.rotation,
                    excluded: p.excluded,
                    thumb: previews.get(&request).cloned(),
                    failed: previews.failed(&request),
                    stripe: Some(entry.color),
                    ..Default::default()
                }
            })
            .collect()
    }

    pub fn view<'a>(&'a self, previews: &'a Previews) -> Element<'a, Message> {
        let busy = self.run.busy();
        let locked = self.custom_order.is_some() || busy;
        let wrap = Message::Merge;
        // Actions that don't apply yet stay hidden rather than disabled.
        let mut header = row![container(widgets::tool_header(catalog::info(ToolId::Merge))).width(Length::Fill)]
            .spacing(8)
            .align_y(Alignment::Center);
        if self.entries.len() > 1 {
            header = header.push(action("Sort by name", Some(icons::sort()), false, (!locked).then_some(wrap(Msg::Sort))));
        }
        if !self.entries.is_empty() {
            header = header
                .push(action("Clear", Some(icons::trash()), false, (!busy).then_some(wrap(Msg::Clear))))
                .push(action("Add files…", Some(icons::plus()), true, (!busy).then_some(wrap(Msg::Add))));
        }
        if self.entries.is_empty() {
            // One place to start: a single drop zone across both panels.
            let zone = crate::app::drop_zone("Drop PDF files here", "or anywhere on the window", "Add PDF files…", Some(wrap(Msg::Add)));
            return column![
                container(header).padding(Padding { top: 20.0, right: 24.0, bottom: 0.0, left: 24.0 }),
                container(container(zone).style(theme::panel)).padding(Padding { top: 0.0, right: 24.0, bottom: 0.0, left: 24.0 }).height(Length::Fill),
                run_bar(&self.run, self.validate(), "Merge PDFs…", icons::merge(), wrap(Msg::Run), wrap(Msg::Reveal)),
            ]
            .spacing(16)
            .into();
        }

        let files: Element<'a, Message> = if self.entries.is_empty() {
            container(
                column![
                    widgets::icon(icons::upload(), 36.0, theme::c().muted),
                    text("Drop PDF files here"),
                    muted("or"),
                    action("Add PDF files", Some(icons::plus()), true, Some(wrap(Msg::Add))),
                ]
                .spacing(8)
                .align_x(Alignment::Center),
            )
            .center(Length::Fill)
            .into()
        } else {
            let count = self.entries.len();
            scrollable(column(self.entries.iter().enumerate().map(|(i, e)| file_row(i, count, e, previews, locked, busy))).spacing(4).padding(8))
                .height(Length::Fill)
                .into()
        };
        let left = container(column![
            row![section_title("Files"), space::horizontal(), muted(if self.entries.is_empty() { String::new() } else { plural(self.entries.len(), "file") })]
                .padding([14, 14])
                .align_y(Alignment::Center),
            files,
        ])
        .width(360)
        .height(Length::Fill)
        .style(theme::panel);

        let tiles = self.tiles(previews);
        let kept = tiles.iter().filter(|t| !t.excluded).count();
        let summary = match tiles.len() {
            0 => String::new(),
            n if kept == n => plural(n, "page"),
            n => format!("{kept} of {} kept", plural(n, "page")),
        };
        let mut grid_header = row![section_title("Merged result"), text(summary).color(theme::c().muted).width(Length::Fill)]
            .spacing(10)
            .align_y(Alignment::Center)
            .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 14.0 })
            .height(46);
        if !self.excluded.is_empty() {
            grid_header = grid_header.push(action("Include all", Some(icons::check()), false, Some(wrap(Msg::IncludeAll))));
        }
        let any = !tiles.is_empty() && !busy;
        grid_header = grid_header
            .push(widgets::small_action("Rotate all left", icons::rotate_ccw(), any.then_some(wrap(Msg::RotateAll(-90)))))
            .push(widgets::small_action("Rotate all right", icons::rotate_cw(), any.then_some(wrap(Msg::RotateAll(90)))));
        let mut right = column![grid_header];
        if self.custom_order.is_some() {
            right = right.push(
                container(
                    row![
                        text("Custom page order. File order is locked until you reset.").width(Length::Fill),
                        action("Reset to file order", Some(icons::reset()), false, Some(wrap(Msg::ResetOrder))),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([6, 14])
                .style(theme::notice),
            );
        }
        let empty = if self.entries.iter().any(|e| e.state == EntryState::Loading) {
            "Reading files…"
        } else {
            "Pages of the merged PDF will appear here."
        };
        let caps = Caps { include: true, rotate: true, reorder: true, ..Default::default() };
        right = right.push(widgets::page_grid(tiles, caps, busy, empty, move |i, e| wrap(Msg::Tile(i, e))));
        let right = container(right).width(Length::Fill).height(Length::Fill).style(theme::panel);

        let label = "Merge PDFs…";
        column![
            container(header).padding(Padding { top: 20.0, right: 24.0, bottom: 0.0, left: 24.0 }),
            row![left, right].spacing(16).padding(Padding { top: 0.0, right: 24.0, bottom: 0.0, left: 24.0 }).height(Length::Fill),
            run_bar(&self.run, self.validate(), label, icons::merge(), wrap(Msg::Run), wrap(Msg::Reveal)),
        ]
        .spacing(16)
        .into()
    }
}

fn file_row<'a>(index: usize, count: usize, entry: &'a Entry, previews: &'a Previews, locked: bool, busy: bool) -> Element<'a, Message> {
    let wrap = Message::Merge;
    let request = PreviewRequest { path: entry.path.clone(), page: 1, width: THUMB_WIDTH, rotation: 0, gray: false };
    let thumb: Element<'a, Message> = match previews.get(&request) {
        Some(t) => image(t.handle.clone()).content_fit(ContentFit::Contain).into(),
        None => space().into(),
    };
    let number = container(text(index + 1).size(12).font(BOLD).color(Color::WHITE)).center(22).style(move |_| container::Style {
        background: Some(entry.color.into()),
        border: iced::border::rounded(11),
        ..Default::default()
    });
    let details: Element<'a, Message> = match &entry.state {
        EntryState::Loading => muted("Reading…"),
        EntryState::Ready(pages) => muted(plural(*pages, "page")),
        EntryState::Failed { message, .. } => text(format!("Can’t be merged: {message}")).size(12).color(theme::c().error).into(),
    };
    let mut row = row![
        number,
        container(thumb).width(44).height(56).style(theme::page),
        column![text(entry.name.clone()).font(BOLD).wrapping(text::Wrapping::None), details].spacing(2).width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if matches!(entry.state, EntryState::Failed { locked: true, .. }) {
        row = row.push(action("Unlock…", None, false, Some(wrap(Msg::Unlock(index)))));
    }
    // Move and remove appear on hover, keeping the list calm.
    let buttons = iced::widget::row![
        icon_button(icons::up(), "Move up", 28.0, (index > 0 && !locked).then_some(wrap(Msg::FileUp(index)))),
        icon_button(icons::down(), "Move down", 28.0, (index + 1 < count && !locked).then_some(wrap(Msg::FileDown(index)))),
        icon_button(icons::x(), "Remove", 28.0, (!busy).then_some(wrap(Msg::FileRemove(index)))),
    ]
    .spacing(1);
    let buttons = container(container(buttons).padding(2).style(theme::hover_bar)).center_y(Length::Fill).align_right(Length::Fill);
    iced::widget::hover(container(row).padding(8), buttons)
}

/// What the app gives a multi-file tool to work with.
pub struct Context<'a> {
    pub dialogs: &'a dyn crate::dialogs::Dialogs,
    pub previews: &'a mut Previews,
}

/// Work a multi-file tool asks the app to start.
pub enum Effect {
    Read(Vec<(u32, PathBuf)>),
    ReadImage(u32, PathBuf),
    Render(PreviewRequest),
    View(usize),
    Unlock(PathBuf),
    Job(crate::single::Job),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(tool: &mut Merge, pages: &[usize]) {
        let paths: Vec<PathBuf> = (0..pages.len()).map(|i| PathBuf::from(format!("f{i}.pdf"))).collect();
        for ((id, _), count) in tool.add_files(paths).into_iter().zip(pages) {
            tool.file_read(id, Ok(PdfInfo { pages: Some(*count), ..Default::default() }));
        }
    }

    #[test]
    fn natural_order_follows_files_and_custom_order_sticks() {
        let mut tool = Merge::default();
        ready(&mut tool, &[2, 1]);
        let order = |t: &Merge| t.ordered().iter().map(|p| (p.entry, p.page)).collect::<Vec<_>>();
        assert_eq!(order(&tool), [(0, 1), (0, 2), (1, 1)]);
        tool.move_page(2, 0);
        assert!(tool.custom_order.is_some());
        tool.move_page(0, 3);
        assert!(tool.custom_order.is_none(), "back to file order");
        tool.move_file(1, 0);
        assert_eq!(order(&tool), [(0, 1), (1, 1), (1, 2)]);
        let mut names = vec!["page10.pdf", "Page2.pdf", "page1.pdf"];
        names.sort_by_key(|n| natural_key(n));
        assert_eq!(names, ["page1.pdf", "Page2.pdf", "page10.pdf"]);
    }
}
