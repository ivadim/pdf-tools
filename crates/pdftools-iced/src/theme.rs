//! Colours and widget styles, matching the Slint version's light theme.

use iced::border::Radius;
use iced::widget::{button, container, svg, text_input};
use iced::{color, Background, Border, Color, Shadow, Theme, Vector};

pub const BG: Color = color!(0xf4f5f8);
pub const SURFACE: Color = color!(0xffffff);
pub const SURFACE_2: Color = color!(0xf6f7fa);
pub const SURFACE_3: Color = color!(0xeceef3);
pub const SIDEBAR: Color = color!(0xeceef3);
pub const BORDER: Color = color!(0xdfe2e9);
pub const TEXT: Color = color!(0x1c2130);
pub const MUTED: Color = color!(0x6b7283);
pub const ACCENT: Color = color!(0x2f5bd3);
pub const ACCENT_HOVER: Color = color!(0x2550c2);
pub const ACCENT_SOFT: Color = color!(0xe6ecfc);
pub const ERROR: Color = color!(0xc43d3d);
pub const SUCCESS: Color = color!(0x23885a);
pub const CATALOG_RED: Color = color!(0xe5484d);

pub const RADIUS: f32 = 12.0;
pub const FONT: f32 = 14.0;
pub const FONT_SMALL: f32 = 13.0;
pub const FONT_TITLE: f32 = 20.0;

pub fn theme() -> Theme {
    Theme::custom(
        "PDF Tools",
        iced::theme::Palette {
            background: BG,
            text: TEXT,
            primary: ACCENT,
            success: SUCCESS,
            warning: color!(0xc77c12),
            danger: ERROR,
        },
    )
}

fn border(width: f32, color: Color, radius: f32) -> Border {
    Border { width, color, radius: Radius::from(radius) }
}

fn shadow(alpha: f32, blur: f32, y: f32) -> Shadow {
    Shadow { color: Color { a: alpha, ..Color::BLACK }, offset: Vector::new(0.0, y), blur_radius: blur }
}

// ── Containers ───────────────────────────────────────────────────────────

/// A white card with a border, like the Slint `Panel`.
pub fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE)),
        border: border(1.0, BORDER, RADIUS),
        ..Default::default()
    }
}

pub fn sidebar(_: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(SIDEBAR)), ..Default::default() }
}

pub fn page_bg(_: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(BG)), text_color: Some(TEXT), ..Default::default() }
}

pub fn grid_bg(_: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(SURFACE_2)), ..Default::default() }
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
    container::Style { background: Some(Background::Color(ACCENT_SOFT)), text_color: Some(TEXT), ..Default::default() }
}

pub fn hover_bar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE)),
        border: border(0.0, Color::TRANSPARENT, 8.0),
        shadow: shadow(0.2, 4.0, 1.0),
        ..Default::default()
    }
}

pub fn drop_zone(hovered: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(SURFACE_2)),
        border: border(1.5, if hovered { ACCENT } else { BORDER }, 12.0),
        ..Default::default()
    }
}

pub fn overlay(_: &Theme) -> container::Style {
    container::Style { background: Some(Background::Color(color!(0x0a0c12, 0.86))), ..Default::default() }
}

pub fn badge(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ACCENT_SOFT)),
        text_color: Some(ACCENT),
        border: border(0.0, Color::TRANSPARENT, 6.0),
        ..Default::default()
    }
}

// ── Buttons ──────────────────────────────────────────────────────────────

pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => ACCENT_HOVER,
        button::Status::Disabled => Color { a: 0.45, ..ACCENT },
        button::Status::Active => ACCENT,
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
        button::Status::Hovered | button::Status::Pressed => (SURFACE_2, TEXT),
        button::Status::Disabled => (SURFACE, Color { a: 0.4, ..TEXT }),
        button::Status::Active => (SURFACE, TEXT),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: text,
        border: border(1.0, BORDER, 9.0),
        ..Default::default()
    }
}

/// An icon-only button: no frame until hovered.
pub fn icon_button(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(Background::Color(SURFACE_3)),
        _ => None,
    };
    button::Style {
        background,
        text_color: if status == button::Status::Disabled { Color { a: 0.4, ..MUTED } } else { MUTED },
        border: border(0.0, Color::TRANSPARENT, 7.0),
        ..Default::default()
    }
}

pub fn nav(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = if active {
            Some(Background::Color(SURFACE))
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(Background::Color(SURFACE_3))
        } else {
            None
        };
        button::Style {
            background,
            text_color: TEXT,
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
            Some(Background::Color(SURFACE))
        } else if matches!(status, button::Status::Hovered) {
            Some(Background::Color(Color { a: 0.5, ..SURFACE }))
        } else {
            None
        };
        button::Style {
            background,
            text_color: if selected { TEXT } else { MUTED },
            border: border(0.0, Color::TRANSPARENT, 7.0),
            shadow: if selected { shadow(0.12, 2.0, 0.0) } else { Shadow::default() },
            ..Default::default()
        }
    }
}

pub fn segmented_track(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE_3)),
        border: border(0.0, Color::TRANSPARENT, 9.0),
        ..Default::default()
    }
}

pub fn card_option(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(Background::Color(if selected { ACCENT_SOFT } else { SURFACE })),
        text_color: TEXT,
        border: border(
            1.0,
            if selected || matches!(status, button::Status::Hovered) { ACCENT } else { BORDER },
            10.0,
        ),
        ..Default::default()
    }
}

/// A tool card on the home screen.
pub fn tool_card(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(SURFACE)),
        text_color: TEXT,
        border: border(
            1.0,
            if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                Color { a: 0.5, ..ACCENT }
            } else {
                BORDER
            },
            16.0,
        ),
        ..Default::default()
    }
}

pub fn link(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: if matches!(status, button::Status::Hovered) { ACCENT_HOVER } else { ACCENT },
        ..Default::default()
    }
}

// ── Inputs and icons ─────────────────────────────────────────────────────

pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let base = text_input::default(theme, status);
    text_input::Style {
        background: Background::Color(SURFACE),
        border: border(1.0, if matches!(status, text_input::Status::Focused { .. }) { ACCENT } else { BORDER }, 8.0),
        ..base
    }
}

pub fn tint(color: Color) -> impl Fn(&Theme, svg::Status) -> svg::Style {
    move |_, _| svg::Style { color: Some(color) }
}
