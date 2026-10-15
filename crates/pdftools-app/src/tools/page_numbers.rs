use pdftools_core::stamps::{add_page_numbers, PageNumbers};
use pdftools_core::{Anchor, HAlign, VAlign};

use super::{choose_output, color_of, edit, Job, Outcome, Shared, SingleFile, Tool, TEXT_COLORS};
use crate::ranges::parse_list;
use crate::{single_file_plumbing, PageTile};

pub struct PageNumbersTool {
    base: SingleFile,
    pub anchor: Anchor,
    pub format: String,
    pub size: f32,
    pub margin: f32,
    pub color: [u8; 3],
    pub custom: bool,
    pub pages_text: String,
    pub start_at: i64,
}

pub fn parse_anchor(value: &str) -> Anchor {
    let mut parts = value.split_whitespace();
    let v = match parts.next() {
        Some("top") => VAlign::Top,
        Some("middle") => VAlign::Middle,
        _ => VAlign::Bottom,
    };
    let h = match parts.next() {
        Some("left") => HAlign::Left,
        Some("right") => HAlign::Right,
        _ => HAlign::Center,
    };
    Anchor::new(h, v)
}

/// Characters the built-in PDF font (Helvetica, WinAnsiEncoding) can show.
pub fn win_ansi(text: &str) -> bool {
    text.chars().all(|c| matches!(c, ' '..='~' | '\u{a0}'..='\u{ff}') || "€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ".contains(c))
}

impl PageNumbersTool {
    pub fn new() -> Self {
        Self {
            base: SingleFile::new(super::id::PAGE_NUMBERS),
            anchor: Anchor::new(HAlign::Center, VAlign::Bottom),
            format: "{n}".into(),
            size: 11.0,
            margin: 28.0,
            color: TEXT_COLORS[0],
            custom: false,
            pages_text: "2-".into(),
            start_at: 1,
        }
    }

    /// Original page numbers that get a number; left-out pages never do.
    fn numbered(&self) -> Vec<usize> {
        if !self.custom {
            return self.base.kept_pages();
        }
        parse_list(&self.pages_text, self.base.page_count())
            .map(|pages| pages.into_iter().filter(|p| !self.base.excluded.contains(p)).collect())
            .unwrap_or_default()
    }

    /// The text for an original page, if it's numbered.
    pub fn label(&self, page: usize) -> Option<String> {
        let numbered = self.numbered();
        let i = numbered.iter().position(|p| *p == page)?;
        let last = self.start_at + numbered.len() as i64 - 1;
        Some(self.format.replace("{n}", &(self.start_at + i as i64).to_string()).replace("{total}", &last.to_string()))
    }
}

impl Default for PageNumbersTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for PageNumbersTool {
    single_file_plumbing!();

    fn option(&mut self, _s: &mut Shared, key: &str, value: &str) {
        match key {
            "anchor" => self.anchor = parse_anchor(value),
            "format" => self.format = value.into(),
            "size" => self.size = value.parse().unwrap_or(self.size),
            "margin" => self.margin = value.parse().unwrap_or(self.margin),
            "color" => self.color = TEXT_COLORS[value.parse::<usize>().unwrap_or(0).min(TEXT_COLORS.len() - 1)],
            "which" => self.custom = value == "custom",
            "pages" => self.pages_text = value.into(),
            "start" => self.start_at = value.parse().unwrap_or(1),
            _ => {}
        }
    }

    fn tiles(&self) -> Vec<PageTile> {
        let numbered = self.numbered();
        self.base
            .base_tiles()
            .into_iter()
            .map(|t| PageTile { dimmed: !t.excluded && !numbered.contains(&(t.page as usize)), ..t })
            .collect()
    }

    fn decorate(&self, tile: &mut PageTile, (width, height): (f32, f32)) {
        let Some(text) = self.label(tile.page as usize) else { return };
        let (mx, my) = (self.margin / width, self.margin / height);
        (tile.overlay_x, tile.overlay_align_x) = match self.anchor.h {
            HAlign::Left => (mx, 0.0),
            HAlign::Center => (0.5, 0.5),
            HAlign::Right => (1.0 - mx, 1.0),
        };
        (tile.overlay_y, tile.overlay_align_y) = match self.anchor.v {
            VAlign::Top => (my, 0.0),
            VAlign::Middle => (0.5, 0.5),
            VAlign::Bottom => (1.0 - my, 1.0),
        };
        tile.overlay_text = text.into();
        tile.overlay_size = self.size / width;
        tile.overlay_color = color_of(self.color);
    }

    fn run_label(&self) -> String {
        "Add page numbers…".into()
    }

    fn validate(&self) -> Option<String> {
        if self.format.trim().is_empty() {
            return Some("Enter a format, e.g. Page {n}.".into());
        }
        if !win_ansi(&self.format) {
            return Some("Page numbers support Latin characters only.".into());
        }
        if self.custom {
            if let Err(e) = parse_list(&self.pages_text, self.base.page_count()) {
                return Some(e);
            }
            if self.numbered().is_empty() {
                return Some("None of the chosen pages are kept.".into());
            }
        }
        None
    }

    fn start(&mut self, s: &mut Shared) -> Option<Job> {
        let input = self.base.path()?.to_path_buf();
        let output = choose_output(&*s.dialogs, &input, "numbered")?;
        let options = PageNumbers {
            anchor: self.anchor,
            margin: self.margin,
            font_size: self.size,
            format: self.format.clone(),
            start_at: self.start_at,
            // Positions in the document after left-out pages are removed.
            pages: self.custom.then(|| self.numbered().into_iter().filter_map(|p| self.base.final_position(p)).collect()),
            color: self.color.map(|c| c as f32 / 255.0),
        };
        let (keep, note) = (self.base.keep(), self.base.removed_note());
        Some(Box::new(move |_| {
            let count = edit(&input, keep.as_deref(), &output, |doc| add_page_numbers(doc, &options))?;
            let pages = if count == 1 { "1 page".to_owned() } else { format!("{count} pages") };
            Ok(Outcome { message: format!("Numbered {pages}{note} in"), output: Some(output) })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_anchors_and_checks_text() {
        assert_eq!(parse_anchor("top left"), Anchor::new(HAlign::Left, VAlign::Top));
        assert_eq!(parse_anchor("middle right"), Anchor::new(HAlign::Right, VAlign::Middle));
        assert_eq!(parse_anchor(""), Anchor::new(HAlign::Center, VAlign::Bottom));
        assert!(win_ansi("Página – 1 €"));
        assert!(!win_ansi("Страница"));
    }
}
