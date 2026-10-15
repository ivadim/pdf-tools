use pdftools_core::PageSpec;

use super::{choose_output, plural, Job, Outcome, Shared, SingleFile, Tool};
use crate::{single_file_plumbing, GridCaps, PageTile};

pub struct RearrangeTool {
    base: SingleFile,
    /// Source page numbers in output order.
    order: Vec<usize>,
}

impl RearrangeTool {
    pub fn new() -> Self {
        Self { base: SingleFile::new(super::id::REARRANGE), order: Vec::new() }
    }

    fn unchanged(&self) -> bool {
        self.order.len() == self.base.page_count() && self.order.iter().enumerate().all(|(i, p)| *p == i + 1)
    }
}

impl Default for RearrangeTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Moves `from` so it lands before the item currently at `to`.
pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= items.len() || to == from || to == from + 1 || to > items.len() {
        return false;
    }
    let item = items.remove(from);
    items.insert(if to > from { to - 1 } else { to }, item);
    true
}

impl Tool for RearrangeTool {
    single_file_plumbing!();

    fn file_changed(&mut self, _s: &mut Shared) {
        self.order = (1..=self.base.page_count()).collect();
    }

    fn caps(&self) -> GridCaps {
        GridCaps { reorder: true, delete: true, duplicate: true, ..Default::default() }
    }

    fn tiles(&self) -> Vec<PageTile> {
        let total = self.order.len();
        self.order
            .iter()
            .enumerate()
            .map(|(i, &page)| PageTile {
                label: if page == i + 1 { format!("p. {page}") } else { format!("was p. {page}") }.into(),
                caption: format!("Page {} of {total} (original page {page})", i + 1).into(),
                ..self.base.tile(page, 0)
            })
            .collect()
    }

    fn tile_action(&mut self, _s: &mut Shared, action: &str, index: usize) {
        if index >= self.order.len() {
            return;
        }
        match action {
            "delete" => {
                self.order.remove(index);
            }
            "duplicate" => self.order.insert(index + 1, self.order[index]),
            _ => {}
        }
    }

    fn tile_moved(&mut self, _s: &mut Shared, from: usize, to: usize) {
        move_item(&mut self.order, from, to);
    }

    fn grid_action(&mut self, _s: &mut Shared, action: &str) {
        match action {
            "reverse" => self.order.reverse(),
            "reset" => self.order = (1..=self.base.page_count()).collect(),
            _ => {}
        }
    }

    fn empty_text(&self) -> String {
        if self.base.file.is_some() {
            "All pages were deleted. Use “Reset” to start over.".into()
        } else {
            "Pages will appear here once you choose a file.".into()
        }
    }

    fn run_label(&self) -> String {
        "Save rearranged PDF…".into()
    }

    fn validate(&self) -> Option<String> {
        if self.order.is_empty() {
            Some("Keep at least one page.".into())
        } else if self.unchanged() {
            Some("Drag pages to reorder them, or use the buttons on each page to duplicate or delete.".into())
        } else {
            None
        }
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let output = choose_output(&*s.dialogs, &input, "rearranged")?;
        let pages: Vec<PageSpec> = self.order.iter().map(|&p| PageSpec::new(0, p)).collect();
        Some(Box::new(move |_| {
            pdftools_core::organize::organize_file(&input, &pages, &output).map_err(|e| e.to_string())?;
            Ok(Outcome { message: format!("Saved {} to", plural(pages.len(), "page")), output: Some(output) })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_items_before_a_target() {
        let mut v = vec![1, 2, 3, 4];
        assert!(move_item(&mut v, 3, 0));
        assert_eq!(v, [4, 1, 2, 3]);
        assert!(move_item(&mut v, 0, 4));
        assert_eq!(v, [1, 2, 3, 4]);
        assert!(!move_item(&mut v, 1, 2), "dropping onto itself is a no-op");
        assert!(!move_item(&mut v, 9, 0));
    }
}
