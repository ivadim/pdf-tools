use pdftools_core::PageSpec;

use super::{choose_output, normalize_rotation, plural, Job, Outcome, Shared, SingleFile, Tool};
use crate::{single_file_plumbing, GridCaps, PageTile};

pub struct RotateTool {
    base: SingleFile,
    /// Extra clockwise rotation per page (index = page − 1).
    rotations: Vec<i32>,
}

impl RotateTool {
    pub fn new() -> Self {
        Self { base: SingleFile::new(super::id::ROTATE), rotations: Vec::new() }
    }

    fn rotate(&mut self, pages: impl IntoIterator<Item = usize>, delta: i32) {
        for i in pages {
            if let Some(r) = self.rotations.get_mut(i) {
                *r = normalize_rotation(*r + delta);
            }
        }
    }
}

impl Default for RotateTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for RotateTool {
    single_file_plumbing!();

    fn file_changed(&mut self, _s: &mut Shared) {
        self.rotations = vec![0; self.base.page_count()];
    }

    fn caps(&self) -> GridCaps {
        GridCaps { rotate: true, include: true, ..Default::default() }
    }

    fn tiles(&self) -> Vec<PageTile> {
        (1..=self.base.page_count()).map(|p| self.base.tile(p, self.rotations[p - 1])).collect()
    }

    fn tile_action(&mut self, _s: &mut Shared, action: &str, index: usize) {
        match action {
            "rotate-left" => self.rotate([index], -90),
            "rotate-right" => self.rotate([index], 90),
            _ => {}
        }
    }

    fn grid_action(&mut self, _s: &mut Shared, action: &str) {
        let all = 0..self.rotations.len();
        match action {
            "rotate-all-left" => self.rotate(all, -90),
            "rotate-all-right" => self.rotate(all, 90),
            "reset" => self.rotations.iter_mut().for_each(|r| *r = 0),
            _ => {}
        }
    }

    fn run_label(&self) -> String {
        "Save rotated PDF…".into()
    }

    fn validate(&self) -> Option<String> {
        let rotated = self.rotations.iter().enumerate().any(|(i, r)| *r != 0 && !self.base.excluded.contains(&(i + 1)));
        (!rotated && self.base.excluded.is_empty())
            .then(|| "Rotate at least one page — use the buttons on each page or above.".into())
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let output = choose_output(&*s.dialogs, &input, "rotated")?;
        let pages: Vec<PageSpec> =
            self.base.kept_pages().into_iter().map(|p| PageSpec::new(0, p).rotated(self.rotations[p - 1] as i64)).collect();
        let note = self.base.removed_note();
        Some(Box::new(move |_| {
            pdftools_core::organize::organize_file(&input, &pages, &output).map_err(|e| e.to_string())?;
            let changed = pages.iter().filter(|p| p.rotation != 0).count();
            let what = if changed > 0 { format!("Rotated {}", plural(changed, "page")) } else { format!("Saved {}", plural(pages.len(), "page")) };
            Ok(Outcome { message: format!("{what}{note} into"), output: Some(output) })
        }))
    }
}
