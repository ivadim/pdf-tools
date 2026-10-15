use super::{choose_output, Job, Outcome, Shared, SingleFile, Tool};
use crate::single_file_plumbing;
use crate::GridCaps;

pub struct UnlockTool {
    base: SingleFile,
    password: String,
}

impl UnlockTool {
    pub fn new() -> Self {
        let mut base = SingleFile::new(super::id::UNLOCK);
        base.allow_locked = true;
        Self { base, password: String::new() }
    }
}

impl Default for UnlockTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for UnlockTool {
    single_file_plumbing!();

    fn file_changed(&mut self, _s: &mut Shared) {
        self.password.clear();
    }

    fn option(&mut self, _s: &mut Shared, key: &str, value: &str) {
        if key == "password" {
            self.password = value.into();
        }
    }

    fn caps(&self) -> GridCaps {
        GridCaps::default()
    }

    /// [0] a file is loaded, [1] it needs a password, [2] it's encrypted.
    fn flags(&self) -> Vec<bool> {
        let info = self.base.file.as_ref().map(|f| &f.info);
        vec![info.is_some(), info.is_some_and(|i| i.needs_password), info.is_some_and(|i| i.encrypted)]
    }

    fn empty_text(&self) -> String {
        match &self.base.file {
            Some(f) if f.info.needs_password => "Pages can be previewed once the file is unlocked.".into(),
            Some(_) => "This file has no pages to preview.".into(),
            None => "Pages will appear here once you choose a file.".into(),
        }
    }

    fn run_label(&self) -> String {
        "Unlock PDF…".into()
    }

    fn validate(&self) -> Option<String> {
        let info = &self.base.file.as_ref()?.info;
        if !info.encrypted {
            Some("This file isn’t protected.".into())
        } else if info.needs_password && self.password.is_empty() {
            Some("Enter the file’s password.".into())
        } else {
            None
        }
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let output = choose_output(&*s.dialogs, &input, "unlocked")?;
        let password = self.password.clone();
        Some(Box::new(move |_| {
            pdftools_core::security::unlock_file(&input, &password, &output).map_err(|e| e.to_string())?;
            Ok(Outcome { message: "Unlocked copy saved as".into(), output: Some(output) })
        }))
    }
}
