use pdftools_core::Metadata;

use super::{choose_output, edit, Job, Outcome, Shared, SingleFile, Tool};
use crate::single_file_plumbing;

const KEYS: [&str; 6] = ["title", "author", "subject", "keywords", "creator", "producer"];

pub struct MetadataTool {
    base: SingleFile,
    original: [String; 6],
    values: [String; 6],
    version: i32,
}

impl MetadataTool {
    pub fn new() -> Self {
        Self { base: SingleFile::new(super::id::METADATA), original: Default::default(), values: Default::default(), version: 0 }
    }
}

impl Default for MetadataTool {
    fn default() -> Self {
        Self::new()
    }
}

fn fields(m: &Metadata) -> [String; 6] {
    [&m.title, &m.author, &m.subject, &m.keywords, &m.creator, &m.producer].map(|v| v.clone().unwrap_or_default())
}

fn format_date(iso: Option<&str>) -> String {
    match iso {
        None => "—".into(),
        // "2024-03-01T10:00:00Z" → "2024-03-01 10:00"
        Some(iso) if iso.len() >= 16 => format!("{} {}", &iso[..10], &iso[11..16]),
        Some(iso) => iso.into(),
    }
}

impl Tool for MetadataTool {
    single_file_plumbing!();

    fn file_changed(&mut self, _s: &mut Shared) {
        self.original = self.base.file.as_ref().map(|f| fields(&f.info.metadata)).unwrap_or_default();
        self.values = self.original.clone();
        self.version += 1;
    }

    fn option(&mut self, _s: &mut Shared, key: &str, value: &str) {
        if key == "clear" {
            self.values = Default::default();
            self.version += 1;
        } else if let Some(i) = KEYS.iter().position(|k| *k == key) {
            self.values[i] = value.into();
        }
    }

    /// The six fields, then created and modified dates.
    fn texts(&self) -> Vec<String> {
        let metadata = self.base.file.as_ref().map(|f| &f.info.metadata);
        let mut texts = self.values.to_vec();
        texts.push(format_date(metadata.and_then(|m| m.created.as_deref())));
        texts.push(format_date(metadata.and_then(|m| m.modified.as_deref())));
        texts
    }

    fn form_version(&self) -> i32 {
        self.version
    }

    fn run_label(&self) -> String {
        "Save metadata…".into()
    }

    fn validate(&self) -> Option<String> {
        let changed = self.values.iter().zip(&self.original).any(|(a, b)| a.trim() != b.trim());
        (!changed && self.base.excluded.is_empty()).then(|| "Edit a field to save new metadata.".into())
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let output = choose_output(&*s.dialogs, &input, "edited")?;
        let v = self.values.clone().map(|v| Some(v.trim().to_owned()).filter(|v| !v.is_empty()));
        let [title, author, subject, keywords, creator, producer] = v;
        let metadata = Metadata { title, author, subject, keywords, creator, producer, created: None, modified: None };
        let (keep, note) = (self.base.keep(), self.base.removed_note());
        Some(Box::new(move |_| {
            edit(&input, keep.as_deref(), &output, |doc| {
                pdftools_core::metadata::write(doc, &metadata)?;
                doc.prune_objects();
                Ok(())
            })?;
            Ok(Outcome { message: format!("Updated properties{note} saved to"), output: Some(output) })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dates() {
        assert_eq!(format_date(None), "—");
        assert_eq!(format_date(Some("2024-03-01T10:00:00Z")), "2024-03-01 10:00");
        assert_eq!(format_date(Some("2024")), "2024");
    }
}
