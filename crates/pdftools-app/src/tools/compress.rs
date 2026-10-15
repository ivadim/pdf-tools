use pdftools_core::compress::{compress_document, Level};

use super::{choose_output, edit, format_bytes, Job, Outcome, Shared, SingleFile, Tool};
use crate::single_file_plumbing;

pub struct CompressTool {
    base: SingleFile,
    level: Level,
}

impl CompressTool {
    pub fn new() -> Self {
        Self { base: SingleFile::new(super::id::COMPRESS), level: Level::Balanced }
    }
}

impl Default for CompressTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for CompressTool {
    single_file_plumbing!();

    fn option(&mut self, _s: &mut Shared, key: &str, value: &str) {
        if key == "level" {
            self.level = match value {
                "lossless" => Level::Lossless,
                "strong" => Level::Strong,
                _ => Level::Balanced,
            };
        }
    }

    fn run_label(&self) -> String {
        "Compress PDF…".into()
    }

    fn validate(&self) -> Option<String> {
        None
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let output = choose_output(&*s.dialogs, &input, "compressed")?;
        let (keep, level, note) = (self.base.keep(), self.level, self.base.removed_note());
        Some(Box::new(move |_| {
            let before = pdftools_core::file_size(&input);
            let images = edit(&input, keep.as_deref(), &output, |doc| Ok(compress_document(doc, level)))?;
            let after = pdftools_core::file_size(&output);
            let saved = if before > 0 { (100.0 * (1.0 - after as f64 / before as f64)).round() as i64 } else { 0 };
            let change = if saved > 0 {
                format!("{} → {} ({saved}% smaller)", format_bytes(before), format_bytes(after))
            } else {
                format!("Already compact: {} → {}", format_bytes(before), format_bytes(after))
            };
            let detail = match images {
                0 => String::new(),
                1 => ", 1 image re-encoded".into(),
                n => format!(", {n} images re-encoded"),
            };
            Ok(Outcome { message: format!("{change}{detail}{note}. Saved as"), output: Some(output) })
        }))
    }
}
