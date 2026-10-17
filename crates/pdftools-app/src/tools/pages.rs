//! Split, Rearrange and Metadata.

use std::collections::BTreeSet;

use iced::keyboard::Modifiers;
use iced::widget::{column, row, text, text_input};
use iced::{color, Color, Element};
use pdftools_core::organize::{split_file, SplitPart};
use pdftools_core::PageSpec;
use pdftools_render::ranges::{format_list, parse_groups};

use super::set;
use crate::app::Message;
use crate::catalog::ToolId;
use crate::dialogs::Dialogs;
use crate::icons;
use crate::previews::Thumb;
use crate::single::{choose_output, edit, file_name, plural, Job, Single, TileSpec, ToolOptions};
use crate::theme;
use crate::widgets::{card_options, field, muted, section, Caps, TileEvent};

fn input<'a>(placeholder: &'a str, value: &'a str, on_input: impl Fn(String) -> Message + 'a) -> Element<'a, Message> {
    text_input(placeholder, value).on_input(on_input).padding(8).style(theme::input).into()
}

// ── Split ────────────────────────────────────────────────────────────────

const PART_COLORS: [Color; 8] = [
    color!(0x3b82f6),
    color!(0xf97316),
    color!(0x22c55e),
    color!(0xa855f7),
    color!(0xec4899),
    color!(0x14b8a6),
    color!(0xeab308),
    color!(0xef4444),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Ranges,
    Every,
    Each,
    Extract,
}

pub struct Split {
    mode: Mode,
    ranges: String,
    every: String,
    base_name: String,
    selected: BTreeSet<usize>,
    last_clicked: Option<usize>,
    total: usize,
}

impl Default for Split {
    fn default() -> Self {
        Self {
            mode: Mode::Ranges,
            ranges: String::new(),
            every: "1".into(),
            base_name: String::new(),
            selected: BTreeSet::new(),
            last_clicked: None,
            total: 0,
        }
    }
}

impl Split {
    /// Output files as lists of pages.
    fn parts(&self) -> Result<Vec<Vec<usize>>, String> {
        let total = self.total;
        match self.mode {
            Mode::Ranges => parse_groups(&self.ranges, total),
            Mode::Every => {
                let every = self.every.trim().parse::<usize>().map_err(|_| "Enter how many pages go in each file.".to_owned())?;
                Ok((1..=total).collect::<Vec<_>>().chunks(every.max(1)).map(<[usize]>::to_vec).collect())
            }
            Mode::Each => Ok((1..=total).map(|p| vec![p]).collect()),
            Mode::Extract if self.selected.is_empty() => Err("Select the pages to extract.".into()),
            Mode::Extract => Ok(vec![self.selected.iter().copied().collect()]),
        }
    }
}

impl Split {
    /// "2 files: 7 + 7 pages", or what's selected.
    fn summary(&self) -> Option<String> {
        let parts = self.parts().ok().filter(|p| !p.is_empty())?;
        let sizes: Vec<usize> = parts.iter().map(Vec::len).collect();
        Some(if sizes.iter().all(|s| *s == sizes[0]) && sizes.len() > 3 {
            format!("{} files of {} each", sizes.len(), plural(sizes[0], "page"))
        } else {
            let list: Vec<String> = sizes.iter().map(usize::to_string).collect();
            format!("{}: {} pages", plural(sizes.len(), "file"), list.join(" + "))
        })
    }
}

impl ToolOptions for Split {
    fn id(&self) -> ToolId {
        ToolId::Split
    }

    fn view<'a>(&'a self, _file: &'a Single, _closeup: Option<&'a Thumb>) -> Element<'a, Message> {
        let t = ToolId::Split;
        let selected = match self.mode {
            Mode::Ranges => "ranges",
            Mode::Every => "every",
            Mode::Each => "each",
            Mode::Extract => "extract",
        };
        let mut content = column![section(
            "Split method",
            [card_options(
                &[
                    ("ranges", "By ranges", "Each range becomes a file"),
                    ("every", "Every N pages", "Equal-size chunks"),
                    ("each", "Each page", "One file per page"),
                    ("extract", "Extract pages", "Selected pages into one file"),
                ],
                selected,
                move |v| Message::Option(t, "mode", v.into()),
            )]
        )]
        .spacing(18);
        match self.mode {
            Mode::Ranges => {
                content = content.push(
                    column![
                        field("Page ranges", input("e.g. 1-3, 4-8, 9-", &self.ranges, set(t, "ranges"))),
                        muted("Separate files with commas. “9-” runs to the last page."),
                    ]
                    .spacing(5),
                );
            }
            Mode::Every => content = content.push(field("Pages per file", input("e.g. 5", &self.every, set(t, "every")))),
            Mode::Extract => {
                content =
                    content.push(muted("Click pages in the preview to select them. Shift-click selects a range, Ctrl-click toggles."));
            }
            Mode::Each => {}
        }
        if let (Some(summary), true) = (self.summary(), self.mode != Mode::Extract) {
            content = content.push(text(summary).size(13).color(theme::c().accent));
        }
        if self.mode != Mode::Extract {
            content = content.push(
                column![
                    field("File names", input("name", &self.base_name, set(t, "base"))),
                    muted("Files are numbered: name-1.pdf, name-2.pdf, …"),
                ]
                .spacing(5),
            );
        }
        content.into()
    }

    fn set(&mut self, key: &str, value: String, _file: &Single, _dialogs: &dyn Dialogs) {
        match key {
            "mode" => {
                self.mode = match value.as_str() {
                    "every" => Mode::Every,
                    "each" => Mode::Each,
                    "extract" => Mode::Extract,
                    _ => Mode::Ranges,
                }
            }
            "ranges" => self.ranges = value,
            "every" => self.every = value,
            "base" => self.base_name = value,
            _ => {}
        }
    }

    fn caps(&self) -> Caps {
        Caps::default()
    }

    fn tiles(&self, file: &Single) -> Vec<TileSpec> {
        let parts = self.parts().unwrap_or_default();
        (1..=file.page_count())
            .map(|page| {
                let tile = TileSpec { page, ..Default::default() };
                if self.mode == Mode::Extract {
                    let selected = self.selected.contains(&page);
                    return TileSpec { selected, dimmed: !self.selected.is_empty() && !selected, ..tile };
                }
                match parts.iter().position(|p| p.contains(&page)) {
                    Some(part) => {
                        TileSpec { badge: Some(format!("File {}", part + 1)), stripe: Some(PART_COLORS[part % PART_COLORS.len()]), ..tile }
                    }
                    None => TileSpec { dimmed: true, label: "not included".into(), ..tile },
                }
            })
            .collect()
    }

    fn clicked(&mut self, _index: usize, page: usize, modifiers: Modifiers) -> bool {
        if self.mode != Mode::Extract {
            return false;
        }
        match (modifiers.shift(), self.last_clicked) {
            (true, Some(last)) => self.selected.extend(last.min(page)..=last.max(page)),
            _ => {
                if !self.selected.remove(&page) {
                    self.selected.insert(page);
                }
            }
        }
        self.last_clicked = Some(page);
        true
    }

    fn grid_actions(&self) -> Vec<(&'static str, fn() -> iced::widget::svg::Handle, &'static str)> {
        if self.mode == Mode::Extract {
            vec![("Select all pages", icons::select_all, "select-all"), ("Clear selection", icons::x, "clear-selection")]
        } else {
            Vec::new()
        }
    }

    fn grid_action(&mut self, action: &str, file: &Single) {
        match action {
            "select-all" => self.selected.extend(1..=file.page_count()),
            "clear-selection" => self.selected.clear(),
            _ => {}
        }
    }

    fn file_changed(&mut self, file: &Single) {
        self.selected.clear();
        self.last_clicked = None;
        self.total = file.page_count();
        self.base_name = file.path().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        self.ranges = match self.total {
            0 => String::new(),
            1 => "1".into(),
            n => format!("1-{}, {}-{n}", n.div_ceil(2), n.div_ceil(2) + 1),
        };
    }

    fn validate(&self, _file: &Single) -> Option<String> {
        let parts = match self.parts() {
            Ok(parts) => parts,
            Err(e) => return Some(e),
        };
        if self.mode != Mode::Extract && parts.len() < 2 {
            return Some(if self.mode == Mode::Ranges {
                "Enter at least two ranges to split into separate files.".into()
            } else {
                "That would produce a single file.".into()
            });
        }
        (self.mode != Mode::Extract && self.base_name.trim().is_empty()).then(|| "Enter a file name.".into())
    }

    fn run_label(&self) -> &'static str {
        if self.mode == Mode::Extract {
            "Extract pages…"
        } else {
            "Split PDF…"
        }
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let input = file.path()?.to_path_buf();
        let parts = self.parts().ok()?;
        let specs = |pages: &[usize]| pages.iter().map(|&p| PageSpec::new(0, p)).collect::<Vec<_>>();
        if self.mode == Mode::Extract {
            let suffix = format!("pages-{}", format_list(&parts[0]).replace(", ", "_"));
            let output = choose_output(dialogs, file, &suffix)?;
            let part = SplitPart { pages: specs(&parts[0]), output: output.clone() };
            let count = parts[0].len();
            return Some(Box::new(move || {
                split_file(&input, &[part], |_| {}).map_err(|e| e.to_string())?;
                Ok((format!("Extracted {} to", plural(count, "page")), Some(output)))
            }));
        }
        let folder = dialogs.pick_folder(&input)?;
        let base = self.base_name.trim().to_owned();
        let split: Vec<SplitPart> = parts
            .iter()
            .enumerate()
            .map(|(i, pages)| SplitPart { pages: specs(pages), output: folder.join(format!("{base}-{}.pdf", i + 1)) })
            .collect();
        let existing: Vec<String> = split.iter().filter(|p| p.output.exists()).map(|p| file_name(&p.output)).collect();
        if !existing.is_empty() {
            let shown: Vec<&str> = existing.iter().take(5).map(String::as_str).collect();
            let more = if existing.len() > 5 { format!("\n…and {} more", existing.len() - 5) } else { String::new() };
            let message = format!("{} already exist and will be replaced:\n\n{}{more}", plural(existing.len(), "file"), shown.join("\n"));
            if !dialogs.confirm("Replace files?", &message) {
                return None;
            }
        }
        let first = split[0].output.clone();
        Some(Box::new(move || {
            let count = split_file(&input, &split, |_| {}).map_err(|e| e.to_string())?;
            Ok((format!("Created {}, starting with", plural(count, "file")), Some(first)))
        }))
    }
}

// ── Rearrange ────────────────────────────────────────────────────────────

/// Moves `from` so it lands before the item currently at `to`.
pub fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= items.len() || to == from || to == from + 1 || to > items.len() {
        return false;
    }
    let item = items.remove(from);
    items.insert(if to > from { to - 1 } else { to }, item);
    true
}

#[derive(Default)]
pub struct Rearrange {
    /// Source page numbers in output order.
    order: Vec<usize>,
    total: usize,
}

impl Rearrange {
    fn unchanged(&self) -> bool {
        self.order.len() == self.total && self.order.iter().enumerate().all(|(i, p)| *p == i + 1)
    }
}

impl ToolOptions for Rearrange {
    fn id(&self) -> ToolId {
        ToolId::Rearrange
    }

    fn view<'a>(&'a self, _file: &'a Single, _closeup: Option<&'a Thumb>) -> Element<'a, Message> {
        muted("Move pages with the arrows on each page, or duplicate and delete them. Reverse or reset the order with the buttons above the pages.")
    }

    fn set(&mut self, _key: &str, _value: String, _file: &Single, _dialogs: &dyn Dialogs) {}

    fn caps(&self) -> Caps {
        Caps { reorder: true, delete: true, duplicate: true, ..Default::default() }
    }

    fn tiles(&self, _file: &Single) -> Vec<TileSpec> {
        self.order
            .iter()
            .enumerate()
            .map(|(i, &page)| TileSpec {
                page,
                label: if page == i + 1 { String::new() } else { format!("was p. {page}") },
                ..Default::default()
            })
            .collect()
    }

    fn tile_event(&mut self, index: usize, _page: usize, event: TileEvent) {
        if index >= self.order.len() {
            return;
        }
        match event {
            TileEvent::Delete => {
                self.order.remove(index);
            }
            TileEvent::Duplicate => self.order.insert(index + 1, self.order[index]),
            TileEvent::MoveEarlier if index > 0 => self.order.swap(index, index - 1),
            TileEvent::MoveLater if index + 1 < self.order.len() => self.order.swap(index, index + 1),
            _ => {}
        }
    }

    fn grid_actions(&self) -> Vec<(&'static str, fn() -> iced::widget::svg::Handle, &'static str)> {
        vec![("Reverse order", icons::reverse, "reverse"), ("Reset to original order", icons::reset, "reset")]
    }

    fn grid_action(&mut self, action: &str, file: &Single) {
        match action {
            "reverse" => self.order.reverse(),
            "reset" => self.order = (1..=file.page_count()).collect(),
            _ => {}
        }
    }

    fn file_changed(&mut self, file: &Single) {
        self.total = file.page_count();
        self.order = (1..=self.total).collect();
    }

    fn empty_text(&self, file: &Single) -> Option<&'static str> {
        file.loaded().map(|_| "All pages were deleted. Use “Reset” to start over.")
    }

    fn validate(&self, _file: &Single) -> Option<String> {
        if self.order.is_empty() {
            Some("Keep at least one page.".into())
        } else if self.unchanged() {
            Some("Move, duplicate or delete pages with the buttons on each page.".into())
        } else {
            None
        }
    }

    fn run_label(&self) -> &'static str {
        "Save rearranged PDF…"
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let input = file.path()?.to_path_buf();
        let output = choose_output(dialogs, file, "rearranged")?;
        let pages: Vec<PageSpec> = self.order.iter().map(|&p| PageSpec::new(0, p)).collect();
        Some(Box::new(move || {
            pdftools_core::organize::organize_file(&input, &pages, &output).map_err(|e| e.to_string())?;
            Ok((format!("Saved {} to", plural(pages.len(), "page")), Some(output)))
        }))
    }
}

// ── Metadata ─────────────────────────────────────────────────────────────

const FIELDS: [(&str, &str, &str); 6] = [
    ("title", "Title", "Document title"),
    ("author", "Author", "Who wrote it"),
    ("subject", "Subject", "What it’s about"),
    ("keywords", "Keywords", "Comma-separated"),
    ("creator", "Created with", "Application that made the original"),
    ("producer", "Producer", "Application that wrote the PDF"),
];

#[derive(Default)]
pub struct Metadata {
    original: [String; 6],
    values: [String; 6],
    dates: [String; 2],
}

fn format_date(iso: Option<&str>) -> String {
    match iso {
        None => "—".into(),
        // "2024-03-01T10:00:00Z" → "2024-03-01 10:00"
        Some(iso) if iso.len() >= 16 => format!("{} {}", &iso[..10], &iso[11..16]),
        Some(iso) => iso.into(),
    }
}

impl ToolOptions for Metadata {
    fn id(&self) -> ToolId {
        ToolId::Metadata
    }

    fn view<'a>(&'a self, file: &'a Single, _closeup: Option<&'a Thumb>) -> Element<'a, Message> {
        if file.loaded().is_none() {
            return column![].into();
        }
        let fields = FIELDS
            .iter()
            .zip(&self.values)
            .map(|((key, label, placeholder), value)| field(label, input(placeholder, value, set(ToolId::Metadata, key))));
        let clear = iced::widget::button(text("Clear all").size(13)).padding(0).style(theme::link).on_press(Message::Option(
            ToolId::Metadata,
            "clear",
            String::new(),
        ));
        column![
            row![crate::widgets::section_title("Document properties"), iced::widget::space::horizontal(), clear]
                .align_y(iced::Alignment::Center),
            muted(format!("Created {} · Modified {}", self.dates[0], self.dates[1])),
            column(fields).spacing(10),
        ]
        .spacing(10)
        .into()
    }

    fn set(&mut self, key: &str, value: String, _file: &Single, _dialogs: &dyn Dialogs) {
        if key == "clear" {
            self.values = Default::default();
        } else if let Some(i) = FIELDS.iter().position(|(k, _, _)| *k == key) {
            self.values[i] = value;
        }
    }

    fn file_changed(&mut self, file: &Single) {
        let metadata = file.loaded().map(|f| f.info.metadata.clone()).unwrap_or_default();
        self.original = [&metadata.title, &metadata.author, &metadata.subject, &metadata.keywords, &metadata.creator, &metadata.producer]
            .map(|v| v.clone().unwrap_or_default());
        self.values = self.original.clone();
        self.dates = [format_date(metadata.created.as_deref()), format_date(metadata.modified.as_deref())];
    }

    fn validate(&self, file: &Single) -> Option<String> {
        let changed = self.values.iter().zip(&self.original).any(|(a, b)| a.trim() != b.trim());
        (!changed && file.excluded.is_empty()).then(|| "Edit a field to save new metadata.".into())
    }

    fn run_label(&self) -> &'static str {
        "Save metadata…"
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let input = file.path()?.to_path_buf();
        let output = choose_output(dialogs, file, "edited")?;
        let v = self.values.clone().map(|v| Some(v.trim().to_owned()).filter(|v| !v.is_empty()));
        let [title, author, subject, keywords, creator, producer] = v;
        let metadata = pdftools_core::Metadata { title, author, subject, keywords, creator, producer, created: None, modified: None };
        let (keep, note) = (file.keep(), file.removed_note());
        Some(Box::new(move || {
            edit(&input, keep.as_deref(), &output, |doc| {
                pdftools_core::metadata::write(doc, &metadata)?;
                doc.prune_objects();
                Ok(())
            })?;
            Ok((format!("Updated properties{note} saved to"), Some(output)))
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_items_before_a_target() {
        let mut items = vec![1, 2, 3, 4];
        assert!(move_item(&mut items, 0, 3));
        assert_eq!(items, [2, 3, 1, 4]);
        assert!(!move_item(&mut items, 1, 2), "dropping onto itself does nothing");
    }

    #[test]
    fn split_ranges_and_chunks() {
        let mut split = Split { total: 5, ranges: "1-2, 4-".into(), ..Default::default() };
        assert_eq!(split.parts().unwrap(), vec![vec![1, 2], vec![4, 5]]);
        split.mode = Mode::Every;
        split.every = "2".into();
        assert_eq!(split.parts().unwrap().len(), 3);
        split.every = "x".into();
        assert!(split.parts().is_err());
        assert_eq!(format_date(Some("2024-03-01T10:00:00Z")), "2024-03-01 10:00");
    }
}
