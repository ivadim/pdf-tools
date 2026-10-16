//! Building blocks shared by the screens: buttons, segmented controls, the
//! tool header, the file card, the run bar and the page grid.

use iced::widget::grid::Sizing;
use iced::widget::svg::Handle;
use iced::widget::{
    button, center, column, container, grid, hover, image, mouse_area, responsive, row, scrollable, sensor, space,
    svg, text, tooltip,
};
use iced::{Alignment, Color, ContentFit, Element, Font, Length, Padding};

use crate::catalog::ToolInfo;
use crate::icons;
use crate::previews::Thumb;
use crate::single::Overlay;
use crate::theme;

pub const BOLD: Font = Font { weight: iced::font::Weight::Semibold, ..Font::DEFAULT };

pub fn icon<'a, M: 'a>(handle: Handle, size: f32, color: Color) -> Element<'a, M> {
    svg(handle).width(size).height(size).style(theme::tint(color)).into()
}

pub fn muted<'a, M: 'a>(content: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(content).size(theme::FONT_SMALL).color(theme::MUTED).into()
}

pub fn section_title<'a, M: 'a>(title: &str) -> Element<'a, M> {
    text(title.to_uppercase()).size(12).font(BOLD).color(theme::MUTED).into()
}

/// A titled group of options.
pub fn section<'a, M: 'a>(title: &str, content: impl IntoIterator<Item = Element<'a, M>>) -> Element<'a, M> {
    column![section_title(title)].extend(content).spacing(10).into()
}

/// A label above a control.
pub fn field<'a, M: 'a>(label: &'a str, control: impl Into<Element<'a, M>>) -> Element<'a, M> {
    column![text(label).size(13).font(BOLD), control.into()].spacing(5).into()
}

/// An icon-only button with a tooltip.
pub fn icon_button<'a, M: Clone + 'a>(handle: Handle, label: &'a str, size: f32, on_press: Option<M>) -> Element<'a, M> {
    let tint = if on_press.is_some() { theme::MUTED } else { Color { a: 0.4, ..theme::MUTED } };
    let button = button(center(icon(handle, size * 0.58, tint)))
        .width(size)
        .height(size)
        .padding(0)
        .style(theme::icon_button)
        .on_press_maybe(on_press);
    tooltip(button, container(text(label).size(12)).padding([4, 8]).style(theme::hover_bar), tooltip::Position::Bottom)
        .into()
}

/// A button with an optional icon; `primary` for the main action.
pub fn action<'a, M: Clone + 'a>(
    label: impl text::IntoFragment<'a>,
    handle: Option<Handle>,
    primary: bool,
    on_press: Option<M>,
) -> Element<'a, M> {
    let color = if primary { Color::WHITE } else { theme::TEXT };
    let mut content = row![].spacing(7).align_y(Alignment::Center);
    if let Some(handle) = handle {
        content = content.push(icon(handle, 16.0, color));
    }
    content = content.push(text(label).font(if primary { BOLD } else { Font::DEFAULT }));
    button(content)
        .padding([7, 13])
        .style(if primary { theme::primary } else { theme::secondary })
        .on_press_maybe(on_press)
        .into()
}

/// Mutually exclusive options side by side.
pub fn segmented<'a, M: Clone + 'a>(
    options: &[(&'static str, &'static str)],
    selected: &str,
    on_select: impl Fn(&'static str) -> M,
) -> Element<'a, M> {
    let buttons = options.iter().map(|(value, label)| {
        let chosen = *value == selected;
        button(text(*label).font(if chosen { BOLD } else { Font::DEFAULT }).width(Length::Fill).center())
            .width(Length::Fill)
            .padding([6, 4])
            .style(theme::segment(chosen))
            .on_press(on_select(value))
            .into()
    });
    container(row(buttons).spacing(3)).padding(3).style(theme::segmented_track).into()
}

/// Mutually exclusive options as cards with descriptions.
pub fn card_options<'a, M: Clone + 'a>(
    options: &[(&'static str, &'static str, &'static str)],
    selected: &str,
    on_select: impl Fn(&'static str) -> M,
) -> Element<'a, M> {
    let cards = options.iter().map(|(value, label, description)| {
        button(column![text(*label).font(BOLD), muted(*description)].spacing(2))
            .width(Length::Fill)
            .padding([9, 12])
            .style(theme::card_option(*value == selected))
            .on_press(on_select(value))
            .into()
    });
    column(cards).spacing(6).into()
}

/// A tool's coloured icon on a tinted square.
pub fn chip<'a, M: 'a>(info: &ToolInfo, size: f32) -> Element<'a, M> {
    container(icon((info.icon)(), size * 0.55, info.tint))
        .width(size)
        .height(size)
        .center(size)
        .style(theme::chip(info.tint))
        .into()
}

pub fn tool_header<'a, M: 'a>(info: &ToolInfo) -> Element<'a, M> {
    row![
        chip(info, 46.0),
        column![
            text(info.name).size(theme::FONT_TITLE).font(BOLD),
            text(info.description).color(theme::MUTED),
        ]
        .spacing(3)
    ]
    .spacing(14)
    .align_y(Alignment::Center)
    .into()
}

// ── Run bar ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Run {
    #[default]
    Idle,
    Running(String),
    Done { message: String, output: Option<std::path::PathBuf> },
    Failed(String),
}

impl Run {
    pub fn busy(&self) -> bool {
        matches!(self, Run::Running(_))
    }
}

/// Status on the left (what's missing, progress or the result) and the
/// tool's main button on the right.
pub fn run_bar<'a, M: Clone + 'a>(
    run: &'a Run,
    hint: Option<String>,
    label: &'a str,
    handle: Handle,
    on_run: M,
    on_reveal: M,
) -> Element<'a, M> {
    let status: Element<'a, M> = match run {
        Run::Running(message) => text(message.clone()).color(theme::MUTED).into(),
        Run::Done { message, output } => {
            let mut done = row![icon(icons::check(), 16.0, theme::SUCCESS), text(message.clone()).color(theme::TEXT)]
                .spacing(6)
                .align_y(Alignment::Center);
            if let Some(output) = output {
                let name = output.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                done = done.push(button(text(name).font(BOLD)).padding(0).style(theme::link).on_press(on_reveal));
            }
            done.into()
        }
        Run::Failed(message) => text(message.clone()).color(theme::ERROR).into(),
        Run::Idle => text(hint.clone().unwrap_or_else(|| "Ready.".into())).color(theme::MUTED).into(),
    };
    let enabled = !run.busy() && (hint.is_none() || matches!(run, Run::Done { .. } | Run::Failed(_)));
    let ready = hint.is_none() && !run.busy() && enabled;
    row![
        container(status).width(Length::Fill),
        container(action(label, Some(handle), true, ready.then_some(on_run))).width(Length::FillPortion(1)).align_right(Length::Fill),
    ]
    .spacing(16)
    .padding(Padding { top: 12.0, right: 24.0, bottom: 18.0, left: 24.0 })
    .align_y(Alignment::Center)
    .into()
}

// ── Page grid ────────────────────────────────────────────────────────────

/// A page as the grid shows it.
#[derive(Debug, Clone, Default)]
pub struct Tile {
    /// Identifies the preview shown; when it changes (e.g. the page is
    /// rotated) the tile asks for a new one.
    pub key: String,
    /// 1-based page number in its source.
    pub page: usize,
    pub label: String,
    pub rotation: i32,
    pub excluded: bool,
    pub thumb: Option<Thumb>,
    pub failed: bool,
    pub badge: Option<String>,
    pub stripe: Option<Color>,
    pub dimmed: bool,
    pub selected: bool,
    pub overlay: Option<Overlay>,
    /// A blank page of this aspect, shown instead of a rendered one (the
    /// overlay supplies the content, e.g. a photo placed on the page).
    pub blank: Option<f32>,
}

/// What a grid offers.
#[derive(Debug, Clone, Copy, Default)]
pub struct Caps {
    pub include: bool,
    pub rotate: bool,
    pub delete: bool,
    pub duplicate: bool,
    pub reorder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileEvent {
    /// Scrolled into view: render its preview.
    Shown,
    View,
    RotateLeft,
    RotateRight,
    ToggleInclude,
    Duplicate,
    Delete,
    MoveEarlier,
    MoveLater,
}

/// The page fitted into `size`, keeping its proportions.
fn fitted(aspect: f32, size: iced::Size) -> (f32, f32) {
    if aspect > size.width / size.height.max(1.0) {
        (size.width, size.width / aspect)
    } else {
        (size.height * aspect, size.height)
    }
}

fn tile_view<'a, M: Clone + 'a>(
    index: usize,
    count: usize,
    tile: Tile,
    caps: Caps,
    busy: bool,
    on: &dyn Fn(usize, TileEvent) -> M,
) -> Element<'a, M> {
    let thumb = tile.thumb.clone();
    let failed = tile.failed;
    let excluded = tile.excluded;
    let selected = tile.selected;
    let overlay = tile.overlay.clone();
    let blank = tile.blank;
    let page = responsive(move |size| {
        let aspect = thumb.as_ref().map(|t| t.aspect).or(blank).unwrap_or(1.0 / 1.3);
        let (w, h) = fitted(aspect, size);
        let content: Element<'_, M> = match &thumb {
            Some(t) => image(t.handle.clone()).width(w).height(h).content_fit(ContentFit::Fill).into(),
            None if failed => center(muted("No preview")).into(),
            None => space().into(),
        };
        let mut layers = iced::widget::stack![content].width(w).height(h);
        if let (Some(overlay), true) = (&overlay, thumb.is_some() || blank.is_some()) {
            layers = layers.push(overlay_view(overlay, w, h));
        }
        let frame = container(layers).width(w).height(h).style(move |t| {
            let mut style = theme::page(t);
            if selected {
                style.border = iced::Border { width: 3.0, color: theme::ACCENT, radius: 2.0.into() };
            }
            style
        });
        center(frame).into()
    });
    let mut base = iced::widget::stack![page];
    if excluded {
        base = base.push(center(
            container(text("Removed").size(12).font(BOLD).color(Color::WHITE))
                .padding([3, 10])
                .style(|_| container::Style {
                    background: Some(theme::TEXT.into()),
                    border: iced::border::rounded(11),
                    ..Default::default()
                }),
        ));
    }
    if tile.dimmed {
        // Fades the page into the grid background.
        base = base.push(container(space()).width(Length::Fill).height(Length::Fill).style(|_| container::Style {
            background: Some(Color { a: 0.6, ..theme::SURFACE_2 }.into()),
            ..Default::default()
        }));
    }
    let base = mouse_area(container(base).height(Length::Fill)).on_press(on(index, TileEvent::View));

    let mut actions = row![].spacing(1);
    if caps.reorder {
        actions = actions
            .push(icon_button(icons::chevron_left(), "Move earlier", 26.0, (!busy && index > 0).then(|| on(index, TileEvent::MoveEarlier))))
            .push(icon_button(icons::chevron_right(), "Move later", 26.0, (!busy && index + 1 < count).then(|| on(index, TileEvent::MoveLater))));
    }
    if caps.rotate {
        actions = actions
            .push(icon_button(icons::rotate_ccw(), "Rotate left", 26.0, (!busy).then(|| on(index, TileEvent::RotateLeft))))
            .push(icon_button(icons::rotate_cw(), "Rotate right", 26.0, (!busy).then(|| on(index, TileEvent::RotateRight))));
    }
    if caps.duplicate {
        actions = actions.push(icon_button(icons::copy(), "Duplicate", 26.0, (!busy).then(|| on(index, TileEvent::Duplicate))));
    }
    if caps.delete {
        actions = actions.push(icon_button(icons::trash(), "Remove", 26.0, (!busy).then(|| on(index, TileEvent::Delete))));
    }
    let include = |checked: bool| -> Element<'a, M> {
        let mark: Element<'a, M> = if checked { icon(icons::check(), 15.0, Color::WHITE) } else { space().into() };
        button(center(mark))
            .width(24)
            .height(24)
            .padding(0)
            .style(move |_, _| button::Style {
                background: Some((if checked { theme::ACCENT } else { theme::SURFACE }).into()),
                border: iced::Border { width: 2.0, color: if checked { theme::ACCENT } else { theme::MUTED }, radius: 6.0.into() },
                ..Default::default()
            })
            .on_press_maybe((!busy).then(|| on(index, TileEvent::ToggleInclude)))
            .into()
    };
    let mut top = row![].padding(6);
    if caps.include && !excluded {
        top = top.push(include(true));
    }
    top = top.push(space::horizontal()).push(container(actions).padding(2).style(theme::hover_bar));
    // Left-out pages always show their (empty) box, so they can be put back.
    let base: Element<'a, M> = if caps.include && excluded {
        iced::widget::stack![base, container(include(false)).padding(6)].into()
    } else {
        base.into()
    };
    let frame = hover(base, top);

    let mut number = row![text(index + 1).size(12).font(BOLD)].spacing(6).align_y(Alignment::Center);
    if tile.rotation != 0 {
        number = number.push(container(text(format!("{}°", tile.rotation)).size(11)).padding([1, 6]).style(theme::badge));
    }
    if let Some(badge) = tile.badge {
        number = number.push(container(text(badge).size(11)).padding([1, 6]).style(theme::badge));
    }
    let mut label = row![].spacing(5).align_y(Alignment::Center);
    if let Some(stripe) = tile.stripe {
        label = label.push(container(space()).width(8).height(8).style(move |_| container::Style {
            background: Some(stripe.into()),
            border: iced::border::rounded(2),
            ..Default::default()
        }));
    }
    label = label.push(text(tile.label).size(12).color(theme::MUTED).wrapping(text::Wrapping::None));

    let cell = column![
        container(frame).height(Length::FillPortion(13)),
        column![number, container(label).clip(true)].spacing(2).height(36),
    ]
    .spacing(6);
    let shown = on(index, TileEvent::Shown);
    sensor(cell).key(tile.key).anticipate(300).on_show(move |_| shown.clone()).into()
}

/// What a tool will add to the page, drawn over a `w` × `h` preview.
fn overlay_view<'a, M: 'a>(overlay: &Overlay, w: f32, h: f32) -> Element<'a, M> {
    match overlay {
        Overlay::Text { text: content, x, y, align_x, align_y, size, color } => {
            let font = (size * w).max(1.0);
            // A box ending, centred on or starting at the anchor, as aligned.
            let (x, y) = (x * w, y * h);
            let half = x.min(w - x).max(0.0);
            let (left, width, horizontal) = match *align_x {
                a if a <= 0.25 => (x, (w - x).max(1.0), iced::alignment::Horizontal::Left),
                a if a >= 0.75 => (0.0, x.max(1.0), iced::alignment::Horizontal::Right),
                _ => (x - half, (2.0 * half).max(1.0), iced::alignment::Horizontal::Center),
            };
            let top = y - align_y * font * 0.75 - font * 0.12;
            let label = text(content.clone()).size(font).color(*color).wrapping(text::Wrapping::None);
            iced::widget::pin(container(label).width(width).align_x(horizontal).clip(false)).x(left).y(top).into()
        }
        Overlay::Image { handle, aspect, x, y, width, opacity } => {
            let iw = width * w;
            let ih = iw / aspect.max(0.01);
            iced::widget::pin(image(handle.clone()).width(iw).height(ih).opacity(*opacity)).x(x * w - iw / 2.0).y(y * h - ih / 2.0).into()
        }
    }
}

/// The pages of a document as a scrolling grid of thumbnails.
pub fn page_grid<'a, M: Clone + 'a>(
    tiles: Vec<Tile>,
    caps: Caps,
    busy: bool,
    empty: &'a str,
    on: impl Fn(usize, TileEvent) -> M + 'a,
) -> Element<'a, M> {
    if tiles.is_empty() {
        return container(center(muted(empty))).style(theme::grid_bg).into();
    }
    let count = tiles.len();
    let cells = tiles.into_iter().enumerate().map(|(i, tile)| tile_view(i, count, tile, caps, busy, &on));
    let grid = grid(cells).fluid(170).spacing(14).height(Sizing::AspectRatio(0.62));
    container(scrollable(container(grid).padding(14)).height(Length::Fill)).style(theme::grid_bg).into()
}
