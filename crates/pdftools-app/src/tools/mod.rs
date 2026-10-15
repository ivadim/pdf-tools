//! The tools: shared state of single-file tools and the `Tool` trait every
//! tool implements. UI events arrive through [`crate::app`].

pub mod compress;
pub mod grayscale;
pub mod images;
pub mod merge;
pub mod metadata;
pub mod page_numbers;
pub mod protect;
pub mod rearrange;
pub mod rotate;
pub mod signature;
pub mod split;
pub mod unlock;
pub mod watermark;

use std::any::Any;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pdftools_core::PdfInfo;
use slint::{Model, ModelRc, SharedString, VecModel};

use crate::dialogs::Dialogs;
use crate::previews::{PreviewRequest, Previews};
use crate::{AppWindow, FileCard, GridCaps, PageTile, RunState, ToolView};

/// Tool indices, matching `Tools.catalog` in `ui/model.slint`.
pub mod id {
    pub const MERGE: usize = 0;
    pub const SPLIT: usize = 1;
    pub const COMPRESS: usize = 2;
    pub const PROTECT: usize = 3;
    pub const UNLOCK: usize = 4;
    pub const ROTATE: usize = 5;
    pub const REARRANGE: usize = 6;
    pub const PAGE_NUMBERS: usize = 7;
    pub const WATERMARK: usize = 8;
    pub const METADATA: usize = 9;
    pub const SIGNATURE: usize = 10;
    pub const GRAYSCALE: usize = 11;
    pub const IMAGES: usize = 12;
    pub const COUNT: usize = 13;
    pub const NAMES: [&str; COUNT] = [
        "merge", "split", "compress", "protect", "unlock", "rotate", "rearrange", "page-numbers", "watermark",
        "metadata", "signature", "grayscale", "images",
    ];
}

/// Thumbnail width in pixels (rendered sharp for high-DPI screens).
pub const THUMB_WIDTH: u32 = 300;

/// What every tool can reach.
pub struct Shared {
    pub ui: AppWindow,
    pub dialogs: Rc<dyn Dialogs>,
    pub previews: Previews,
    pub views: Rc<VecModel<ToolView>>,
}

impl Shared {
    pub fn view(&self, tool: usize) -> ToolView {
        self.views.row_data(tool).unwrap_or_default()
    }

    pub fn set_view(&self, tool: usize, view: ToolView) {
        self.views.set_row_data(tool, view);
    }
}

/// The result of a successful run.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub message: String,
    pub output: Option<PathBuf>,
}

/// Reports progress from a running job: message and fraction (or -1).
pub type Reporter = Box<dyn Fn(String, f32) + Send>;

/// Work done off the UI thread when the user runs a tool.
pub type Job = Box<dyn FnOnce(Reporter) -> Result<Outcome, String> + Send>;

pub trait Tool: Any {
    fn index(&self) -> usize;
    fn as_any(&mut self) -> &mut dyn Any;

    /// State shared by single-file tools (`None` for Merge).
    fn base(&mut self) -> Option<&mut SingleFile> {
        None
    }

    fn base_ref(&self) -> Option<&SingleFile> {
        None
    }

    /// Run state; single-file tools keep it in their base.
    fn run_mut(&mut self) -> &mut Run {
        &mut self.base().expect("single-file tool").run
    }

    fn run_ref(&self) -> &Run {
        &self.base_ref().expect("single-file tool").run
    }

    /// Called after a file is loaded or removed.
    fn file_changed(&mut self, _s: &mut Shared) {}

    fn option(&mut self, _s: &mut Shared, _key: &str, _value: &str) {}

    fn grid_action(&mut self, _s: &mut Shared, _action: &str) {}

    /// Page actions other than "view" and "toggle-include".
    fn tile_action(&mut self, _s: &mut Shared, _action: &str, _index: usize) {}

    fn tile_clicked(&mut self, _s: &mut Shared, _index: usize, _shift: bool, _ctrl: bool) {}

    fn tile_moved(&mut self, _s: &mut Shared, _from: usize, _to: usize) {}

    fn caps(&self) -> GridCaps {
        GridCaps { include: true, ..Default::default() }
    }

    /// Tiles of the preview, in display order.
    fn tiles(&self) -> Vec<PageTile> {
        self.base_ref().map(SingleFile::base_tiles).unwrap_or_default()
    }

    /// Adds overlay previews to a tile once its page size is known.
    fn decorate(&self, _tile: &mut PageTile, _page_size: (f32, f32)) {}

    /// Text above the grid, e.g. "custom order".
    fn notice(&self) -> String {
        String::new()
    }

    /// Extra strings and flags exposed to the tool's page.
    fn texts(&self) -> Vec<String> {
        Vec::new()
    }

    fn flags(&self) -> Vec<bool> {
        Vec::new()
    }

    /// Bumped when `texts` must replace what the user typed.
    fn form_version(&self) -> i32 {
        0
    }

    fn gray_previews(&self) -> bool {
        false
    }

    fn empty_text(&self) -> String {
        if self.base_ref().is_some_and(|b| b.file.is_some()) {
            "This file has no pages to preview.".into()
        } else {
            "Pages will appear here once you choose a file.".into()
        }
    }

    fn run_label(&self) -> String;

    /// `None` when ready, otherwise what's missing.
    fn validate(&self) -> Option<String>;

    /// Asks for an output location etc. and returns the work to do, or
    /// `None` if the user cancelled.
    fn start(&mut self, s: &mut Shared) -> Option<Job>;
}

/// A loaded input file.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub path: PathBuf,
    pub name: String,
    pub info: PdfInfo,
}

impl Loaded {
    pub fn pages(&self) -> usize {
        self.info.pages.unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default)]
pub enum FileState {
    #[default]
    Empty,
    Loading(PathBuf),
    Loaded(Loaded),
    Failed { path: PathBuf, message: String, locked: bool },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Run {
    /// 0 idle, 1 running, 2 done, 3 failed.
    pub kind: i32,
    pub progress: f32,
    pub message: String,
    pub output: Option<PathBuf>,
}

/// File, page selection and run state of a single-file tool.
pub struct SingleFile {
    pub index: usize,
    pub state: FileState,
    pub file: Option<Loaded>,
    /// Original page numbers left out of the result.
    pub excluded: BTreeSet<usize>,
    pub run: Run,
    pub pages: Rc<VecModel<PageTile>>,
    /// Page sizes in points, learned from rendering.
    pub sizes: HashMap<usize, (f32, f32)>,
    pub allow_locked: bool,
    pub thumb: Option<slint::Image>,
}

impl SingleFile {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            state: FileState::Empty,
            file: None,
            excluded: BTreeSet::new(),
            run: Run::default(),
            pages: Rc::new(VecModel::default()),
            sizes: HashMap::new(),
            allow_locked: false,
            thumb: None,
        }
    }

    pub fn page_count(&self) -> usize {
        self.file.as_ref().map_or(0, Loaded::pages)
    }

    pub fn path(&self) -> Option<&Path> {
        self.file.as_ref().map(|f| f.path.as_path())
    }

    /// Original page numbers kept in the result, in order.
    pub fn kept_pages(&self) -> Vec<usize> {
        (1..=self.page_count()).filter(|p| !self.excluded.contains(p)).collect()
    }

    /// Pages to keep for the backend: `None` when nothing was left out.
    pub fn keep(&self) -> Option<Vec<usize>> {
        (!self.excluded.is_empty()).then(|| self.kept_pages())
    }

    /// Position of an original page in the result, if kept.
    pub fn final_position(&self, page: usize) -> Option<usize> {
        if self.excluded.contains(&page) {
            return None;
        }
        Some((1..=page).filter(|p| !self.excluded.contains(p)).count())
    }

    /// " (2 pages removed)" for result messages.
    pub fn removed_note(&self) -> String {
        if self.excluded.is_empty() {
            String::new()
        } else {
            format!(" ({} removed)", plural(self.excluded.len(), "page"))
        }
    }

    pub fn tile(&self, page: usize, rotation: i32) -> PageTile {
        let path = self.path().map(Path::to_path_buf).unwrap_or_default();
        PageTile {
            key: tile_key(&path, page, rotation).into(),
            page: page as i32,
            rotation,
            aspect: 1.0 / 1.3,
            excluded: self.excluded.contains(&page),
            caption: format!(
                "Page {page} of {}{}",
                self.page_count(),
                if self.excluded.contains(&page) { " · left out" } else { "" }
            )
            .into(),
            ..Default::default()
        }
    }

    /// Every page, unrotated.
    pub fn base_tiles(&self) -> Vec<PageTile> {
        (1..=self.page_count()).map(|p| self.tile(p, 0)).collect()
    }

    pub fn card(&self) -> FileCard {
        match &self.state {
            FileState::Empty => FileCard::default(),
            FileState::Loading(path) => FileCard { state: 1, name: file_name(path).into(), ..Default::default() },
            FileState::Failed { path, message, locked } => FileCard {
                state: 3,
                name: file_name(path).into(),
                path: path.display().to_string().into(),
                error: message.into(),
                locked: *locked,
                ..Default::default()
            },
            FileState::Loaded(file) => FileCard {
                state: 2,
                name: file.name.clone().into(),
                path: file.path.display().to_string().into(),
                details: format!(
                    "{} · {}",
                    file.info.pages.map_or_else(|| "Locked".to_owned(), |p| plural(p, "page")),
                    format_bytes(file.info.file_size)
                )
                .into(),
                locked: file.info.needs_password,
                restricted: file.info.encrypted && !file.info.needs_password,
                thumb: self.thumb.clone().unwrap_or_default(),
                has_thumb: self.thumb.is_some(),
                ..Default::default()
            },
        }
    }
}

pub fn tile_key(path: &Path, page: usize, rotation: i32) -> String {
    format!("{}|{page}|{rotation}", path.display())
}

pub fn preview_request(path: &Path, page: usize, rotation: i32, gray: bool) -> PreviewRequest {
    PreviewRequest { path: path.to_path_buf(), page, width: THUMB_WIDTH, rotation, gray }
}

pub fn run_state(run: &Run, label: String, hint: Option<String>) -> RunState {
    RunState {
        hint: hint.unwrap_or_default().into(),
        label: label.into(),
        kind: run.kind,
        progress: run.progress,
        message: run.message.clone().into(),
        output: run.output.as_ref().map(|p| p.display().to_string()).unwrap_or_default().into(),
        output_name: run.output.as_deref().map(file_name).unwrap_or_default().into(),
    }
}

pub fn strings(values: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(values.into_iter().map(SharedString::from).collect::<Vec<_>>()))
}

pub fn bools(values: Vec<bool>) -> ModelRc<bool> {
    ModelRc::new(VecModel::from(values))
}

pub fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// File name without extension.
pub fn stem(path: &Path) -> String {
    path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn is_pdf(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// JPEG or PNG, judged by the extension.
pub fn is_image(path: &Path) -> bool {
    path.extension().is_some_and(|e| ["jpg", "jpeg", "png"].iter().any(|x| e.eq_ignore_ascii_case(x)))
}

pub fn plural(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = "KB";
    for next in ["MB", "GB"] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    if value < 10.0 {
        format!("{value:.1} {unit}")
    } else {
        format!("{value:.0} {unit}")
    }
}

pub fn normalize_rotation(degrees: i32) -> i32 {
    degrees.rem_euclid(360)
}

/// Asks where to save, suggesting `<input>-<suffix>.pdf` next to the input.
pub fn choose_output(dialogs: &dyn Dialogs, input: &Path, suffix: &str) -> Option<PathBuf> {
    let suggested = input.with_file_name(format!("{}-{suffix}.pdf", stem(input)));
    dialogs.save_pdf("Save PDF", &suggested)
}

/// Six text colours offered by swatches (Page numbers).
pub const TEXT_COLORS: [[u8; 3]; 6] =
    [[0x00, 0x00, 0x00], [0x1d, 0x2c, 0x6b], [0x2f, 0x5b, 0xd3], [0xc4, 0x3d, 0x3d], [0x23, 0x88, 0x5a], [0x6b, 0x72, 0x83]];

pub fn color_of(rgb: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes_and_counts() {
        assert_eq!(format_bytes(900), "900 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(50 * 1024 * 1024), "50 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(plural(1, "page"), "1 page");
        assert_eq!(plural(2, "file"), "2 files");
        assert_eq!(normalize_rotation(-90), 270);
        assert_eq!(stem(Path::new("a/b.pdf")), "b");
        assert!(is_pdf(Path::new("X.PDF")) && !is_pdf(Path::new("x.txt")));
        assert!(is_image(Path::new("a.JPEG")) && is_image(Path::new("b.png")) && !is_image(Path::new("c.gif")));
    }

    #[test]
    fn tracks_kept_pages_and_positions() {
        let mut base = SingleFile::new(id::COMPRESS);
        base.file = Some(Loaded {
            path: "f.pdf".into(),
            name: "f.pdf".into(),
            info: PdfInfo { pages: Some(5), ..Default::default() },
        });
        assert_eq!(base.keep(), None);
        base.excluded.extend([1, 4]);
        assert_eq!(base.kept_pages(), vec![2, 3, 5]);
        assert_eq!(base.keep(), Some(vec![2, 3, 5]));
        assert_eq!(base.final_position(3), Some(2));
        assert_eq!(base.final_position(4), None);
        assert_eq!(base.removed_note(), " (2 pages removed)");
        assert!(base.tile(4, 90).excluded);
        assert_eq!(base.base_tiles().len(), 5);
    }
}

/// Implements the `Tool` plumbing for a struct with a `base: SingleFile`.
#[macro_export]
macro_rules! single_file_plumbing {
    () => {
        fn index(&self) -> usize {
            self.base.index
        }
        fn as_any(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn base(&mut self) -> Option<&mut $crate::tools::SingleFile> {
            Some(&mut self.base)
        }
        fn base_ref(&self) -> Option<&$crate::tools::SingleFile> {
            Some(&self.base)
        }
    };
}

/// Loads `input` with only the `keep` pages (original numbering), applies
/// `change`, and saves to `output`.
pub fn edit<T>(
    input: &Path,
    keep: Option<&[usize]>,
    output: &Path,
    change: impl FnOnce(&mut lopdf::Document) -> Result<T, pdftools_core::Error>,
) -> Result<T, String> {
    let mut doc = pdftools_core::load_pages(input, keep).map_err(|e| e.to_string())?;
    let result = change(&mut doc).map_err(|e| e.to_string())?;
    pdftools_core::save(&mut doc, output).map_err(|e| e.to_string())?;
    Ok(result)
}
