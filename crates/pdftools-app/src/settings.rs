//! Preferences kept between launches, in a small text file in the usual
//! place for each system.

use std::path::PathBuf;

/// Which theme to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    /// Follow the system's light or dark setting.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub fn name(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "light" => ThemeChoice::Light,
            "dark" => ThemeChoice::Dark,
            _ => ThemeChoice::System,
        }
    }

    /// Whether to use the dark theme, given the system's setting.
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            ThemeChoice::System => system_dark,
            ThemeChoice::Light => false,
            ThemeChoice::Dark => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Settings {
    pub theme: ThemeChoice,
}

/// `%APPDATA%\PDF Tools`, `~/Library/Application Support/PDF Tools` or
/// `$XDG_CONFIG_HOME/pdftools` (`~/.config/pdftools`).
fn dir() -> Option<PathBuf> {
    let env = |name| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        env("APPDATA").map(|d| d.join("PDF Tools"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|d| d.join("Library/Application Support/PDF Tools"))
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config"))).map(|d| d.join("pdftools"))
    }
}

impl Settings {
    /// Reads `key=value` lines; anything unknown or missing keeps its default.
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            if let Some(("theme", value)) = line.split_once('=') {
                settings.theme = ThemeChoice::parse(value);
            }
        }
        settings
    }

    pub fn to_text(self) -> String {
        format!("theme={}\n", self.theme.name())
    }

    pub fn load() -> Self {
        dir()
            .and_then(|d| std::fs::read_to_string(d.join("settings.txt")).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Saves quietly: preferences are a convenience, never worth an error.
    pub fn save(self) {
        if let Some(dir) = dir() {
            if std::fs::create_dir_all(&dir).is_ok() {
                let _ = std::fs::write(dir.join("settings.txt"), self.to_text());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_it_writes() {
        for theme in [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark] {
            let settings = Settings { theme };
            assert_eq!(Settings::parse(&settings.to_text()), settings);
        }
        assert_eq!(Settings::parse("nonsense\ntheme=purple"), Settings::default());
        assert!(ThemeChoice::System.is_dark(true) && !ThemeChoice::Light.is_dark(true) && ThemeChoice::Dark.is_dark(false));
    }
}
