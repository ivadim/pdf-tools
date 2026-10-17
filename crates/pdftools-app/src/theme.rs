//! Colours and widget styles for the light and dark themes.

use iced::border::Radius;
use iced::widget::{button, container, svg, text_input};
use iced::{color, Background, Border, Color, Shadow, Theme, Vector};

/// The colours of one theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    pub dark: bool,
    pub bg: Color,
    pub surface: Color,
    pub surface_2: Color,
    pub surface_3: Color,
    /// The selected segment, lifted above its track.
    pub raised: Color,
    pub sidebar: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_soft: Color,
    pub error: Color,
    pub success: Color,
    pub warning: Color,
    /// How dark shadows are.
    pub shadow: f32,
}

/// The light theme.
pub const LIGHT: Colors = Colors {
    dark: false,
    bg: color!(0xf4f5f8),
    surface: color!(0xffffff),
    surface_2: color!(0xf6f7fa),
    surface_3: color!(0xeceef3),
    raised: color!(0xffffff),
    sidebar: color!(0xeceef3),
    border: color!(0xdfe2e9),
    text: color!(0x1c2130),
    muted: color!(0x6b7283),
    accent: color!(0x2f5bd3),
    accent_hover: color!(0x2550c2),
    accent_soft: color!(0xe6ecfc),
    error: color!(0xc43d3d),
    success: color!(0x23885a),
    warning: color!(0xc77c12),
    shadow: 1.0,
};

/// The dark theme.
pub const DARK: Colors = Colors {
    dark: true,
    bg: color!(0x0e0f13),
    surface: color!(0x18191e),
    surface_2: color!(0x1e2026),
    surface_3: color!(0x26282f),
    raised: color!(0x383b45),
    sidebar: color!(0x131418),
    border: color!(0x2a2c34),
    text: color!(0xe8e9ee),
    muted: color!(0x9196a5),
    accent: color!(0x5b82f0),
    accent_hover: color!(0x6c90f5),
    accent_soft: color!(0x232f52),
    error: color!(0xef6b6b),
    success: color!(0x4cc28a),
    warning: color!(0xe0a03a),
    shadow: 2.5,
};

thread_local! {
    /// The theme in use. Views and styles run on the UI thread; tests on
    /// their own threads each get their own.
    static CURRENT: std::cell::Cell<&'static Colors> = const { std::cell::Cell::new(&LIGHT) };
}

/// The colours of the theme in use.
pub fn c() -> &'static Colors {
    CURRENT.with(std::cell::Cell::get)
}

pub fn set_dark(dark: bool) {
    CURRENT.with(|current| current.set(if dark { &DARK } else { &LIGHT }));
}

pub const RADIUS: f32 = 12.0;
pub const FONT: f32 = 14.0;
pub const FONT_SMALL: f32 = 13.0;
pub const FONT_TITLE: f32 = 20.0;

/// The iced theme for the colours in use (inputs, sliders, scrollbars…).
pub fn theme() -> Theme {
    let c = c();
    Theme::custom(
        if c.dark { "PDF Tools Dark" } else { "PDF Tools" },
        iced::theme::Palette {
            background: c.bg,
            text: c.text,
            primary: c.accent,
            success: c.success,
            warning: c.warning,
            danger: c.error,
        },
    )
}

fn border(width: f32, color: Color, radius: f32) -> Border {
    Border { width, color, radius: Radius::from(radius) }
}

/// A shadow, stronger in the dark theme so it still shows.
fn shadow(alpha: f32, blur: f32, y: f32) -> Shadow {
    Shadow { color: Color { a: (alpha * c().shadow).min(1.0), ..Color::BLACK }, offset: Vector::new(0.0, y), blur_radius: blur }
}

// ── Containers ───────────────────────────────────────────────────────────

/// A card with a border.
pub fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(c().surface)),
        border: border(1.0, c().border, RADIUS),
        ..Default::default()
    }
}

pub fn sidebar(_: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(c().sidebar)), ..Default::default() }
}

pub fn page_bg(_: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(c().bg)), text_color: Some(c().text), ..Default::default() }
}

pub fn grid_bg(_: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(c().surface_2)), ..Default::default() }
}

/// A rounded square behind a tool's icon, tinted with its colour.
pub fn chip(tint: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(Color { a: 0.16, ..tint })),
        border: border(0.0, Color::TRANSPARENT, 9.0),
        ..Default::default()
    }
}

/// A page thumbnail: white with a soft shadow, so its real shape shows.
pub fn page(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::WHITE)),
        shadow: shadow(0.23, 3.0, 1.0),
        ..Default::default()
    }
}

pub fn notice(_: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(c().accent_soft)), text_color: Some(c().text), ..Default::default() }
}

pub fn hover_bar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(c().surface)),
        border: border(0.0, Color::TRANSPARENT, 8.0),
        shadow: shadow(0.2, 4.0, 1.0),
        ..Default::default()
    }
}

pub fn drop_zone(hovered: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(c().surface_2)),
        border: border(1.5, if hovered { c().accent } else { c().border }, 12.0),
        ..Default::default()
    }
}

/// Where a signature is drawn or previewed: always white, like the page
/// it will be placed on, so every ink shows as it will print.
pub fn paper(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::WHITE)),
        border: border(1.5, c().border, 12.0),
        ..Default::default()
    }
}

pub fn overlay(_: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(color!(0x0a0c12, 0.86))), ..Default::default() }
}

pub fn badge(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(c().accent_soft)),
        text_color: Some(c().accent),
        border: border(0.0, Color::TRANSPARENT, 6.0),
        ..Default::default()
    }
}

// ── Buttons ──────────────────────────────────────────────────────────────

pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => c().accent_hover,
        button::Status::Disabled => Color { a: 0.45, ..c().accent },
        button::Status::Active => c().accent,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: Color::WHITE,
        border: border(0.0, Color::TRANSPARENT, 9.0),
        ..Default::default()
    }
}

pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    let (background, text) = match status {
        button::Status::Hovered | button::Status::Pressed => (c().surface_2, c().text),
        button::Status::Disabled => (c().surface, Color { a: 0.4, ..c().text }),
        button::Status::Active => (c().surface, c().text),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: text,
        border: border(1.0, c().border, 9.0),
        ..Default::default()
    }
}

/// An icon-only button: no frame until hovered.
pub fn icon_button(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(Background::Color(c().surface_3)),
        _ => None,
    };
    button::Style {
        background,
        text_color: if status == button::Status::Disabled { Color { a: 0.4, ..c().muted } } else { c().muted },
        border: border(0.0, Color::TRANSPARENT, 7.0),
        ..Default::default()
    }
}

pub fn nav(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = if active {
            Some(Background::Color(c().surface))
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(Background::Color(c().surface_3))
        } else {
            None
        };
        button::Style {
            background,
            text_color: c().text,
            border: border(0.0, Color::TRANSPARENT, 9.0),
            shadow: if active { shadow(0.08, 2.0, 0.0) } else { Shadow::default() },
            ..Default::default()
        }
    }
}

/// A selectable option in a segmented control or a list of cards.
pub fn segment(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = if selected {
            Some(Background::Color(c().raised))
        } else if matches!(status, button::Status::Hovered) {
            Some(Background::Color(Color { a: 0.5, ..c().raised }))
        } else {
            None
        };
        button::Style {
            background,
            text_color: if selected { c().text } else { c().muted },
            border: border(0.0, Color::TRANSPARENT, 7.0),
            shadow: if selected { shadow(0.12, 2.0, 0.0) } else { Shadow::default() },
            ..Default::default()
        }
    }
}

pub fn segmented_track(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(c().surface_3)),
        border: border(0.0, Color::TRANSPARENT, 9.0),
        ..Default::default()
    }
}

pub fn card_option(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(Background::Color(if selected { c().accent_soft } else { c().surface })),
        text_color: c().text,
        border: border(
            1.0,
            if selected || matches!(status, button::Status::Hovered) { c().accent } else { c().border },
            10.0,
        ),
        ..Default::default()
    }
}

/// A tool card on the home screen.
pub fn tool_card(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(c().surface)),
        text_color: c().text,
        border: border(
            1.0,
            if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                Color { a: 0.5, ..c().accent }
            } else {
                c().border
            },
            16.0,
        ),
        ..Default::default()
    }
}

pub fn link(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: if matches!(status, button::Status::Hovered) { c().accent_hover } else { c().accent },
        ..Default::default()
    }
}

// ── Inputs and icons ─────────────────────────────────────────────────────

pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let base = text_input::default(theme, status);
    text_input::Style {
        background: Background::Color(c().surface),
        border: border(1.0, if matches!(status, text_input::Status::Focused { .. }) { c().accent } else { c().border }, 8.0),
        ..base
    }
}

pub fn tint(color: Color) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |_, _| svg::Style { color: Some(color) }
}
