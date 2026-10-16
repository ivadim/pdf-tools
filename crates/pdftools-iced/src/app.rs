//! The application: state, messages, `update` and `view`.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use iced::widget::{
    button, center, column, container, image, mouse_area, opaque, row, scrollable, space, stack, text,
};
use iced::{event, window, Alignment, Color, ContentFit, Element, Event, Length, Padding, Subscription, Task};
use pdftools_core::PdfInfo;
use pdftools_render::pages::PreviewRequest;

use crate::catalog::{self, ToolId, TOOLS};
use crate::images::{self, Images};
use crate::merge::{self, Context, Effect, Merge};
use crate::dialogs::{self, Dialogs};
use crate::icons;
use crate::previews::{self, Previews, Thumb};
use crate::single::{file_name, plural, FileState, Loaded, Single, TileSpec, ToolOptions};
use crate::theme;
use crate::tools;
use crate::widgets::{self, action, icon, icon_button, muted, run_bar, section_title, Run, Tile, TileEvent, BOLD};

/// Thumbnails are rendered this wide (sharp on high-DPI screens).
pub const THUMB_WIDTH: u32 = 300;
/// The viewer renders this many pixels wide.
const VIEWER_WIDTH: u32 = 1400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    About,
    Tool(ToolId),
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    ToggleCatalog,
    // Files.
    PickFile(ToolId),
    ClearFile(ToolId),
    OpenInUnlock(ToolId),
    FileLoaded(ToolId, PathBuf, Result<PdfInfo, String>),
    Hovering(bool),
    Dropped(PathBuf),
    // Pages.
    Tile(ToolId, usize, TileEvent),
    GridAction(ToolId, &'static str),
    IncludeAll(ToolId),
    Thumb(String, Result<Thumb, String>),
    // Options and running.
    Option(ToolId, &'static str, String),
    Run(ToolId),
    Finished(ToolId, Result<(String, Option<PathBuf>), String>),
    Reveal(ToolId),
    // Viewer.
    ViewerStep(i32),
    ViewerRotate(i32),
    CloseViewer,
    // About.
    OpenUrl(String),
    Modifiers(iced::keyboard::Modifiers),
    // Multi-file tools.
    Merge(merge::Msg),
    Images(images::Msg),
    /// A sharper image for the viewer (Images to PDF).
    ViewerImage(usize, Option<image::Handle>),
}

/// The large page view.
struct Viewer {
    tool: ToolId,
    index: usize,
}

pub struct App {
    pub screen: Screen,
    dialogs: Rc<dyn Dialogs>,
    previews: Previews,
    singles: Vec<(Single, Box<dyn ToolOptions>)>,
    pub merge: Merge,
    pub images: Images,
    viewer: Option<Viewer>,
    viewer_image: Option<(usize, image::Handle)>,
    hovering: bool,
    catalog_open: bool,
    modifiers: iced::keyboard::Modifiers,
    /// Run background work immediately and queue its messages (for tests).
    inline: bool,
    pub queued: Vec<Message>,
}

impl App {
    pub fn new(dialogs: Rc<dyn Dialogs>, inline: bool) -> Self {
        let tools: Vec<Box<dyn ToolOptions>> = vec![
            Box::new(tools::Split::default()),
            Box::new(tools::Compress::default()),
            Box::new(tools::Protect::default()),
            Box::new(tools::Unlock::default()),
            Box::new(tools::Rotate::default()),
            Box::new(tools::Rearrange::default()),
            Box::new(tools::PageNumbers::default()),
            Box::new(tools::Watermark::default()),
            Box::new(tools::Metadata::default()),
            Box::new(tools::Signature::default()),
            Box::new(tools::Grayscale::default()),
        ];
        Self {
            screen: Screen::Home,
            dialogs,
            previews: Previews::default(),
            singles: tools.into_iter().map(|t| (Single::default(), t)).collect(),
            merge: Merge::default(),
            images: Images::default(),
            viewer: None,
            viewer_image: None,
            hovering: false,
            catalog_open: true,
            modifiers: iced::keyboard::Modifiers::default(),
            inline,
            queued: Vec::new(),
        }
    }

    pub fn title(&self) -> String {
        match self.screen {
            Screen::Tool(id) => format!("{} — PDF Tools", catalog::info(id).name),
            _ => "PDF Tools".into(),
        }
    }

    fn single(&self, id: ToolId) -> Option<&(Single, Box<dyn ToolOptions>)> {
        self.singles.iter().find(|(_, t)| t.id() == id)
    }

    /// The last run of a single-file tool.
    pub fn run_of(&self, id: ToolId) -> Run {
        self.single(id).map(|(s, _)| s.run.clone()).unwrap_or_default()
    }

    fn single_mut(&mut self, id: ToolId) -> Option<&mut (Single, Box<dyn ToolOptions>)> {
        self.singles.iter_mut().find(|(_, t)| t.id() == id)
    }

    /// Runs `work` off the UI thread (or right away in inline mode).
    fn spawn<T: Send + 'static>(
        &mut self,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(T) -> Message + Send + 'static,
    ) -> Task<Message> {
        if self.inline {
            self.queued.push(then(work()));
            Task::none()
        } else {
            Task::perform(async move { work() }, then)
        }
    }

    // ── Pages ────────────────────────────────────────────────────────────

    /// The tool's tiles (what each shows, in order).
    fn specs(&self, id: ToolId) -> Vec<TileSpec> {
        self.single(id).map(|(single, tool)| tool.tiles(single)).unwrap_or_default()
    }

    fn request(&self, id: ToolId, spec: &TileSpec, width: u32) -> Option<PreviewRequest> {
        let (single, tool) = self.single(id)?;
        let path = single.path()?.to_path_buf();
        Some(PreviewRequest { path, page: spec.page, width, rotation: spec.rotation, gray: tool.gray() })
    }

    fn tiles(&self, id: ToolId) -> Vec<Tile> {
        let Some((single, tool)) = self.single(id) else { return Vec::new() };
        tool.tiles(single)
            .into_iter()
            .map(|spec| {
                let request = self.request(id, &spec, THUMB_WIDTH).expect("file loaded");
                let thumb = self.previews.get(&request).cloned();
                let overlay = thumb.as_ref().and_then(|t| tool.overlay(single, &spec, t.page_size));
                Tile {
                    key: previews::key(&request),
                    page: spec.page,
                    label: spec.label,
                    rotation: spec.rotation,
                    excluded: spec.excluded,
                    failed: self.previews.failed(&request),
                    thumb,
                    badge: spec.badge,
                    stripe: spec.stripe,
                    dimmed: spec.dimmed,
                    selected: spec.selected,
                    overlay,
                    blank: None,
                }
            })
            .collect()
    }

    fn render(&mut self, request: PreviewRequest) -> Task<Message> {
        match self.previews.start(request) {
            Some(work) => self.spawn(work, |(key, result)| Message::Thumb(key, result)),
            None => Task::none(),
        }
    }

    // ── Files ────────────────────────────────────────────────────────────

    pub fn load(&mut self, id: ToolId, path: PathBuf) -> Task<Message> {
        let Some((single, _)) = self.single_mut(id) else { return Task::none() };
        single.state = FileState::Loading(path.clone());
        single.excluded.clear();
        single.run = Run::Idle;
        let work_path = path.clone();
        self.spawn(
            move || pdftools_core::inspect(&work_path).map_err(|e| e.to_string()),
            move |result| Message::FileLoaded(id, path, result),
        )
    }

    fn file_loaded(&mut self, id: ToolId, path: PathBuf, result: Result<PdfInfo, String>) {
        let Some((single, tool)) = self.single_mut(id) else { return };
        if !matches!(&single.state, FileState::Loading(p) if *p == path) {
            return;
        }
        single.state = match result {
            Ok(info) if info.needs_password && !tool.allow_locked() => {
                FileState::Failed { path, message: "This file is password-protected.".into(), locked: true }
            }
            Ok(info) => FileState::Ready(Loaded { name: file_name(&path), path, info }),
            Err(message) => FileState::Failed { path, message, locked: false },
        };
        tool.file_changed(single);
    }

    fn run(&mut self, id: ToolId) -> Task<Message> {
        let dialogs = self.dialogs.clone();
        let Some((single, tool)) = self.single_mut(id) else { return Task::none() };
        if single.run.busy() || single.hint(tool.caps()).is_some() || tool.validate(single).is_some() {
            return Task::none();
        }
        let Some(job) = tool.start(single, &*dialogs) else { return Task::none() };
        single.run = Run::Running("Working…".into());
        self.spawn(job, move |result| Message::Finished(id, result))
    }

    fn finished(&mut self, id: ToolId, result: Result<(String, Option<PathBuf>), String>) -> Task<Message> {
        let Some((single, _)) = self.single_mut(id) else { return Task::none() };
        let input = single.path().map(Path::to_path_buf);
        match result {
            Ok((message, output)) => {
                single.run = Run::Done { message, output: output.clone() };
                // The input was overwritten: show its new state.
                if let (Some(input), Some(output)) = (input, output) {
                    if input == output {
                        self.previews.forget(&input);
                        let run = self.single(id).map(|(s, _)| s.run.clone()).unwrap_or_default();
                        let task = self.load(id, input);
                        if let Some((single, _)) = self.single_mut(id) {
                            single.run = run;
                        }
                        return task;
                    }
                }
            }
            Err(message) => single.run = Run::Failed(message),
        }
        Task::none()
    }

    // ── Multi-file tools ─────────────────────────────────────────────────

    fn multi(&mut self, tool: ToolId, effect: Option<Effect>) -> Task<Message> {
        let Some(effect) = effect else { return Task::none() };
        match effect {
            Effect::Read(files) => Task::batch(files.into_iter().map(|(id, path)| {
                self.spawn(
                    move || pdftools_core::inspect(&path).map_err(|e| e.to_string()),
                    move |result| Message::Merge(merge::Msg::Read(id, result)),
                )
            })),
            Effect::ReadImage(id, path) => {
                self.spawn(move || images::read_image(&path), move |result| Message::Images(images::Msg::Read(id, result)))
            }
            Effect::Render(request) => self.render(request),
            Effect::View(index) => {
                self.viewer = Some(Viewer { tool, index });
                self.show_viewer()
            }
            Effect::Unlock(path) => {
                self.screen = Screen::Tool(ToolId::Unlock);
                self.load(ToolId::Unlock, path)
            }
            Effect::Job(job) => self.spawn(job, move |result| match tool {
                ToolId::Images => Message::Images(images::Msg::Finished(result)),
                _ => Message::Merge(merge::Msg::Finished(result)),
            }),
        }
    }

    fn merge_update(&mut self, msg: merge::Msg) -> Task<Message> {
        let dialogs = self.dialogs.clone();
        let effect = self.merge.update(msg, &mut Context { dialogs: &*dialogs, previews: &mut self.previews });
        self.multi(ToolId::Merge, effect)
    }

    fn images_update(&mut self, msg: images::Msg) -> Task<Message> {
        let dialogs = self.dialogs.clone();
        let effect = self.images.update(msg, &mut Context { dialogs: &*dialogs, previews: &mut self.previews });
        self.multi(ToolId::Images, effect)
    }

    // ── Viewer ───────────────────────────────────────────────────────────

    fn viewer_request(&self) -> Option<PreviewRequest> {
        let viewer = self.viewer.as_ref()?;
        match viewer.tool {
            ToolId::Merge => return self.merge.ordered().get(viewer.index).map(|p| self.merge.request(p, VIEWER_WIDTH)),
            ToolId::Images => return None,
            _ => {}
        }
        let spec = self.specs(viewer.tool).into_iter().nth(viewer.index)?;
        self.request(viewer.tool, &spec, VIEWER_WIDTH)
    }

    fn viewer_count(&self) -> usize {
        match self.viewer.as_ref().map(|v| v.tool) {
            Some(ToolId::Merge) => self.merge.ordered().len(),
            Some(ToolId::Images) => self.images.entries.len(),
            Some(tool) => self.specs(tool).len(),
            None => 0,
        }
    }

    fn show_viewer(&mut self) -> Task<Message> {
        if let Some(Viewer { tool: ToolId::Images, index }) = self.viewer {
            // Photos are shown from the file itself, decoded at viewer size.
            let Some((path, rotation)) = self.images.source(index) else { return Task::none() };
            return self.spawn(
                move || {
                    let image = pdftools_core::images::load_image(&path, rotation).ok()?;
                    let image = image.resize(VIEWER_WIDTH, VIEWER_WIDTH, ::image::imageops::FilterType::Triangle).to_rgba8();
                    Some(image::Handle::from_rgba(image.width(), image.height(), image.into_raw()))
                },
                move |handle| Message::ViewerImage(index, handle),
            );
        }
        match self.viewer_request() {
            Some(request) => self.render(request),
            None => Task::none(),
        }
    }

    // ── Update ───────────────────────────────────────────────────────────

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        match self.screen {
            Screen::Tool(id) => Task::batch([task, self.sync_stage(id)]),
            _ => task,
        }
    }

    /// For tools that show one page at a time: render it, and tell the tool
    /// its proportions once known.
    fn sync_stage(&mut self, id: ToolId) -> Task<Message> {
        let Some((single, tool)) = self.single(id) else { return Task::none() };
        let (Some(page), Some(path)) = (tool.stage_page(), single.path()) else { return Task::none() };
        let request = PreviewRequest { path: path.to_path_buf(), page, width: crate::tools::STAGE_WIDTH, rotation: 0, gray: false };
        match self.previews.get(&request).map(|t| t.aspect) {
            Some(aspect) => {
                let dialogs = self.dialogs.clone();
                if let Some((single, tool)) = self.single_mut(id) {
                    tool.set("page-aspect", aspect.to_string(), single, &*dialogs);
                }
                Task::none()
            }
            None => self.render(request),
        }
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(screen) => {
                self.screen = screen;
                self.viewer = None;
            }
            Message::ToggleCatalog => self.catalog_open = !self.catalog_open,
            Message::PickFile(id) => {
                if let Some(path) = self.dialogs.pick_pdf() {
                    return self.load(id, path);
                }
            }
            Message::ClearFile(id) => {
                if let Some((single, tool)) = self.single_mut(id) {
                    *single = Single::default();
                    tool.file_changed(single);
                }
            }
            Message::OpenInUnlock(id) => {
                let path = match self.single(id).map(|(s, _)| &s.state) {
                    Some(FileState::Failed { path, .. }) => path.clone(),
                    _ => return Task::none(),
                };
                self.screen = Screen::Tool(ToolId::Unlock);
                return self.load(ToolId::Unlock, path);
            }
            Message::FileLoaded(id, path, result) => self.file_loaded(id, path, result),
            Message::Hovering(on) => self.hovering = on,
            Message::Dropped(path) => {
                self.hovering = false;
                if self.screen == Screen::Tool(ToolId::Merge) && !self.merge.run.busy() {
                    let unread = self.merge.add_files(vec![path]);
                    return self.multi(ToolId::Merge, Some(Effect::Read(unread)));
                }
                if self.screen == Screen::Tool(ToolId::Images) && !self.images.run.busy() {
                    let effect = self.images.add_files(vec![path]);
                    return self.multi(ToolId::Images, effect);
                }
                if let Screen::Tool(id) = self.screen {
                    let busy = self.single(id).is_some_and(|(s, _)| s.run.busy());
                    if !busy && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                        return self.load(id, path);
                    }
                }
            }
            Message::Tile(id, index, event) => {
                let Some(spec) = self.specs(id).into_iter().nth(index) else { return Task::none() };
                if event == TileEvent::Shown {
                    return match self.request(id, &spec, THUMB_WIDTH) {
                        Some(request) => self.render(request),
                        None => Task::none(),
                    };
                }
                let modifiers = self.modifiers;
                let Some((single, tool)) = self.single_mut(id) else { return Task::none() };
                match event {
                    TileEvent::View => {
                        if tool.clicked(index, spec.page, modifiers) {
                            return Task::none();
                        }
                        self.viewer = Some(Viewer { tool: id, index });
                        return self.show_viewer();
                    }
                    TileEvent::ToggleInclude => {
                        if !single.excluded.remove(&spec.page) {
                            single.excluded.insert(spec.page);
                        }
                    }
                    other => tool.tile_event(index, spec.page, other),
                }
                if !single.run.busy() {
                    single.run = Run::Idle;
                }
            }
            Message::GridAction(id, action) => {
                if let Some((single, tool)) = self.single_mut(id) {
                    tool.grid_action(action, single);
                    single.run = Run::Idle;
                }
            }
            Message::IncludeAll(id) => {
                if let Some((single, _)) = self.single_mut(id) {
                    single.excluded.clear();
                }
            }
            Message::Thumb(key, result) => self.previews.finished(key, result),
            Message::Option(id, key, value) => {
                let dialogs = self.dialogs.clone();
                if let Some((single, tool)) = self.single_mut(id) {
                    tool.set(key, value, single, &*dialogs);
                    // The last result described other settings.
                    if matches!(single.run, Run::Done { .. } | Run::Failed(_)) {
                        single.run = Run::Idle;
                    }
                }
            }
            Message::Run(id) => return self.run(id),
            Message::Finished(id, result) => return self.finished(id, result),
            Message::Reveal(id) => {
                if let Some((Single { run: Run::Done { output: Some(output), .. }, .. }, _)) = self.single(id) {
                    dialogs::reveal(output);
                }
            }
            Message::ViewerStep(delta) => {
                let count = self.viewer_count();
                if let Some(viewer) = &mut self.viewer {
                    let target = viewer.index as i64 + delta as i64;
                    if (0..count as i64).contains(&target) {
                        viewer.index = target as usize;
                        return self.show_viewer();
                    }
                }
            }
            Message::ViewerRotate(delta) => {
                if let Some(Viewer { tool, index }) = self.viewer {
                    let event = if delta < 0 { TileEvent::RotateLeft } else { TileEvent::RotateRight };
                    if tool == ToolId::Merge {
                        let _ = self.merge_update(merge::Msg::Tile(index, event));
                        return self.show_viewer();
                    }
                    if tool == ToolId::Images {
                        let _ = self.images_update(images::Msg::Tile(index, event));
                        return self.show_viewer();
                    }
                    let page = self.specs(tool).get(index).map_or(index + 1, |s| s.page);
                    if let Some((_, options)) = self.single_mut(tool) {
                        options.tile_event(index, page, event);
                    }
                    return self.show_viewer();
                }
            }
            Message::CloseViewer => self.viewer = None,
            Message::OpenUrl(url) => self.dialogs.open_url(&url),
            Message::Modifiers(modifiers) => self.modifiers = modifiers,
            Message::Merge(msg) => return self.merge_update(msg),
            Message::Images(msg) => return self.images_update(msg),
            Message::ViewerImage(index, handle) => {
                if let (Some(Viewer { index: shown, .. }), Some(handle)) = (&self.viewer, handle) {
                    if *shown == index {
                        self.viewer_image = Some((index, handle));
                    }
                }
            }
        }
        Task::none()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        event::listen_with(|event, _status, _window| match event {
            Event::Window(window::Event::FileHovered(_)) => Some(Message::Hovering(true)),
            Event::Window(window::Event::FilesHoveredLeft) => Some(Message::Hovering(false)),
            Event::Window(window::Event::FileDropped(path)) => Some(Message::Dropped(path)),
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => Some(Message::Modifiers(modifiers)),
            _ => None,
        })
    }

    // ── View ─────────────────────────────────────────────────────────────

    pub fn view(&self) -> Element<'_, Message> {
        let page: Element<'_, Message> = match self.screen {
            Screen::Home => self.home(),
            Screen::About => crate::about::view(),
            Screen::Tool(ToolId::Merge) => self.merge.view(&self.previews),
            Screen::Tool(ToolId::Images) => self.images.view(),
            Screen::Tool(id) if self.single(id).is_some() => self.single_page(id),
            Screen::Tool(id) => not_ported(id),
        };
        let main = row![self.sidebar(), container(page).width(Length::Fill).height(Length::Fill).style(theme::page_bg)];
        let mut layers = stack![main];
        if let Some(viewer) = self.viewer_view() {
            layers = layers.push(viewer);
        }
        if self.hovering {
            let label = match self.screen {
                Screen::Tool(ToolId::Images) => "Drop images here",
                Screen::Tool(_) => "Drop PDF files here",
                _ => "Open a tool first, then drop files",
            };
            layers = layers.push(
                container(center(text(label).size(18).font(BOLD).color(theme::ACCENT)))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Color { a: 0.12, ..theme::ACCENT }.into()),
                        border: iced::Border { width: 3.0, color: theme::ACCENT, radius: 0.0.into() },
                        ..Default::default()
                    }),
            );
        }
        layers.into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let nav = |screen: Screen, name: &'static str, chip: Element<'static, Message>| -> Element<'_, Message> {
            let active = self.screen == screen;
            button(
                row![chip, text(name).font(if active { BOLD } else { iced::Font::DEFAULT })]
                    .spacing(10)
                    .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([5, 8])
            .style(theme::nav(active))
            .on_press(Message::Navigate(screen))
            .into()
        };
        let plain_chip = |handle| -> Element<'static, Message> {
            container(icon(handle, 15.0, theme::MUTED)).center(26).style(theme::chip(theme::MUTED)).into()
        };
        let mut tools = column![nav(Screen::Home, "All tools", plain_chip(icons::home())), divider()].spacing(1);
        for info in &TOOLS {
            tools = tools.push(nav(Screen::Tool(info.id), info.name, widgets::chip(info, 26.0)));
        }
        container(
            column![
                text("PDF Tools").size(16).font(BOLD),
                tools,
                space::vertical(),
                nav(Screen::About, "About", plain_chip(icons::info())),
            ]
            .spacing(14)
            .padding(Padding { top: 18.0, right: 10.0, bottom: 18.0, left: 10.0 }),
        )
        .width(210)
        .height(Length::Fill)
        .style(theme::sidebar)
        .into()
    }

    fn home(&self) -> Element<'_, Message> {
        let card = |info: &'static catalog::ToolInfo| -> Element<'static, Message> {
            button(
                row![widgets::chip(info, 40.0), column![text(info.name).size(15).font(BOLD), muted(info.description)].spacing(3)]
                    .spacing(14),
            )
            .width(Length::Fill)
            .height(92)
            .padding(14)
            .style(theme::tool_card)
            .on_press(Message::Navigate(Screen::Tool(info.id)))
            .into()
        };
        // As many columns of at least 215 px as fit; every card 92 px tall.
        let cards = move |featured: bool| {
            iced::widget::responsive(move |size| {
                let columns = (((size.width + 12.0) / (215.0 + 12.0)).floor() as usize).max(1);
                let tools: Vec<_> = TOOLS.iter().filter(|t| t.featured == featured).collect();
                let rows = tools.chunks(columns).map(|chunk| {
                    let mut cells: Vec<Element<'_, Message>> = chunk.iter().map(|info| card(info)).collect();
                    cells.resize_with(columns, || space::horizontal().into());
                    row(cells).spacing(12).height(92).into()
                });
                column(rows).spacing(12).into()
            })
            .height(Length::Shrink)
        };
        let toggle = button(
            row![
                text("FULL TOOL CATALOG").size(12.5).font(BOLD).color(theme::CATALOG_RED).width(Length::Fill),
                icon(if self.catalog_open { icons::chevron_up() } else { icons::down() }, 16.0, theme::CATALOG_RED),
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([16, 20])
        .style(theme::tool_card)
        .on_press(Message::ToggleCatalog);
        let mut content = column![
            column![
                text("PDF Tools").size(28).font(BOLD),
                text("Everything runs on your computer — your files never leave it.").color(theme::MUTED),
            ]
            .spacing(6),
            cards(true),
            toggle,
        ]
        .spacing(16)
        .padding(Padding { top: 36.0, right: 28.0, bottom: 36.0, left: 28.0 });
        if self.catalog_open {
            content = content.push(cards(false));
        }
        scrollable(content).into()
    }

    fn single_page(&self, id: ToolId) -> Element<'_, Message> {
        let info = catalog::info(id);
        let (single, tool) = self.single(id).expect("single-file tool");
        let busy = single.run.busy();
        let caps = tool.caps();
        let tiles = self.tiles(id);

        let left = column![file_card(id, single, &self.previews, busy), tool_options(single, tool.as_ref())]
            .spacing(18)
            .padding(14);
        let left = container(scrollable(left)).width(340).height(Length::Fill).style(theme::panel);

        let kept = tiles.iter().filter(|t| !t.excluded).count();
        let summary = match tiles.len() {
            0 => String::new(),
            n if kept == n => plural(n, "page"),
            n => format!("{kept} of {} kept", plural(n, "page")),
        };
        let mut header = row![section_title("Pages"), text(summary).color(theme::MUTED).width(Length::Fill)]
            .spacing(10)
            .align_y(Alignment::Center)
            .padding(Padding { top: 0.0, right: 10.0, bottom: 0.0, left: 14.0 })
            .height(46);
        if caps.include && !single.excluded.is_empty() {
            header = header.push(action("Include all", Some(icons::check()), false, Some(Message::IncludeAll(id))));
        }
        for (label, handle, act) in tool.grid_actions() {
            header = header.push(icon_button(handle(), label, 30.0, (!busy && !tiles.is_empty()).then_some(Message::GridAction(id, act))));
        }
        let empty = tool.empty_text(single).unwrap_or(match &single.state {
            FileState::Ready(f) if f.info.needs_password => "Pages can be previewed once the file is unlocked.",
            FileState::Ready(_) => "This file has no pages to preview.",
            _ => "Pages will appear here once you choose a file.",
        });
        let right: Element<'_, Message> = match (tool.stage_page(), single.path()) {
            (Some(page), Some(path)) => {
                let request = PreviewRequest { path: path.to_path_buf(), page, width: crate::tools::STAGE_WIDTH, rotation: 0, gray: false };
                tool.stage(single, self.previews.get(&request))
            }
            _ => {
                let grid = widgets::page_grid(tiles, caps, busy, empty, move |i, e| Message::Tile(id, i, e));
                column![header, divider(), grid].into()
            }
        };
        let right = container(right).width(Length::Fill).height(Length::Fill).style(theme::panel);

        let hint = single.hint(caps).or_else(|| tool.validate(single));
        column![
            container(widgets::tool_header(info)).padding(Padding { top: 20.0, right: 24.0, bottom: 0.0, left: 24.0 }),
            row![left, right].spacing(16).padding(Padding { top: 0.0, right: 24.0, bottom: 0.0, left: 24.0 }).height(Length::Fill),
            run_bar(&single.run, hint, tool.run_label(), (info.icon)(), Message::Run(id), Message::Reveal(id)),
        ]
        .spacing(16)
        .into()
    }

    fn viewer_view(&self) -> Option<Element<'_, Message>> {
        let viewer = self.viewer.as_ref()?;
        let count = self.viewer_count();
        let (handle, rotation, can_rotate) = match viewer.tool {
            ToolId::Images => {
                let tile = self.images.tiles().into_iter().nth(viewer.index)?;
                let large = self.viewer_image.as_ref().filter(|(i, _)| *i == viewer.index).map(|(_, h)| h.clone());
                let small = match tile.overlay {
                    Some(crate::single::Overlay::Image { handle, .. }) => Some(handle),
                    _ => None,
                };
                (large.or(small), tile.rotation, true)
            }
            tool => {
                let request = self.viewer_request()?;
                // The thumbnail stands in until the large render arrives.
                let small = match tool {
                    ToolId::Merge => self.merge.ordered().get(viewer.index).map(|p| self.merge.request(p, THUMB_WIDTH)),
                    _ => self.specs(tool).get(viewer.index).and_then(|spec| self.request(tool, spec, THUMB_WIDTH)),
                };
                let thumb = self.previews.get(&request).or_else(|| small.as_ref().and_then(|r| self.previews.get(r)));
                let can_rotate = tool == ToolId::Merge || self.single(tool).is_some_and(|(_, t)| t.caps().rotate);
                (thumb.map(|t| t.handle.clone()), request.rotation, can_rotate)
            }
        };
        let page: Element<'_, Message> = match handle {
            Some(handle) => image(handle).content_fit(ContentFit::Contain).width(Length::Fill).height(Length::Fill).into(),
            None => center(text("Loading…").color(Color::WHITE)).into(),
        };
        let rotation = if rotation != 0 { format!(" · rotated {rotation}°") } else { String::new() };
        let caption = text(format!("Page {} of {count}{rotation}", viewer.index + 1)).color(Color { a: 0.8, ..Color::WHITE });
        let light = |handle, label, msg: Option<Message>| -> Element<'_, Message> {
            let tint = if msg.is_some() { Color { a: 0.8, ..Color::WHITE } } else { Color { a: 0.3, ..Color::WHITE } };
            iced::widget::tooltip(
                button(center(icon(handle, 24.0, tint))).width(44).height(44).padding(0).style(|_, _| button::Style::default()).on_press_maybe(msg),
                container(text(label).size(12)).padding([4, 8]).style(theme::hover_bar),
                iced::widget::tooltip::Position::Top,
            )
            .into()
        };
        let mut controls = row![].spacing(8);
        if can_rotate {
            controls = controls
                .push(light(icons::rotate_ccw(), "Rotate left", Some(Message::ViewerRotate(-90))))
                .push(light(icons::rotate_cw(), "Rotate right", Some(Message::ViewerRotate(90))));
        }
        let body = column![
            row![space::horizontal(), light(icons::x(), "Close", Some(Message::CloseViewer))],
            row![
                light(icons::chevron_left(), "Previous page", (viewer.index > 0).then_some(Message::ViewerStep(-1))),
                container(page).width(Length::Fill).height(Length::Fill).padding([0, 20]),
                light(icons::chevron_right(), "Next page", (viewer.index + 1 < count).then_some(Message::ViewerStep(1))),
            ]
            .align_y(Alignment::Center)
            .height(Length::Fill),
            // The caption has its own line, so the buttons stay put.
            center(caption).height(24),
            center(controls).height(48),
        ]
        .padding(12);
        let backdrop = mouse_area(container(body).width(Length::Fill).height(Length::Fill).style(theme::overlay))
            .on_press(Message::CloseViewer);
        Some(opaque(backdrop))
    }
}

fn divider<'a>() -> Element<'a, Message> {
    container(space()).width(Length::Fill).height(1).style(|_| container::Style {
        background: Some(theme::BORDER.into()),
        ..Default::default()
    })
    .into()
}

fn not_ported<'a>(id: ToolId) -> Element<'a, Message> {
    let info = catalog::info(id);
    column![
        widgets::tool_header(info),
        center(muted("This tool hasn’t been ported to the iced version yet.")),
    ]
    .padding(24)
    .into()
}

/// The chosen file (or a place to choose one).
fn file_card<'a>(id: ToolId, single: &'a Single, previews: &'a Previews, busy: bool) -> Element<'a, Message> {
    match &single.state {
        FileState::Empty => button(
            column![
                icon(icons::upload(), 30.0, theme::ACCENT),
                text("Choose a PDF").font(BOLD),
                muted("or drop it anywhere on the window"),
            ]
            .spacing(6)
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([28, 12])
        .style(|_, status| button::Style {
            background: Some(theme::SURFACE_2.into()),
            border: iced::Border {
                width: 1.5,
                color: if matches!(status, button::Status::Hovered) { theme::ACCENT } else { theme::BORDER },
                radius: 12.0.into(),
            },
            text_color: theme::TEXT,
            ..Default::default()
        })
        .on_press_maybe((!busy).then_some(Message::PickFile(id)))
        .into(),
        FileState::Loading(path) => card(text(file_name(path)).font(BOLD).into(), muted("Reading…"), None, id, busy),
        FileState::Failed { path, message, locked } => {
            let mut details = column![text(message.clone()).size(12).color(theme::ERROR)].spacing(6);
            if *locked && id != ToolId::Unlock {
                details = details.push(action("Unlock…", Some(icons::unlock()), false, Some(Message::OpenInUnlock(id))));
            }
            card(text(file_name(path)).font(BOLD).into(), details.into(), None, id, busy)
        }
        FileState::Ready(file) => {
            let pages = file.info.pages.map_or_else(|| "Locked".to_owned(), |p| plural(p, "page"));
            let request = PreviewRequest { path: file.path.clone(), page: 1, width: THUMB_WIDTH, rotation: 0, gray: false };
            let thumb = previews.get(&request).map(|t| t.handle.clone());
            card(
                text(file.name.clone()).font(BOLD).into(),
                muted(format!("{pages} · {}", crate::single::format_bytes(file.info.file_size))),
                thumb,
                id,
                busy,
            )
        }
    }
}

fn card<'a>(
    name: Element<'a, Message>,
    details: Element<'a, Message>,
    thumb: Option<iced::widget::image::Handle>,
    id: ToolId,
    busy: bool,
) -> Element<'a, Message> {
    let thumb: Element<'a, Message> = match thumb {
        Some(handle) => container(image(handle).content_fit(ContentFit::Contain)).width(46).height(60).style(theme::page).into(),
        None => container(space()).width(46).height(60).style(theme::page).into(),
    };
    container(
        row![
            thumb,
            column![name, details].spacing(3).width(Length::Fill),
            icon_button(icons::folder(), "Choose another file", 30.0, (!busy).then_some(Message::PickFile(id))),
            icon_button(icons::x(), "Remove", 30.0, (!busy).then_some(Message::ClearFile(id))),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(10)
    .style(|_| container::Style {
        background: Some(theme::SURFACE_2.into()),
        border: iced::Border { width: 1.0, color: theme::BORDER, radius: 12.0.into() },
        ..Default::default()
    })
    .into()
}

fn tool_options<'a>(single: &'a Single, tool: &'a dyn ToolOptions) -> Element<'a, Message> {
    tool.view(single)
}

/// Starts the app with native dialogs.
pub fn run() -> iced::Result {
    iced::application(
        || {
            let app = App::new(Rc::new(dialogs::NativeDialogs), false);
            (app, Task::none())
        },
        App::update,
        App::view,
    )
    .title(App::title)
    .subscription(App::subscription)
    .theme(|_: &App| theme::theme())
    .settings(iced::Settings { default_text_size: theme::FONT.into(), ..Default::default() })
    .window(window::Settings {
        size: iced::Size::new(1180.0, 760.0),
        min_size: Some(iced::Size::new(760.0, 500.0)),
        icon: window_icon(),
        ..Default::default()
    })
    .run()
}

fn window_icon() -> Option<window::Icon> {
    let png = include_bytes!("../../../packaging/icon-256.png");
    let image = ::image::load_from_memory(png).ok()?.to_rgba8();
    window::icon::from_rgba(image.to_vec(), image.width(), image.height()).ok()
}
