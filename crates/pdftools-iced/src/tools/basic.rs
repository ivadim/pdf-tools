//! Compress, Grayscale, Rotate, Protect and Unlock.

use iced::widget::{checkbox, column, row, text, text_input};
use iced::{Alignment, Element, Length};
use pdftools_core::compress::{compress_document, Level};
use pdftools_core::grayscale::grayscale_document;
use pdftools_core::security::{protect_document, Protection};
use pdftools_core::PageSpec;

use super::{set, toggle};
use crate::app::Message;
use crate::previews::Thumb;
use crate::catalog::ToolId;
use crate::icons;
use crate::dialogs::Dialogs;
use crate::single::{choose_output, edit, format_bytes, plural, Job, Single, ToolOptions};
use crate::theme;
use crate::widgets::{card_options, field, icon_button, muted, section, Caps, TileEvent};

// ── Compress ─────────────────────────────────────────────────────────────

pub struct Compress {
    level: Level,
}

impl Default for Compress {
    fn default() -> Self {
        Self { level: Level::Balanced }
    }
}

impl ToolOptions for Compress {
    fn id(&self) -> ToolId {
        ToolId::Compress
    }

    fn view<'a>(&'a self, file: &'a Single, _closeup: Option<&'a Thumb>) -> Element<'a, Message> {
        let selected = match self.level {
            Level::Lossless => "lossless",
            Level::Balanced => "balanced",
            Level::Strong => "strong",
        };
        section(
            "Compression",
            [card_options(
                &[
                    ("lossless", "Lossless", "Removes unused data. Pages look exactly the same."),
                    ("balanced", "Balanced", "Images up to 2000 px at good JPEG quality. Recommended."),
                    ("strong", "Strong", "Images up to 1200 px at lower quality. Smallest files."),
                ],
                selected,
                |v| Message::Option(ToolId::Compress, "level", v.into()),
            ), estimate(file)],
        )
    }

    fn set(&mut self, key: &str, value: String, _file: &Single, _dialogs: &dyn Dialogs) {
        if key == "level" {
            self.level = match value.as_str() {
                "lossless" => Level::Lossless,
                "strong" => Level::Strong,
                _ => Level::Balanced,
            };
        }
    }

    fn validate(&self, _file: &Single) -> Option<String> {
        None
    }

    fn run_label(&self) -> &'static str {
        "Compress PDF…"
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let output = choose_output(dialogs, file, "compressed")?;
        let input = file.path().expect("file loaded").to_path_buf();
        let (keep, level, note) = (file.keep(), self.level, file.removed_note());
        Some(Box::new(move || {
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
            Ok((format!("{change}{detail}{note}. Saved as"), Some(output)))
        }))
    }
}

/// What compression can work with: the share of the file that's images.
fn estimate<'a>(file: &Single) -> Element<'a, Message> {
    let Some(info) = file.loaded().map(|f| &f.info) else { return column![].into() };
    let share = if info.file_size > 0 { info.image_bytes as f64 / info.file_size as f64 } else { 0.0 };
    let text_of = if info.images == 0 {
        "There are no images in this file, so only Lossless clean-up applies; savings will be small.".to_owned()
    } else if share < 0.2 {
        format!(
            "Images are only {} of this file ({:.0}%), so even Strong won’t shrink it much.",
            format_bytes(info.image_bytes),
            share * 100.0
        )
    } else {
        format!(
            "{} make up {} of {} ({:.0}%). Balanced and Strong re-encode these; that’s where the savings are.",
            plural(info.images, "image"),
            format_bytes(info.image_bytes),
            format_bytes(info.file_size),
            share * 100.0
        )
    };
    text(text_of).size(13).color(theme::ACCENT).into()
}

// ── Grayscale ────────────────────────────────────────────────────────────

pub struct Grayscale {
    images: bool,
}

impl Default for Grayscale {
    fn default() -> Self {
        Self { images: true }
    }
}

impl ToolOptions for Grayscale {
    fn id(&self) -> ToolId {
        ToolId::Grayscale
    }

    fn view<'a>(&'a self, _file: &'a Single, _closeup: Option<&'a Thumb>) -> Element<'a, Message> {
        section(
            "Options",
            [
                checkbox(self.images).label("Convert images").on_toggle(toggle(ToolId::Grayscale, "images")).into(),
                muted("Text and shapes are converted while staying sharp and selectable. Gradients and spot colours keep their colour."),
            ],
        )
    }

    fn set(&mut self, key: &str, value: String, _file: &Single, _dialogs: &dyn Dialogs) {
        if key == "images" {
            self.images = value == "1";
        }
    }

    fn gray(&self) -> bool {
        true
    }

    fn validate(&self, _file: &Single) -> Option<String> {
        None
    }

    fn run_label(&self) -> &'static str {
        "Convert to grayscale…"
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let output = choose_output(dialogs, file, "grayscale")?;
        let input = file.path().expect("file loaded").to_path_buf();
        let (keep, images, note) = (file.keep(), self.images, file.removed_note());
        Some(Box::new(move || {
            edit(&input, keep.as_deref(), &output, |doc| Ok(grayscale_document(doc, images)))?;
            Ok((format!("Grayscale copy{note} saved as"), Some(output)))
        }))
    }
}

// ── Rotate ───────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Rotate {
    rotations: Vec<i32>,
}

impl Rotate {
    fn turn(&mut self, pages: impl IntoIterator<Item = usize>, delta: i32) {
        for page in pages {
            if let Some(r) = self.rotations.get_mut(page - 1) {
                *r = (*r + delta).rem_euclid(360);
            }
        }
    }
}

impl ToolOptions for Rotate {
    fn id(&self) -> ToolId {
        ToolId::Rotate
    }

    fn view<'a>(&'a self, _file: &'a Single, _closeup: Option<&'a Thumb>) -> Element<'a, Message> {
        muted("Turn pages with the buttons on each page, or all at once with the buttons above them.")
    }

    fn set(&mut self, _key: &str, _value: String, _file: &Single, _dialogs: &dyn Dialogs) {}

    fn caps(&self) -> Caps {
        Caps { include: true, rotate: true, ..Default::default() }
    }

    fn rotation(&self, page: usize) -> i32 {
        self.rotations.get(page - 1).copied().unwrap_or(0)
    }

    fn tile_event(&mut self, _index: usize, page: usize, event: TileEvent) {
        match event {
            TileEvent::RotateLeft => self.turn([page], -90),
            TileEvent::RotateRight => self.turn([page], 90),
            _ => {}
        }
    }

    fn grid_actions(&self) -> Vec<(&'static str, fn() -> iced::widget::svg::Handle, &'static str)> {
        vec![
            ("Rotate all left", icons::rotate_ccw, "rotate-all-left"),
            ("Rotate all right", icons::rotate_cw, "rotate-all-right"),
            ("Reset rotation", icons::reset, "reset"),
        ]
    }

    fn grid_action(&mut self, action: &str, file: &Single) {
        let pages = file.page_count();
        match action {
            "rotate-all-left" => self.turn(1..=pages, -90),
            "rotate-all-right" => self.turn(1..=pages, 90),
            "reset" => self.rotations.iter_mut().for_each(|r| *r = 0),
            _ => {}
        }
    }

    fn file_changed(&mut self, file: &Single) {
        self.rotations = vec![0; file.page_count()];
    }

    fn validate(&self, file: &Single) -> Option<String> {
        let rotated = self.rotations.iter().enumerate().any(|(i, r)| *r != 0 && !file.excluded.contains(&(i + 1)));
        (!rotated && file.excluded.is_empty())
            .then(|| "Rotate at least one page — use the buttons on each page or above.".into())
    }

    fn run_label(&self) -> &'static str {
        "Save rotated PDF…"
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let output = choose_output(dialogs, file, "rotated")?;
        let input = file.path().expect("file loaded").to_path_buf();
        let pages: Vec<PageSpec> =
            file.kept_pages().into_iter().map(|p| PageSpec::new(0, p).rotated(self.rotation(p) as i64)).collect();
        let note = file.removed_note();
        Some(Box::new(move || {
            pdftools_core::organize::organize_file(&input, &pages, &output).map_err(|e| e.to_string())?;
            let changed = pages.iter().filter(|p| p.rotation != 0).count();
            let what =
                if changed > 0 { format!("Rotated {}", plural(changed, "page")) } else { format!("Saved {}", plural(pages.len(), "page")) };
            Ok((format!("{what}{note} into"), Some(output)))
        }))
    }
}

// ── Protect ──────────────────────────────────────────────────────────────

pub struct Protect {
    show: bool,
    password: String,
    confirm: String,
    owner: String,
    /// Print, copy, modify, annotate.
    allow: [bool; 4],
}

impl Default for Protect {
    fn default() -> Self {
        Self { show: false, password: String::new(), confirm: String::new(), owner: String::new(), allow: [true; 4] }
    }
}

impl ToolOptions for Protect {
    fn id(&self) -> ToolId {
        ToolId::Protect
    }

    fn view<'a>(&'a self, _file: &'a Single, _closeup: Option<&'a Thumb>) -> Element<'a, Message> {
        let t = ToolId::Protect;
        let secret = |placeholder: &'a str, value: &'a str, key: &'static str| {
            text_input(placeholder, value)
                .secure(!self.show)
                .on_input(set(t, key))
                .on_submit(Message::Run(t))
                .padding(8)
                .style(theme::input)
        };
        let (eye, eye_label) = if self.show { (icons::eye_off(), "Hide passwords") } else { (icons::eye(), "Show passwords") };
        let mut password = column![
            row![
                secret("Password", &self.password, "password").width(Length::Fill),
                icon_button(eye, eye_label, 34.0, Some(Message::Option(t, "show", if self.show { "0" } else { "1" }.into()))),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(5);
        if let Some((label, color)) = strength(&self.password) {
            password = password.push(text(label).size(12).color(color));
        }
        let mut confirm = column![secret("Password again", &self.confirm, "confirm")].spacing(5);
        if !self.confirm.is_empty() && self.confirm != self.password {
            confirm = confirm.push(text("The passwords don’t match.").size(12).color(theme::ERROR));
        }
        column![
            section(
                "Password",
                [field("Password to open the file", password), field("Repeat the password", confirm)]
            ),
            section(
                "Permissions",
                [
                    checkbox(self.allow[0]).label("Allow printing").on_toggle(toggle(t, "print")).into(),
                    checkbox(self.allow[1]).label("Allow copying text and images").on_toggle(toggle(t, "copy")).into(),
                    checkbox(self.allow[2]).label("Allow changes").on_toggle(toggle(t, "modify")).into(),
                    checkbox(self.allow[3]).label("Allow comments and form filling").on_toggle(toggle(t, "annotate")).into(),
                    field("Owner password (optional)", secret("Lifts the restrictions", &self.owner, "owner")),
                    muted("Encrypted with AES-256. Restrictions are honoured by most viewers, but can't be enforced."),
                ]
            ),
        ]
        .spacing(18)
        .into()
    }

    fn set(&mut self, key: &str, value: String, _file: &Single, _dialogs: &dyn Dialogs) {
        let on = value == "1";
        match key {
            "show" => self.show = value == "1",
            "password" => self.password = value,
            "confirm" => self.confirm = value,
            "owner" => self.owner = value,
            "print" => self.allow[0] = on,
            "copy" => self.allow[1] = on,
            "modify" => self.allow[2] = on,
            "annotate" => self.allow[3] = on,
            _ => {}
        }
    }

    fn validate(&self, _file: &Single) -> Option<String> {
        if self.password.is_empty() {
            Some("Enter a password.".into())
        } else if self.password != self.confirm {
            Some("The passwords don’t match.".into())
        } else {
            None
        }
    }

    fn run_label(&self) -> &'static str {
        "Protect PDF…"
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let output = choose_output(dialogs, file, "protected")?;
        let input = file.path().expect("file loaded").to_path_buf();
        let protection = Protection {
            user_password: self.password.clone(),
            owner_password: (!self.owner.is_empty()).then(|| self.owner.clone()),
            allow_print: self.allow[0],
            allow_copy: self.allow[1],
            allow_modify: self.allow[2],
            allow_annotate: self.allow[3],
        };
        let (keep, note) = (file.keep(), file.removed_note());
        Some(Box::new(move || {
            edit(&input, keep.as_deref(), &output, |doc| protect_document(doc, &protection))?;
            Ok((format!("Password-protected copy{note} saved as"), Some(output)))
        }))
    }
}

/// A rough guide to a password's strength.
fn strength(password: &str) -> Option<(&'static str, iced::Color)> {
    if password.is_empty() {
        return None;
    }
    let kinds = [
        password.chars().any(|c| c.is_lowercase()),
        password.chars().any(|c| c.is_uppercase()),
        password.chars().any(|c| c.is_ascii_digit()),
        password.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|k| **k)
    .count();
    let length = password.chars().count();
    Some(if length >= 12 && kinds >= 3 {
        ("Strong password", theme::SUCCESS)
    } else if length >= 8 && kinds >= 2 {
        ("Fair password — longer is stronger", iced::color!(0xc77c12))
    } else {
        ("Weak password — use 8 or more characters, mixing letters and numbers", theme::ERROR)
    })
}

// ── Unlock ───────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Unlock {
    password: String,
}

impl ToolOptions for Unlock {
    fn id(&self) -> ToolId {
        ToolId::Unlock
    }

    fn view<'a>(&'a self, file: &'a Single, _closeup: Option<&'a Thumb>) -> Element<'a, Message> {
        let Some(loaded) = file.loaded() else { return column![].into() };
        if !loaded.info.encrypted {
            return section("Nothing to unlock", [muted("This file has no password or restrictions.")]);
        }
        if loaded.info.needs_password {
            section(
                "Password",
                [field(
                    "The file’s password",
                    text_input("Password", &self.password)
                        .secure(true)
                        .on_input(set(ToolId::Unlock, "password"))
                        .on_submit(Message::Run(ToolId::Unlock))
                        .padding(8)
                        .style(theme::input),
                )],
            )
        } else {
            section("Restrictions", [text("This file restricts printing, copying or editing. Unlocking removes those restrictions.").into()])
        }
    }

    fn set(&mut self, key: &str, value: String, _file: &Single, _dialogs: &dyn Dialogs) {
        if key == "password" {
            self.password = value;
        }
    }

    fn caps(&self) -> Caps {
        Caps::default()
    }

    fn file_changed(&mut self, _file: &Single) {
        self.password.clear();
    }

    fn allow_locked(&self) -> bool {
        true
    }

    fn empty_text(&self, file: &Single) -> Option<&'static str> {
        let info = &file.loaded()?.info;
        Some(if info.needs_password {
            "This file needs its password to open. Enter it on the left and choose “Unlock PDF…” to save an unlocked copy."
        } else if info.encrypted {
            "This file opens without a password but restricts printing, copying or editing."
        } else {
            "This file isn’t protected."
        })
    }

    fn validate(&self, file: &Single) -> Option<String> {
        let info = &file.loaded()?.info;
        if !info.encrypted {
            Some("This file isn’t protected.".into())
        } else if info.needs_password && self.password.is_empty() {
            Some("Enter the file’s password.".into())
        } else {
            None
        }
    }

    fn run_label(&self) -> &'static str {
        "Unlock PDF…"
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let output = choose_output(dialogs, file, "unlocked")?;
        let input = file.path().expect("file loaded").to_path_buf();
        let password = self.password.clone();
        Some(Box::new(move || {
            pdftools_core::security::unlock_file(&input, &password, &output).map_err(|e| e.to_string())?;
            Ok(("Unlocked copy saved as".into(), Some(output)))
        }))
    }
}
