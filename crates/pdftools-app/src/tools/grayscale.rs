use pdftools_core::grayscale::grayscale_document;

use super::{choose_output, edit, Job, Outcome, Shared, SingleFile, Tool};
use crate::single_file_plumbing;

pub struct GrayscaleTool {
    base: SingleFile,
    images: bool,
}

impl GrayscaleTool {
    pub fn new() -> Self {
        Self { base: SingleFile::new(super::id::GRAYSCALE), images: true }
    }
}

impl Default for GrayscaleTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for GrayscaleTool {
    single_file_plumbing!();

    fn option(&mut self, _s: &mut Shared, key: &str, value: &str) {
        if key == "images" {
            self.images = value == "1";
        }
    }

    fn gray_previews(&self) -> bool {
        true
    }

    fn run_label(&self) -> String {
        "Convert to grayscale…".into()
    }

    fn validate(&self) -> Option<String> {
        None
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let output = choose_output(&*s.dialogs, &input, "grayscale")?;
        let (keep, images, note) = (self.base.keep(), self.images, self.base.removed_note());
        Some(Box::new(move |_| {
            edit(&input, keep.as_deref(), &output, |doc| Ok(grayscale_document(doc, images)))?;
            Ok(Outcome { message: format!("Grayscale copy{note} saved as"), output: Some(output) })
        }))
    }
}
