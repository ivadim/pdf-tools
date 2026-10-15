//! Native dialogs behind a trait, so tests can script the answers.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub trait Dialogs {
    fn pick_pdf(&self) -> Option<PathBuf>;
    fn pick_pdfs(&self) -> Vec<PathBuf>;
    fn pick_folder(&self, near: &Path) -> Option<PathBuf>;
    fn pick_image(&self) -> Option<PathBuf>;
    fn pick_images(&self) -> Vec<PathBuf>;
    /// Where to save a PDF; the result always ends in `.pdf`.
    fn save_pdf(&self, title: &str, suggested: &Path) -> Option<PathBuf>;
    /// A yes/no question; true for yes.
    fn confirm(&self, title: &str, message: &str) -> bool;
}

pub struct NativeDialogs;

const PDF: (&str, &[&str]) = ("PDF documents", &["pdf"]);
const IMAGES: (&str, &[&str]) = ("Images", &["jpg", "jpeg", "png"]);

impl Dialogs for NativeDialogs {
    fn pick_pdf(&self) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title("Choose a PDF").add_filter(PDF.0, PDF.1).pick_file()
    }

    fn pick_pdfs(&self) -> Vec<PathBuf> {
        rfd::FileDialog::new().set_title("Add PDF files").add_filter(PDF.0, PDF.1).pick_files().unwrap_or_default()
    }

    fn pick_folder(&self, near: &Path) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title("Choose where to save the files");
        if let Some(dir) = near.parent() {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_folder()
    }

    fn pick_image(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Choose an image")
            .add_filter(IMAGES.0, IMAGES.1)
            .pick_file()
    }

    fn pick_images(&self) -> Vec<PathBuf> {
        rfd::FileDialog::new().set_title("Add images").add_filter(IMAGES.0, IMAGES.1).pick_files().unwrap_or_default()
    }

    fn save_pdf(&self, title: &str, suggested: &Path) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title(title).add_filter(PDF.0, PDF.1);
        if let Some(dir) = suggested.parent() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = suggested.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
        dialog.save_file().map(with_pdf_extension)
    }

    fn confirm(&self, title: &str, message: &str) -> bool {
        rfd::MessageDialog::new()
            .set_title(title)
            .set_description(message)
            .set_level(rfd::MessageLevel::Warning)
            .set_buttons(rfd::MessageButtons::OkCancelCustom("Replace".into(), "Cancel".into()))
            .show()
            == rfd::MessageDialogResult::Custom("Replace".into())
    }
}

pub fn with_pdf_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".pdf");
        PathBuf::from(name)
    }
}

/// Queued answers for tests. An empty queue means "cancelled".
#[derive(Default)]
pub struct ScriptedDialogs {
    pub files: RefCell<VecDeque<Vec<PathBuf>>>,
    pub saves: RefCell<VecDeque<Option<PathBuf>>>,
    pub confirms: RefCell<VecDeque<bool>>,
    /// Titles of the save dialogs shown, for assertions.
    pub save_titles: RefCell<Vec<String>>,
}

impl ScriptedDialogs {
    pub fn open(&self, paths: &[&Path]) {
        self.files.borrow_mut().push_back(paths.iter().map(|p| p.to_path_buf()).collect());
    }

    pub fn save(&self, path: Option<&Path>) {
        self.saves.borrow_mut().push_back(path.map(Path::to_path_buf));
    }

    pub fn answer(&self, yes: bool) {
        self.confirms.borrow_mut().push_back(yes);
    }

    fn next_file(&self) -> Option<PathBuf> {
        self.files.borrow_mut().pop_front().and_then(|f| f.into_iter().next())
    }
}

impl Dialogs for ScriptedDialogs {
    fn pick_pdf(&self) -> Option<PathBuf> {
        self.next_file()
    }

    fn pick_pdfs(&self) -> Vec<PathBuf> {
        self.files.borrow_mut().pop_front().unwrap_or_default()
    }

    fn pick_folder(&self, _near: &Path) -> Option<PathBuf> {
        self.next_file()
    }

    fn pick_images(&self) -> Vec<PathBuf> {
        self.files.borrow_mut().pop_front().unwrap_or_default()
    }

    fn pick_image(&self) -> Option<PathBuf> {
        self.next_file()
    }

    fn save_pdf(&self, title: &str, _suggested: &Path) -> Option<PathBuf> {
        self.save_titles.borrow_mut().push(title.to_owned());
        self.saves.borrow_mut().pop_front().flatten().map(with_pdf_extension)
    }

    fn confirm(&self, _title: &str, _message: &str) -> bool {
        self.confirms.borrow_mut().pop_front().unwrap_or(false)
    }
}

/// Shows a file in the system file manager.
pub fn reveal(path: &Path) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
    let _ = result;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_pdf_extension() {
        assert_eq!(with_pdf_extension("a/b".into()), PathBuf::from("a/b.pdf"));
        assert_eq!(with_pdf_extension("a/b.PDF".into()), PathBuf::from("a/b.PDF"));
        assert_eq!(with_pdf_extension("a/b.txt".into()), PathBuf::from("a/b.txt.pdf"));
    }

    #[test]
    fn scripted_answers_come_in_order() {
        let d = ScriptedDialogs::default();
        d.open(&[Path::new("x.pdf"), Path::new("y.pdf")]);
        d.open(&[Path::new("z.pdf")]);
        assert_eq!(d.pick_pdfs().len(), 2);
        assert_eq!(d.pick_pdf(), Some(PathBuf::from("z.pdf")));
        assert_eq!(d.pick_pdf(), None);
        d.save(Some(Path::new("out")));
        assert_eq!(d.save_pdf("Save", Path::new("s.pdf")), Some(PathBuf::from("out.pdf")));
        assert_eq!(d.save_titles.borrow().as_slice(), ["Save"]);
        d.answer(true);
        assert!(d.confirm("t", "m"));
        assert!(!d.confirm("t", "m"));
    }
}
