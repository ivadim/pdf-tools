//! Tools that work on one PDF: the file they work on, pages left out, the
//! last run, and the trait each tool implements for its options and job.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use iced::widget::svg::Handle;
use iced::Element;
use pdftools_core::PdfInfo;

use crate::app::Message;
use crate::catalog::ToolId;
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

/// What a single-file tool adds to the shared behaviour.
pub trait ToolOptions {
    fn id(&self) -> ToolId;

    /// The options panel; changes come back through [`ToolOptions::set`].
    fn view<'a>(&'a self, file: &'a Single) -> Element<'a, Message>;

    fn set(&mut self, key: &str, value: String);

    fn caps(&self) -> Caps {
        Caps { include: true, ..Default::default() }
    }

    /// Extra rotation shown for (and applied to) a page.
    fn rotation(&self, _page: usize) -> i32 {
        0
    }

    fn tile_event(&mut self, _page: usize, _event: TileEvent) {}

    /// Buttons above the page grid: (label, icon, action).
    fn grid_actions(&self) -> Vec<(&'static str, fn() -> Handle, &'static str)> {
        Vec::new()
    }

    fn grid_action(&mut self, _action: &str, _pages: usize) {}

    fn file_changed(&mut self, _file: &Single) {}

    /// Previews in shades of grey.
    fn gray(&self) -> bool {
        false
    }

    /// Opens password-protected files (Unlock).
    fn allow_locked(&self) -> bool {
        false
    }

    /// `None` when ready, otherwise what's missing.
    fn validate(&self, file: &Single) -> Option<String>;

    fn run_label(&self) -> &'static str;

    /// Suggested output name: `<input>-<suffix>.pdf`.
    fn suffix(&self) -> &'static str;

    fn job(&self, file: &Single, output: PathBuf) -> Job;
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
