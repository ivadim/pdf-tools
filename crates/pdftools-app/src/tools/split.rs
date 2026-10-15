use std::collections::BTreeSet;

use pdftools_core::organize::{split_file, SplitPart};
use pdftools_core::PageSpec;

use super::{plural, stem, Job, Outcome, Shared, SingleFile, Tool};
use crate::ranges::{format_list, parse_groups};
use crate::{single_file_plumbing, GridCaps, PageTile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Ranges,
    Every,
    Each,
    Extract,
}

/// Colours of consecutive output files in the preview.
const PART_COLORS: [[u8; 3]; 8] = [
    [0x3b, 0x82, 0xf6], [0xf9, 0x73, 0x16], [0x22, 0xc5, 0x5e], [0xa8, 0x55, 0xf7],
    [0xec, 0x48, 0x99], [0x14, 0xb8, 0xa6], [0xea, 0xb3, 0x08], [0xef, 0x44, 0x44],
];

pub struct SplitTool {
    base: SingleFile,
    mode: Mode,
    ranges: String,
    every: usize,
    base_name: String,
    selected: BTreeSet<usize>,
    last_clicked: Option<usize>,
    version: i32,
}

impl SplitTool {
    pub fn new() -> Self {
        Self {
            base: SingleFile::new(super::id::SPLIT),
            mode: Mode::Ranges,
            ranges: String::new(),
            every: 1,
            base_name: String::new(),
            selected: BTreeSet::new(),
            last_clicked: None,
            version: 0,
        }
    }

    /// Output files as lists of pages.
    pub fn parts(&self) -> Result<Vec<Vec<usize>>, String> {
        let total = self.base.page_count();
        match self.mode {
            Mode::Ranges => parse_groups(&self.ranges, total),
            Mode::Every => Ok((1..=total).collect::<Vec<_>>().chunks(self.every.max(1)).map(<[usize]>::to_vec).collect()),
            Mode::Each => Ok((1..=total).map(|p| vec![p]).collect()),
            Mode::Extract if self.selected.is_empty() => Err("Select the pages to extract.".into()),
            Mode::Extract => Ok(vec![self.selected.iter().copied().collect()]),
        }
    }
}

impl Default for SplitTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for SplitTool {
    single_file_plumbing!();

    fn file_changed(&mut self, _s: &mut Shared) {
        self.selected.clear();
        self.last_clicked = None;
        let total = self.base.page_count();
        self.base_name = self.base.path().map(stem).unwrap_or_default();
        self.ranges = match total {
            0 => String::new(),
            1 => "1".into(),
            n => format!("1-{}, {}-{n}", n.div_ceil(2), n.div_ceil(2) + 1),
        };
        self.every = self.every.clamp(1, total.max(1));
        self.version += 1;
    }

    fn option(&mut self, _s: &mut Shared, key: &str, value: &str) {
        match key {
            "mode" => {
                self.mode = match value {
                    "every" => Mode::Every,
                    "each" => Mode::Each,
                    "extract" => Mode::Extract,
                    _ => Mode::Ranges,
                }
            }
            "ranges" => self.ranges = value.into(),
            "every" => self.every = value.parse::<usize>().unwrap_or(1).max(1),
            "base" => self.base_name = value.trim().into(),
            _ => {}
        }
    }

    fn texts(&self) -> Vec<String> {
        vec![self.ranges.clone(), self.base_name.clone()]
    }

    fn form_version(&self) -> i32 {
        self.version
    }

    fn caps(&self) -> GridCaps {
        GridCaps { select: self.mode == Mode::Extract, ..Default::default() }
    }

    fn tiles(&self) -> Vec<PageTile> {
        let parts = self.parts().unwrap_or_default();
        let part_of = |page: usize| parts.iter().position(|p| p.contains(&page));
        (1..=self.base.page_count())
            .map(|page| {
                let tile = PageTile { excluded: false, ..self.base.tile(page, 0) };
                if self.mode == Mode::Extract {
                    let selected = self.selected.contains(&page);
                    return PageTile { selected, dimmed: !self.selected.is_empty() && !selected, ..tile };
                }
                match part_of(page) {
                    Some(part) => PageTile {
                        stripe: super::color_of(PART_COLORS[part % PART_COLORS.len()]),
                        has_stripe: true,
                        badge: format!("File {}", part + 1).into(),
                        ..tile
                    },
                    None => PageTile { dimmed: true, label: "not included".into(), ..tile },
                }
            })
            .collect()
    }

    fn tile_clicked(&mut self, _s: &mut Shared, index: usize, shift: bool, _ctrl: bool) {
        if self.mode != Mode::Extract {
            return;
        }
        let page = index + 1;
        match (shift, self.last_clicked) {
            (true, Some(last)) => self.selected.extend(last.min(page)..=last.max(page)),
            _ => {
                if !self.selected.remove(&page) {
                    self.selected.insert(page);
                }
            }
        }
        self.last_clicked = Some(page);
    }

    fn grid_action(&mut self, _s: &mut Shared, action: &str) {
        match action {
            "select-all" => self.selected.extend(1..=self.base.page_count()),
            "clear-selection" => self.selected.clear(),
            _ => {}
        }
    }

    fn run_label(&self) -> String {
        "Split PDF…".into()
    }

    fn validate(&self) -> Option<String> {
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
        (self.mode != Mode::Extract && self.base_name.is_empty()).then(|| "Enter a file name.".into())
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let parts = self.parts().ok()?;
        let specs = |pages: &[usize]| pages.iter().map(|&p| PageSpec::new(0, p)).collect::<Vec<_>>();

        if self.mode == Mode::Extract {
            let suffix = format!("pages-{}", format_list(&parts[0]).replace(", ", "_"));
            let output = super::choose_output(&*s.dialogs, &input, &suffix)?;
            let part = SplitPart { pages: specs(&parts[0]), output: output.clone() };
            let count = parts[0].len();
            return Some(Box::new(move |_| {
                split_file(&input, &[part], |_| {}).map_err(|e| e.to_string())?;
                Ok(Outcome { message: format!("Extracted {} to", plural(count, "page")), output: Some(output) })
            }));
        }

        let folder = s.dialogs.pick_folder(&input)?;
        let split: Vec<SplitPart> = parts
            .iter()
            .enumerate()
            .map(|(i, pages)| SplitPart { pages: specs(pages), output: folder.join(format!("{}-{}.pdf", self.base_name, i + 1)) })
            .collect();
        let existing: Vec<String> = split.iter().filter(|p| p.output.exists()).map(|p| super::file_name(&p.output)).collect();
        if !existing.is_empty() {
            let shown: Vec<&str> = existing.iter().take(5).map(String::as_str).collect();
            let more = if existing.len() > 5 { format!("\n…and {} more", existing.len() - 5) } else { String::new() };
            let message = format!("{} already exist and will be replaced:\n\n{}{more}", plural(existing.len(), "file"), shown.join("\n"));
            if !s.dialogs.confirm("Replace files?", &message) {
                return None;
            }
        }
        let first = split[0].output.clone();
        Some(Box::new(move |report| {
            let total = split.len();
            let count = split_file(&input, &split, |i| {
                report(format!("Writing file {} of {total}…", i + 1), i as f32 / total as f32)
            })
            .map_err(|e| e.to_string())?;
            Ok(Outcome { message: format!("Created {}, starting with", plural(count, "file")), output: Some(first) })
        }))
    }
}
