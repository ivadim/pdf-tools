//! The application: owns the tools and services, and turns UI callbacks
//! into tool actions.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::ToImage;
use crate::dialogs::Dialogs;
use crate::jobs;
use crate::previews::{PreviewRequest, Previews};
use crate::tools::images::ImagesTool;
use crate::tools::merge::MergeTool;
use crate::tools::signature::SignatureTool;
use crate::tools::{self, id, preview_request, run_state, FileState, Loaded, Outcome, Shared, Tool};
use crate::{About, AppWindow, Merge, PageTile, Sign, ToolView, Tools, Viewer};

pub struct App {
    pub s: Shared,
    tools: Vec<Option<Box<dyn Tool>>>,
    /// Preview renders requested but not yet delivered.
    in_flight: HashSet<String>,
    viewer: Option<(usize, usize)>,
}

/// The large viewer renders this many pixels wide.
const VIEWER_WIDTH: u32 = 1400;

impl App {
    /// Builds the app for `ui` and connects every callback.
    pub fn new(ui: &AppWindow, dialogs: Rc<dyn Dialogs>) -> Rc<RefCell<App>> {
        let views = Rc::new(VecModel::from(vec![ToolView::default(); id::COUNT]));
        ui.global::<Tools>().set_views(ModelRc::from(views.clone()));
        let tools: Vec<Option<Box<dyn Tool>>> = vec![
            Some(Box::new(MergeTool::new())),
            Some(Box::new(tools::split::SplitTool::new())),
            Some(Box::new(tools::compress::CompressTool::new())),
            Some(Box::new(tools::protect::ProtectTool::new())),
            Some(Box::new(tools::unlock::UnlockTool::new())),
            Some(Box::new(tools::rotate::RotateTool::new())),
            Some(Box::new(tools::rearrange::RearrangeTool::new())),
            Some(Box::new(tools::page_numbers::PageNumbersTool::new())),
            Some(Box::new(tools::watermark::WatermarkTool::new())),
            Some(Box::new(tools::metadata::MetadataTool::new())),
            Some(Box::new(SignatureTool::new())),
            Some(Box::new(tools::grayscale::GrayscaleTool::new())),
            Some(Box::new(ImagesTool::new())),
        ];
        let app = Rc::new(RefCell::new(App {
            s: Shared { ui: ui.clone_strong(), dialogs, previews: Previews::new(), views },
            tools,
            in_flight: HashSet::new(),
            viewer: None,
        }));
        let about = ui.global::<About>();
        about.set_version(env!("CARGO_PKG_VERSION").into());
        about.set_platform(platform().into());
        about.set_repository(env!("CARGO_PKG_REPOSITORY").into());
        jobs::register(&app);
        {
            let mut a = app.borrow_mut();
            for i in 0..id::COUNT {
                a.refresh(i);
            }
        }
        connect(ui, &app);
        app
    }

    /// Runs `f` with a tool taken out of the list, so it can use `Shared`.
    pub fn with_tool<R>(&mut self, index: usize, f: impl FnOnce(&mut dyn Tool, &mut Shared) -> R) -> R {
        let mut tool = self.tools[index].take().expect("tool is not re-entered");
        let result = f(tool.as_mut(), &mut self.s);
        self.tools[index] = Some(tool);
        result
    }

    pub fn tool<T: 'static>(&mut self, index: usize) -> &mut T {
        self.tools[index].as_mut().expect("tool present").as_any().downcast_mut::<T>().expect("tool type")
    }

    // ── Views ─────────────────────────────────────────────────────────────

    /// Rebuilds a tool's view from its state and requests missing previews.
    pub fn refresh(&mut self, index: usize) {
        if index == id::MERGE {
            let tool = self.tools[index].take().expect("merge");
            let mut merge = tool;
            merge.as_any().downcast_mut::<MergeTool>().expect("merge").refresh(&mut self.s);
            self.tools[index] = Some(merge);
        } else if index == id::IMAGES {
            self.with_tool(index, |tool, s| {
                (tool as &mut dyn std::any::Any).downcast_mut::<ImagesTool>().expect("images").refresh(s)
            });
            self.read_images();
        } else {
            self.with_tool(index, |tool, s| refresh_single(tool, s));
        }
        self.request_previews(index, None);
        if index == id::SIGNATURE {
            self.with_tool(index, |tool, s| {
                if let Some(sig) = (tool as &mut dyn std::any::Any).downcast_mut::<SignatureTool>() {
                    sig.refresh_page(s);
                }
            });
        }
    }

    /// Asks for previews of tiles that have none; `urgent` ones first.
    pub fn request_previews(&mut self, index: usize, urgent: Option<(usize, usize)>) {
        let tiles: Vec<PageTile> = self.s.view(index).pages.iter().collect();
        let gray = self.with_tool(index, |tool, _| tool.gray_previews());
        let order: Vec<usize> = match urgent {
            Some((first, last)) => (first..=last.min(tiles.len().saturating_sub(1))).collect(),
            None => (0..tiles.len()).collect(),
        };
        for i in order {
            let tile = &tiles[i];
            if tile.loaded || tile.failed {
                continue;
            }
            let Some(request) = tile_request(tile, gray) else { continue };
            let key = format!("{}|{gray}", tile.key);
            if !self.in_flight.insert(key.clone()) && urgent.is_none() {
                continue;
            }
            let tile_key = tile.key.clone();
            self.s.previews.request(request, urgent.is_some(), move |app: &mut App, result| {
                app.in_flight.remove(&key);
                app.apply_preview(index, &tile_key, result);
            });
        }
    }

    fn apply_preview(&mut self, index: usize, key: &SharedString, result: Result<(slint::Image, f32, (f32, f32)), String>) {
        let pages = self.s.view(index).pages;
        self.with_tool(index, |tool, _| {
            for row in 0..pages.row_count() {
                let Some(mut tile) = pages.row_data(row) else { continue };
                if &tile.key != key {
                    continue;
                }
                match &result {
                    Ok((image, aspect, size)) => {
                        tile.image = image.clone();
                        tile.aspect = *aspect;
                        tile.loaded = true;
                        if let Some(base) = tool.base() {
                            base.sizes.insert(tile.page as usize, *size);
                        }
                        tool.decorate(&mut tile, *size);
                    }
                    Err(_) => tile.failed = true,
                }
                pages.set_row_data(row, tile);
            }
        });
    }

    // ── Files ─────────────────────────────────────────────────────────────

    /// Opens `path` in a single-file tool, or adds it to Merge or Images.
    pub fn load_file(&mut self, index: usize, path: PathBuf) {
        if index == id::MERGE {
            self.tool::<MergeTool>(index).add_files(vec![path]);
            self.refresh(index);
            self.inspect_merge_files();
            return;
        }
        if index == id::IMAGES {
            self.tool::<ImagesTool>(index).add_files(vec![path]);
            self.refresh(index);
            return;
        }
        self.with_tool(index, |tool, s| {
            let base = tool.base().expect("single-file tool");
            if let Some(old) = &base.file {
                s.previews.forget(&old.path);
            }
            base.state = FileState::Loading(path.clone());
            base.file = None;
            base.thumb = None;
            base.run = Default::default();
        });
        self.refresh(index);
        let work_path = path.clone();
        jobs::spawn(
            move || pdftools_core::inspect(&work_path).map_err(|e| e.to_string()),
            move |app: &mut App, result| app.file_loaded(index, path, result),
        );
    }

    fn file_loaded(&mut self, index: usize, path: PathBuf, result: Result<pdftools_core::PdfInfo, String>) {
        let still_wanted = self.with_tool(index, |tool, _| {
            matches!(&tool.base().expect("single").state, FileState::Loading(p) if *p == path)
        });
        if !still_wanted {
            return;
        }
        self.with_tool(index, |tool, s| {
            let base = tool.base().expect("single-file tool");
            base.excluded.clear();
            base.sizes.clear();
            match result {
                Ok(info) if info.needs_password && !base.allow_locked => {
                    base.state = FileState::Failed { path: path.clone(), message: "This file is password-protected.".into(), locked: true };
                }
                Ok(info) => {
                    let loaded = Loaded { name: tools::file_name(&path), path: path.clone(), info };
                    base.state = FileState::Loaded(loaded.clone());
                    base.file = Some(loaded);
                }
                Err(message) => {
                    base.state = FileState::Failed { path: path.clone(), message, locked: false };
                }
            }
            tool.file_changed(s);
        });
        self.refresh(index);
        // A small render of page 1 for the file card.
        let has_pages = self.with_tool(index, |tool, _| tool.base_ref().is_some_and(|b| b.page_count() > 0));
        if has_pages {
            let request = PreviewRequest { path: path.clone(), page: 1, width: 120, rotation: 0, gray: false };
            self.s.previews.request(request, true, move |app: &mut App, result| {
                if let Ok((image, _, _)) = result {
                    app.with_tool(index, |tool, _| {
                        if let Some(base) = tool.base() {
                            if base.path() == Some(path.as_path()) {
                                base.thumb = Some(image);
                            }
                        }
                    });
                    app.refresh(index);
                }
            });
        }
    }

    pub fn clear_file(&mut self, index: usize) {
        self.with_tool(index, |tool, s| {
            let base = tool.base().expect("single-file tool");
            if let Some(file) = &base.file {
                s.previews.forget(&file.path);
            }
            base.state = FileState::Empty;
            base.file = None;
            base.thumb = None;
            base.excluded.clear();
            base.run = Default::default();
            tool.file_changed(s);
        });
        self.refresh(index);
    }

    /// Reads page counts of Merge's files that are still loading.
    pub fn inspect_merge_files(&mut self) {
        for (entry, path) in self.tool::<MergeTool>(id::MERGE).take_unread() {
            let path_for_thumb = path.clone();
            jobs::spawn(
                move || pdftools_core::inspect(&path).map_err(|e| e.to_string()),
                move |app: &mut App, result| {
                    let readable = matches!(&result, Ok(info) if info.pages.is_some_and(|p| p > 0));
                    app.tool::<MergeTool>(id::MERGE).file_read(entry, result);
                    app.refresh(id::MERGE);
                    // Page 1 doubles as the file's thumbnail in the list.
                    if readable {
                        let request = preview_request(&path_for_thumb, 1, 0, false);
                        app.s.previews.request(request, true, |app: &mut App, _| app.refresh(id::MERGE));
                    }
                },
            );
        }
    }

    /// Reads sizes and thumbnails of new images, one at a time in a single
    /// background job, so large photos are never decoded all at once.
    fn read_images(&mut self) {
        let unread = self.tool::<ImagesTool>(id::IMAGES).take_unread();
        if unread.is_empty() {
            return;
        }
        let listener = jobs::listener(|app: &mut App, (entry, result): (u32, tools::images::ReadResult)| {
            app.tool::<ImagesTool>(id::IMAGES).image_read(entry, result);
            app.refresh(id::IMAGES);
        });
        jobs::spawn(
            move || {
                for (entry, path) in unread {
                    jobs::post(listener, Box::new((entry, tools::images::read_image(&path))));
                }
            },
            move |_: &mut App, ()| jobs::forget(listener),
        );
    }

    /// Files dropped onto the window.
    pub fn files_dropped(&mut self, paths: Vec<PathBuf>) {
        let active = self.s.ui.global::<Tools>().get_active().to_string();
        let Some(index) = id::NAMES.iter().position(|n| *n == active) else { return };
        if index == id::IMAGES {
            if !self.busy(index) {
                self.tool::<ImagesTool>(index).add_files(paths);
                self.refresh(index);
            }
            return;
        }
        let pdfs: Vec<PathBuf> = paths.into_iter().filter(|p| tools::is_pdf(p)).collect();
        if pdfs.is_empty() || self.busy(index) {
            return;
        }
        if index == id::MERGE {
            self.tool::<MergeTool>(index).add_files(pdfs);
            self.refresh(index);
            self.inspect_merge_files();
        } else {
            self.load_file(index, pdfs[0].clone());
        }
    }

    fn busy(&mut self, index: usize) -> bool {
        self.with_tool(index, |tool, _| tool.run_ref().kind == 1)
    }

    // ── Running ───────────────────────────────────────────────────────────

    pub fn run(&mut self, index: usize) {
        let ready = self.with_tool(index, |tool, _| tool.validate().is_none() && tool.run_ref().kind != 1);
        if !ready {
            return;
        }
        let Some(job) = self.with_tool(index, |tool, s| tool.start(s)) else {
            return;
        };
        self.with_tool(index, |tool, _| {
            *tool.run_mut() = tools::Run { kind: 1, progress: -1.0, message: "Working…".into(), output: None };
        });
        self.refresh(index);

        let listener = jobs::listener(move |app: &mut App, (message, fraction): (String, f32)| {
            app.with_tool(index, |tool, _| {
                let run = tool.run_mut();
                if run.kind == 1 {
                    run.message = message;
                    run.progress = fraction;
                }
            });
            app.refresh(index);
        });
        let reporter: tools::Reporter = Box::new(move |message, fraction| jobs::post(listener, Box::new((message, fraction))));
        jobs::spawn(move || job(reporter), move |app: &mut App, result: Result<Outcome, String>| {
            jobs::forget(listener);
            app.finished(index, result);
        });
    }

    fn finished(&mut self, index: usize, result: Result<Outcome, String>) {
        let overwritten = self.with_tool(index, |tool, _| {
            let input = tool.base_ref().and_then(|b| b.path().map(Path::to_path_buf));
            let run = tool.run_mut();
            match result {
                Ok(outcome) => {
                    *run = tools::Run { kind: 2, progress: 1.0, message: outcome.message, output: outcome.output.clone() };
                    input.filter(|i| outcome.output.as_deref() == Some(i.as_path()))
                }
                Err(message) => {
                    *run = tools::Run { kind: 3, progress: 0.0, message, output: None };
                    None
                }
            }
        });
        self.refresh(index);
        // If the input was overwritten, show its new state.
        if let Some(path) = overwritten {
            self.s.previews.forget(&path);
            let run = self.with_tool(index, |tool, _| tool.run_ref().clone());
            self.load_file(index, path);
            self.with_tool(index, |tool, _| *tool.run_mut() = run);
        }
    }

    // ── Viewer ────────────────────────────────────────────────────────────

    pub fn open_viewer(&mut self, index: usize, page: usize) {
        let count = self.s.view(index).pages.row_count();
        if page >= count {
            return;
        }
        self.viewer = Some((index, page));
        self.show_viewer();
    }

    fn show_viewer(&mut self) {
        let Some((index, page)) = self.viewer else { return };
        let view = self.s.view(index);
        let count = view.pages.row_count();
        let Some(tile) = view.pages.row_data(page) else { return };
        let viewer = self.s.ui.global::<Viewer>();
        viewer.set_caption(format!("{}{}", tile.caption, if tile.rotation != 0 { format!(" · rotated {}°", tile.rotation) } else { String::new() }).into());
        viewer.set_can_prev(page > 0);
        viewer.set_can_next(page + 1 < count);
        viewer.set_can_rotate(view.caps.rotate);
        viewer.set_open(true);
        if index == id::IMAGES {
            // The tile already shows the thumbnail; swap in a sharper image.
            viewer.set_tile(tile);
            let Some((path, rotation)) = self.tool::<ImagesTool>(index).source(page) else { return };
            jobs::spawn(
                move || {
                    let image = pdftools_core::images::load_image(&path, rotation).ok()?;
                    let image = image.resize(VIEWER_WIDTH, VIEWER_WIDTH, image::imageops::FilterType::Triangle).to_rgba8();
                    Some(crate::raster::Picture { width: image.width(), height: image.height(), pixels: image.into_raw() })
                },
                move |app: &mut App, picture: Option<crate::raster::Picture>| {
                    if let (Some(picture), true) = (picture, app.viewer == Some((index, page))) {
                        let viewer = app.s.ui.global::<Viewer>();
                        let mut shown = viewer.get_tile();
                        shown.overlay_image = picture.to_image();
                        viewer.set_tile(shown);
                    }
                },
            );
            return;
        }
        viewer.set_tile(PageTile { loaded: false, ..tile.clone() });
        let gray = self.with_tool(index, |tool, _| tool.gray_previews());
        let Some(mut request) = tile_request(&tile, gray) else { return };
        request.width = VIEWER_WIDTH;
        self.s.previews.request(request, true, move |app: &mut App, result| {
            if app.viewer != Some((index, page)) {
                return;
            }
            let viewer = app.s.ui.global::<Viewer>();
            let mut shown = viewer.get_tile();
            match result {
                Ok((image, aspect, _)) => {
                    shown.image = image;
                    shown.aspect = aspect;
                    shown.loaded = true;
                }
                Err(_) => shown.failed = true,
            }
            viewer.set_tile(shown);
        });
    }

    pub fn viewer_step(&mut self, delta: i32) {
        if let Some((index, page)) = self.viewer {
            let count = self.s.view(index).pages.row_count() as i32;
            let target = page as i32 + delta;
            if (0..count).contains(&target) {
                self.viewer = Some((index, target as usize));
                self.show_viewer();
            }
        }
    }

    pub fn viewer_rotate(&mut self, delta: i32) {
        if let Some((index, page)) = self.viewer {
            let action = if delta < 0 { "rotate-left" } else { "rotate-right" };
            self.tile_action(index, action, page);
            self.show_viewer();
        }
    }

    pub fn close_viewer(&mut self) {
        self.viewer = None;
        self.s.ui.global::<Viewer>().set_open(false);
    }

    // ── Page actions shared by all tools ──────────────────────────────────

    pub fn tile_action(&mut self, index: usize, action: &str, tile: usize) {
        match action {
            "view" => return self.open_viewer(index, tile),
            "toggle-include" if is_single(index) => {
                self.with_tool(index, |tool, _| {
                    let page = tool.tiles().get(tile).map(|t| t.page as usize);
                    if let (Some(page), Some(base)) = (page, tool.base()) {
                        if !base.excluded.remove(&page) {
                            base.excluded.insert(page);
                        }
                    }
                });
            }
            _ => self.with_tool(index, |tool, s| tool.tile_action(s, action, tile)),
        }
        self.refresh(index);
    }

    pub fn include_range(&mut self, index: usize, from: usize, to: usize, include: bool) {
        if index == id::MERGE {
            self.tool::<MergeTool>(index).include_range(from, to, include);
        } else if is_single(index) {
            self.with_tool(index, |tool, _| {
                let pages: Vec<usize> = tool.tiles().iter().map(|t| t.page as usize).collect();
                if let Some(base) = tool.base() {
                    for page in pages.iter().skip(from).take(to + 1 - from.min(to + 1)) {
                        if include {
                            base.excluded.remove(page);
                        } else {
                            base.excluded.insert(*page);
                        }
                    }
                }
            });
        }
        self.refresh(index);
    }

    pub fn grid_action(&mut self, index: usize, action: &str) {
        if action == "include-all" && is_single(index) {
            self.with_tool(index, |tool, _| tool.base().expect("single").excluded.clear());
        } else {
            self.with_tool(index, |tool, s| tool.grid_action(s, action));
        }
        self.refresh(index);
    }
}

/// The system and processor, as people know them: "macOS arm64".
pub fn platform() -> String {
    let os = match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    };
    format!("{os} {arch}")
}

/// Tools that work on one PDF (everything but Merge and Images).
fn is_single(index: usize) -> bool {
    index != id::MERGE && index != id::IMAGES
}

/// The preview request for a tile (its key encodes path, page and rotation).
fn tile_request(tile: &PageTile, gray: bool) -> Option<PreviewRequest> {
    // Merge prefixes the key with its file id: "3#path|page|rotation".
    let key = match tile.key.split_once('#') {
        Some((id, rest)) if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) => rest,
        _ => tile.key.as_str(),
    };
    let mut parts = key.rsplitn(3, '|');
    let rotation: i32 = parts.next()?.parse().ok()?;
    let page: usize = parts.next()?.parse().ok()?;
    let path = PathBuf::from(parts.next()?);
    Some(preview_request(&path, page, rotation, gray))
}

/// Builds a single-file tool's view; previews already rendered are reused.
fn refresh_single(tool: &mut dyn Tool, s: &mut Shared) {
    let index = tool.index();
    // A new requirement matters more than the previous run's result.
    if matches!(tool.run_ref().kind, 2 | 3) && tool.base_ref().is_some_and(|b| b.file.is_some()) && tool.validate().is_some() {
        *tool.run_mut() = tools::Run::default();
    }
    let gray = tool.gray_previews();
    let mut tiles = tool.tiles();
    let caps = tool.caps();
    let base = tool.base_ref().expect("single-file tool");
    for tile in &mut tiles {
        if let Some(request) = tile_request(tile, gray) {
            if let Some((image, aspect, size)) = s.previews.cached(&request) {
                tile.image = image;
                tile.aspect = aspect;
                tile.loaded = true;
                tool.decorate(tile, size);
            } else if let Some(size) = base.sizes.get(&(tile.page as usize)) {
                tool.decorate(tile, *size);
            }
        }
    }
    let kept = tiles.iter().filter(|t| !t.excluded).count();
    let summary = if tiles.is_empty() {
        String::new()
    } else if kept == tiles.len() {
        tools::plural(tiles.len(), "page")
    } else {
        format!("{kept} of {} kept", tools::plural(tiles.len(), "page"))
    };
    let hint = if base.file.is_none() {
        Some("Choose a PDF to get started.".to_owned())
    } else if caps.include && base.page_count() > 0 && base.kept_pages().is_empty() {
        Some("Every page is left out — tick at least one to keep it.".to_owned())
    } else {
        tool.validate()
    };
    let run = run_state(&base.run, tool.run_label(), hint);
    let card = base.card();
    let include_all = caps.include && !base.excluded.is_empty();
    base.pages.set_vec(tiles);
    let mut texts = tool.texts();
    texts.resize(texts.len().max(8), String::new());
    let mut flags = tool.flags();
    flags.resize(flags.len().max(4), false);
    s.set_view(
        index,
        ToolView {
            file: card,
            pages: ModelRc::from(base.pages.clone()),
            caps,
            run,
            summary: summary.into(),
            include_all,
            notice: tool.notice().into(),
            empty_text: tool.empty_text().into(),
            texts: tools::strings(texts),
            form_version: tool.form_version(),
            flags: tools::bools(flags),
        },
    );
}

/// Runs `f` with the app, or later if the app is busy (e.g. a callback fired
/// while a native dialog was open), then delivers finished background work.
fn dispatch(app: &Weak<RefCell<App>>, f: impl FnOnce(&mut App) + 'static) {
    let Some(strong) = app.upgrade() else { return };
    let Ok(mut guard) = strong.try_borrow_mut() else {
        let app = app.clone();
        slint::Timer::single_shot(Duration::from_millis(1), move || dispatch(&app, f));
        return;
    };
    f(&mut guard);
    drop(guard);
    jobs::drain();
}

fn connect(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    let tools_global = ui.global::<Tools>();
    let weak = Rc::downgrade(app);
    let ui_weak = ui.as_weak();

    macro_rules! on {
        ($global:expr, $setter:ident, |$app:ident $(, $arg:ident : $ty:ty)*| $body:block) => {{
            let weak = weak.clone();
            $global.$setter(move |$($arg: $ty),*| {
                $(let $arg = $arg.clone();)*
                dispatch(&weak, move |$app: &mut App| $body);
            });
        }};
    }

    {
        let ui_weak = ui_weak.clone();
        let weak = weak.clone();
        tools_global.on_navigate(move |target| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.global::<Tools>().set_active(target.clone());
            }
            dispatch(&weak, |app| app.close_viewer());
        });
    }
    on!(tools_global, on_pick_file, |app, tool: i32| {
        let index = tool as usize;
        if index == id::MERGE {
            return;
        }
        if index == id::IMAGES {
            let paths = app.s.dialogs.pick_images();
            app.tool::<ImagesTool>(index).add_files(paths);
            app.refresh(index);
            return;
        }
        if let Some(path) = app.s.dialogs.pick_pdf() {
            app.load_file(index, path);
        }
    });
    on!(tools_global, on_clear_file, |app, tool: i32| { app.clear_file(tool as usize) });
    on!(tools_global, on_unlock_file, |app, tool: i32| {
        let path = app.with_tool(tool as usize, |t, _| match &t.base().expect("single").state {
            FileState::Failed { path, .. } => Some(path.clone()),
            _ => None,
        });
        if let Some(path) = path {
            app.s.ui.global::<Tools>().set_active("unlock".into());
            app.load_file(id::UNLOCK, path);
        }
    });
    on!(tools_global, on_run, |app, tool: i32| { app.run(tool as usize) });
    on!(tools_global, on_reveal, |app, tool: i32| {
        let output = app.with_tool(tool as usize, |t, _| t.run_ref().output.clone());
        if let Some(output) = output {
            crate::dialogs::reveal(&output);
        }
    });
    on!(tools_global, on_tile_clicked, |app, tool: i32, index: i32, shift: bool, ctrl: bool| {
        app.with_tool(tool as usize, |t, s| t.tile_clicked(s, index as usize, shift, ctrl));
        app.refresh(tool as usize);
    });
    on!(tools_global, on_tile_action, |app, tool: i32, action: SharedString, index: i32| {
        app.tile_action(tool as usize, &action, index as usize)
    });
    on!(tools_global, on_tile_moved, |app, tool: i32, from: i32, to: i32| {
        app.with_tool(tool as usize, |t, s| t.tile_moved(s, from as usize, to.max(0) as usize));
        app.refresh(tool as usize);
    });
    on!(tools_global, on_include_range, |app, tool: i32, from: i32, to: i32, include: bool| {
        app.include_range(tool as usize, from as usize, to as usize, include)
    });
    on!(tools_global, on_visible_range, |app, tool: i32, first: i32, last: i32| {
        if last >= first {
            app.request_previews(tool as usize, Some((first.max(0) as usize, last as usize)));
        }
    });
    on!(tools_global, on_grid_action, |app, tool: i32, action: SharedString| { app.grid_action(tool as usize, &action) });
    on!(tools_global, on_option, |app, tool: i32, key: SharedString, value: SharedString| {
        app.with_tool(tool as usize, |t, s| {
            t.option(s, &key, &value);
            // The last result described other settings.
            if matches!(t.run_ref().kind, 2 | 3) {
                *t.run_mut() = tools::Run::default();
            }
        });
        app.refresh(tool as usize);
    });

    on!(ui.global::<About>(), on_open, |app, url: SharedString| { app.s.dialogs.open_url(&url) });

    let viewer = ui.global::<Viewer>();
    on!(viewer, on_step, |app, delta: i32| { app.viewer_step(delta) });
    on!(viewer, on_rotate, |app, delta: i32| { app.viewer_rotate(delta) });
    on!(viewer, on_close, |app| { app.close_viewer() });

    let merge = ui.global::<Merge>();
    on!(merge, on_add_files, |app| {
        let paths = app.s.dialogs.pick_pdfs();
        app.tool::<MergeTool>(id::MERGE).add_files(paths);
        app.refresh(id::MERGE);
        app.inspect_merge_files();
    });
    on!(merge, on_sort_files, |app| {
        app.tool::<MergeTool>(id::MERGE).sort_by_name();
        app.refresh(id::MERGE);
    });
    on!(merge, on_clear_files, |app| {
        app.with_tool(id::MERGE, |t, s| (t as &mut dyn std::any::Any).downcast_mut::<MergeTool>().expect("merge").clear(s));
        app.refresh(id::MERGE);
    });
    on!(merge, on_file_action, |app, action: SharedString, index: i32| {
        let index = index as usize;
        match action.as_str() {
            "unlock" => {
                if let Some(path) = app.tool::<MergeTool>(id::MERGE).path_of(index) {
                    app.s.ui.global::<Tools>().set_active("unlock".into());
                    app.load_file(id::UNLOCK, path);
                }
                return;
            }
            _ => {
                app.with_tool(id::MERGE, |t, s| {
                    (t as &mut dyn std::any::Any).downcast_mut::<MergeTool>().expect("merge").file_action(s, &action, index)
                });
            }
        }
        if action != "focus" {
            app.refresh(id::MERGE);
        }
    });
    on!(merge, on_file_moved, |app, from: i32, to: i32| {
        app.tool::<MergeTool>(id::MERGE).move_file(from as usize, to.max(0) as usize);
        app.refresh(id::MERGE);
    });

    let sign = ui.global::<Sign>();
    macro_rules! sig {
        ($app:ident, |$tool:ident, $s:ident| $body:expr) => {
            $app.with_tool(id::SIGNATURE, |t, $s| {
                let $tool = (t as &mut dyn std::any::Any).downcast_mut::<SignatureTool>().expect("signature");
                $body
            })
        };
    }
    on!(sign, on_pad_down, |app, x: f32, y: f32| { sig!(app, |tool, s| tool.pad_down(s, x, y)) });
    on!(sign, on_pad_move, |app, x: f32, y: f32| { sig!(app, |tool, s| tool.pad_move(s, x, y)) });
    on!(sign, on_pad_up, |app| {
        sig!(app, |tool, s| tool.pad_up(s));
        app.refresh(id::SIGNATURE);
    });
    on!(sign, on_pad_clear, |app| {
        sig!(app, |tool, s| tool.pad_clear(s));
        app.refresh(id::SIGNATURE);
    });
    on!(sign, on_box_changed, |app, box_id: i32, x: f32, y: f32, w: f32, h: f32| {
        sig!(app, |tool, s| tool.box_changed(s, box_id, x, y, w, h))
    });
    on!(sign, on_box_removed, |app, box_id: i32| {
        sig!(app, |tool, s| tool.box_removed(s, box_id));
        app.refresh(id::SIGNATURE);
    });
    on!(sign, on_page_step, |app, delta: i32| {
        sig!(app, |tool, s| tool.page_step(s, delta));
        app.refresh(id::SIGNATURE);
    });
    on!(sign, on_add_box, |app, every: bool| {
        sig!(app, |tool, s| tool.add_box(s, every));
        app.refresh(id::SIGNATURE);
    });
    on!(sign, on_choose_image, |app| {
        sig!(app, |tool, s| tool.choose_image(s));
        app.refresh(id::SIGNATURE);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::THUMB_WIDTH;

    #[test]
    fn tile_keys_round_trip_into_requests() {
        let tile = PageTile { key: format!("C:\\a|b\\c.pdf|{}|{}", 3, 90).into(), ..Default::default() };
        let request = tile_request(&tile, true).unwrap();
        assert_eq!(request.path, PathBuf::from("C:\\a|b\\c.pdf"));
        assert_eq!((request.page, request.rotation, request.gray, request.width), (3, 90, true, THUMB_WIDTH));
        assert!(tile_request(&PageTile::default(), false).is_none());
    }
}
