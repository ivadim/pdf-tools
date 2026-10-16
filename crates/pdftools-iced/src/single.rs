//! Tools that work on one PDF: the file they work on, pages left out, the
//! last run, and the trait each tool implements for its options and job.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use iced::keyboard::Modifiers;
use iced::widget::svg::Handle;
use iced::{Color, Element};
use pdftools_core::PdfInfo;

use crate::app::Message;
use crate::catalog::ToolId;
use crate::dialogs::Dialogs;
use crate::widgets::{Caps, Run, TileEvent};

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
    Ready(Loaded),
    Failed { path: PathBuf, message: String, locked: bool },
}

/// Work done off the UI thread: a message for the run bar and the file
/// written, or what went wrong.
pub type Job = Box<dyn FnOnce() -> Result<(String, Option<PathBuf>), String> + Send>;

/// A page as a tool wants it shown: which source page, turned how, and
/// any marks. The app adds the preview.
#[derive(Debug, Clone, Default)]
pub struct TileSpec {
    pub page: usize,
    pub rotation: i32,
    pub label: String,
    pub badge: Option<String>,
    pub stripe: Option<Color>,
    pub excluded: bool,
    pub dimmed: bool,
    pub selected: bool,
}

/// A preview of what a tool will add to a page, positioned in fractions of
/// the page (from the top left).
#[derive(Debug, Clone)]
pub enum Overlay {
    /// `align` is where the anchor point sits in the text box: 0 start,
    /// 0.5 centre, 1 end. `size` is a fraction of the page width.
    Text { text: String, x: f32, y: f32, align_x: f32, align_y: f32, size: f32, color: Color },
    /// Centred at (x, y); `width` is a fraction of the page width.
    Image { handle: iced::widget::image::Handle, aspect: f32, x: f32, y: f32, width: f32, opacity: f32 },
}

/// What a single-file tool adds to the shared behaviour.
pub trait ToolOptions {
    fn id(&self) -> ToolId;

    /// The options panel; changes come back through [`ToolOptions::set`].
    fn view<'a>(&'a self, file: &'a Single) -> Element<'a, Message>;

    fn set(&mut self, key: &str, value: String, file: &Single, dialogs: &dyn Dialogs);

    fn caps(&self) -> Caps {
        Caps { include: true, ..Default::default() }
    }

    /// The pages shown, in order. By default every page, as it is.
    fn tiles(&self, file: &Single) -> Vec<TileSpec> {
        (1..=file.page_count())
            .map(|page| TileSpec {
                page,
                rotation: self.rotation(page),
                label: format!("p. {page}"),
                excluded: file.excluded.contains(&page),
                ..Default::default()
            })
            .collect()
    }

    /// Extra rotation shown for (and applied to) a page.
    fn rotation(&self, _page: usize) -> i32 {
        0
    }

    /// What the tool will add to a page, once its size (in points) is known.
    fn overlay(&self, _file: &Single, _tile: &TileSpec, _page_size: (f32, f32)) -> Option<Overlay> {
        None
    }

    /// A page button was used (`index` is the position in [`ToolOptions::tiles`]).
    fn tile_event(&mut self, _index: usize, _page: usize, _event: TileEvent) {}

    /// A page was clicked; return true if the tool used the click (e.g. to
    /// select it) instead of opening the viewer.
    fn clicked(&mut self, _index: usize, _page: usize, _modifiers: Modifiers) -> bool {
        false
    }

    /// Buttons above the page grid: (label, icon, action).
    fn grid_actions(&self) -> Vec<(&'static str, fn() -> Handle, &'static str)> {
        Vec::new()
    }

    fn grid_action(&mut self, _action: &str, _file: &Single) {}

    fn file_changed(&mut self, _file: &Single) {}

    /// Previews in shades of grey.
    fn gray(&self) -> bool {
        false
    }

    /// Opens password-protected files (Unlock).
    fn allow_locked(&self) -> bool {
        false
    }

    /// Shown instead of the grid when it's empty.
    fn empty_text(&self, _file: &Single) -> Option<&'static str> {
        None
    }

    /// `None` when ready, otherwise what's missing.
    fn validate(&self, file: &Single) -> Option<String>;

    fn run_label(&self) -> &'static str;

    /// Asks where to save etc. and returns the work, or `None` if cancelled.
    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job>;
}

/// Asks where to save, suggesting `<input>-<suffix>.pdf` next to the input.
pub fn choose_output(dialogs: &dyn Dialogs, file: &Single, suffix: &str) -> Option<PathBuf> {
    let input = file.path()?;
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    dialogs.save_pdf("Save PDF", &input.with_file_name(format!("{stem}-{suffix}.pdf")))
}

/// The file, page selection and run state of a single-file tool.
#[derive(Default)]
pub struct Single {
    pub state: FileState,
    /// Original page numbers left out of the result.
    pub excluded: BTreeSet<usize>,
    pub run: Run,
}

impl Single {
    pub fn loaded(&self) -> Option<&Loaded> {
        match &self.state {
            FileState::Ready(file) => Some(file),
            _ => None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.loaded().map(|f| f.path.as_path())
    }

    pub fn page_count(&self) -> usize {
        self.loaded().map_or(0, Loaded::pages)
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

    /// What's missing before the tool can run, common to all tools.
    pub fn hint(&self, caps: Caps) -> Option<String> {
        match &self.state {
            FileState::Ready(_) if caps.include && self.page_count() > 0 && self.kept_pages().is_empty() => {
                Some("Every page is left out — tick at least one to keep it.".into())
            }
            FileState::Ready(_) => None,
            _ => Some("Choose a PDF to get started.".into()),
        }
    }
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

pub fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// Loads `input` with only the `keep` pages, applies `change`, and saves.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_kept_pages() {
        let mut single = Single {
            state: FileState::Ready(Loaded {
                path: "f.pdf".into(),
                name: "f.pdf".into(),
                info: PdfInfo { pages: Some(5), ..Default::default() },
            }),
            ..Default::default()
        };
        assert_eq!(single.keep(), None);
        single.excluded.extend([1, 4]);
        assert_eq!(single.kept_pages(), vec![2, 3, 5]);
        assert_eq!(single.removed_note(), " (2 pages removed)");
        single.excluded.extend([2, 3, 5]);
        assert!(single.hint(Caps { include: true, ..Default::default() }).unwrap().contains("Every page"));
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MB");
    }
}
