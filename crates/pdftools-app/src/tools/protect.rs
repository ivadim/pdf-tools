use pdftools_core::security::{protect_document, Protection};

use super::{choose_output, edit, Job, Outcome, Shared, SingleFile, Tool};
use crate::single_file_plumbing;

pub struct ProtectTool {
    base: SingleFile,
    password: String,
    confirm: String,
    owner: String,
    allow: [bool; 4],
}

impl ProtectTool {
    pub fn new() -> Self {
        Self {
            base: SingleFile::new(super::id::PROTECT),
            password: String::new(),
            confirm: String::new(),
            owner: String::new(),
            allow: [true; 4],
        }
    }
}

impl Default for ProtectTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ProtectTool {
    single_file_plumbing!();

    fn option(&mut self, _s: &mut Shared, key: &str, value: &str) {
        let on = value == "1";
        match key {
            "password" => self.password = value.into(),
            "confirm" => self.confirm = value.into(),
            "owner" => self.owner = value.into(),
            "print" => self.allow[0] = on,
            "copy" => self.allow[1] = on,
            "modify" => self.allow[2] = on,
            "annotate" => self.allow[3] = on,
            _ => {}
        }
    }

    fn run_label(&self) -> String {
        "Protect PDF…".into()
    }

    fn validate(&self) -> Option<String> {
        if self.password.is_empty() {
            Some("Enter a password.".into())
        } else if self.password != self.confirm {
            Some("The passwords don’t match.".into())
        } else {
            None
        }
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let output = choose_output(&*s.dialogs, &input, "protected")?;
        let protection = Protection {
            user_password: self.password.clone(),
            owner_password: (!self.owner.is_empty()).then(|| self.owner.clone()),
            allow_print: self.allow[0],
            allow_copy: self.allow[1],
            allow_modify: self.allow[2],
            allow_annotate: self.allow[3],
        };
        let (keep, note) = (self.base.keep(), self.base.removed_note());
        Some(Box::new(move |_| {
            edit(&input, keep.as_deref(), &output, |doc| protect_document(doc, &protection))?;
            Ok(Outcome { message: format!("Password-protected copy{note} saved as"), output: Some(output) })
        }))
    }
}
