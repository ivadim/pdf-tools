//! Select areas on a page and permanently remove them from a flattened copy.

use iced::mouse::{self, Cursor};
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path};
use iced::widget::{center, checkbox, column, container, image, responsive, row, stack, text, text_input};
use iced::{Color, Element, Event, Length, Point, Rectangle, Renderer, Theme};
use pdftools_render::redact::{redact_with_quality, RedactQuality, Redaction};

use crate::app::Message;
use crate::catalog::ToolId;
use crate::dialogs::Dialogs;
use crate::icons;
use crate::previews::Thumb;
use crate::single::{choose_output, plural, Job, Single, ToolOptions};
use crate::theme;
use crate::widgets::{action, field, icon_button, muted, section, Caps};

const T: ToolId = ToolId::Redact;

#[derive(Default)]
pub struct Redact {
    marks: Vec<Redaction>,
    page: usize,
    pages: usize,
    coordinates: [String; 4],
    quality: RedactQuality,
}

impl ToolOptions for Redact {
    fn id(&self) -> ToolId {
        T
    }

    fn view<'a>(&'a self, _file: &'a Single, _closeup: Option<&'a Thumb>) -> Element<'a, Message> {
        column![section(
            "Mark areas",
            [
                muted("Drag across the page to mark each area. Review all marks before creating the copy."),
                text("The exported PDF converts every page to an image. Text will not be selectable; links, forms and document details are removed.").into(),
                checkbox(self.quality == RedactQuality::Smaller).label("Smaller file").on_toggle(|on| Message::Option(T, "smaller", if on { "1" } else { "0" }.into())).into(),
                muted("Smaller file lowers resolution and may use JPEG; text may be less sharp."),
                muted(format!("{} marked", plural(self.marks.len(), "area"))),
                action("Undo last mark", None, false, (!self.marks.is_empty()).then(|| Message::Option(T, "undo", String::new()))),
                action("Clear all marks", None, false, (!self.marks.is_empty()).then(|| Message::Option(T, "clear", String::new()))),
                muted("Or enter left, top, width and height as percentages of the page:"),
                row![
                    field("Left (%)", text_input("0", &self.coordinates[0]).on_input(|v| Message::Option(T, "left", v)).width(85)),
                    field("Top (%)", text_input("0", &self.coordinates[1]).on_input(|v| Message::Option(T, "top", v)).width(85)),
                ]
                .spacing(6)
                .into(),
                row![
                    field("Width (%)", text_input("10", &self.coordinates[2]).on_input(|v| Message::Option(T, "width", v)).width(85)),
                    field("Height (%)", text_input("10", &self.coordinates[3]).on_input(|v| Message::Option(T, "height", v)).width(85)),
                ]
                .spacing(6)
                .into(),
                action("Add area", None, false, self.coordinate_area().map(|_| Message::Option(T, "add-coordinates", String::new()))),
            ]
        )]
        .into()
    }

    fn set(&mut self, key: &str, value: String, _file: &Single, _dialogs: &dyn Dialogs) {
        match key {
            "page" => {
                let n = self.page as i64 + value.parse::<i64>().unwrap_or(0);
                if n >= 1 && n as usize <= self.pages {
                    self.page = n as usize;
                }
            }
            "undo" => {
                self.marks.pop();
            }
            "clear" => self.marks.clear(),
            "smaller" => self.quality = if value == "1" { RedactQuality::Smaller } else { RedactQuality::Higher },
            "left" | "top" | "width" | "height" => {
                let i = match key {
                    "left" => 0,
                    "top" => 1,
                    "width" => 2,
                    _ => 3,
                };
                self.coordinates[i] = value;
            }
            "add-coordinates" => {
                if let Some(rect) = self.coordinate_area() {
                    self.marks.push(rect);
                }
            }
            "area" => {
                let v: Vec<f32> = value.split(',').filter_map(|s| s.parse().ok()).collect();
                if let [x, y, width, height] = v[..] {
                    if width > 0.002 && height > 0.002 {
                        self.marks.push(Redaction { page: self.page, x, y, width, height });
                    }
                }
            }
            _ => {}
        }
    }
    fn caps(&self) -> Caps {
        Caps::default()
    }
    fn file_changed(&mut self, file: &Single) {
        self.marks.clear();
        self.page = 1;
        self.pages = file.page_count();
    }
    fn stage_page(&self) -> Option<usize> {
        (self.pages > 0).then_some(self.page)
    }
    fn stage<'a>(&'a self, _file: &'a Single, page: Option<&'a Thumb>) -> Element<'a, Message> {
        let label =
            format!("Page {} of {} · {}", self.page, self.pages, plural(self.marks.iter().filter(|m| m.page == self.page).count(), "area"));
        let header = row![
            icon_button(icons::chevron_left(), "Previous page", 30.0, (self.page > 1).then(|| Message::Option(T, "page", "-1".into()))),
            center(text(label).color(theme::c().muted)).height(30),
            icon_button(
                icons::chevron_right(),
                "Next page",
                30.0,
                (self.page < self.pages).then(|| Message::Option(T, "page", "1".into()))
            ),
        ]
        .padding([8, 10])
        .align_y(iced::Alignment::Center);
        let Some(page) = page else { return column![header, center(muted("Rendering the page…"))].into() };
        let handle = page.handle.clone();
        let aspect = page.aspect;
        let marks: Vec<Redaction> = self.marks.iter().copied().filter(|m| m.page == self.page).collect();
        let stage = responsive(move |size| {
            let (w, h) = if aspect > size.width / size.height.max(1.0) {
                (size.width, size.width / aspect)
            } else {
                (size.height * aspect, size.height)
            };
            let layers = stack![image(handle.clone()).width(w).height(h), Canvas::new(Stage { marks: marks.clone() }).width(w).height(h)]
                .width(w)
                .height(h);
            center(container(layers).style(theme::page)).into()
        });
        column![header, container(stage).padding(16).height(Length::Fill)].into()
    }
    fn validate(&self, _file: &Single) -> Option<String> {
        self.marks.is_empty().then(|| "Mark an area to remove.".into())
    }
    fn run_label(&self) -> &'static str {
        "Redact"
    }
    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let input = file.path()?.to_path_buf();
        let output = choose_output(dialogs, file, "redacted")?;
        let marks = self.marks.clone();
        let quality = self.quality;
        Some(Box::new(move || {
            let pages = redact_with_quality(&input, &output, &marks, quality)?;
            Ok((format!("Created a redacted copy of {} in", plural(pages, "page")), Some(output)))
        }))
    }
}

impl Redact {
    fn coordinate_area(&self) -> Option<Redaction> {
        if self.pages == 0 {
            return None;
        }
        let values: Vec<f32> = self.coordinates.iter().map(|s| s.parse::<f32>().ok().map(|v| v / 100.0)).collect::<Option<_>>()?;
        let [x, y, width, height] = values[..] else { return None };
        if ![x, y, width, height].iter().all(|v| v.is_finite())
            || x < 0.0
            || y < 0.0
            || width <= 0.0
            || height <= 0.0
            || x + width > 1.0
            || y + height > 1.0
        {
            return None;
        }
        Some(Redaction { page: self.page, x, y, width, height })
    }
}

#[derive(Default)]
struct Drag {
    start: Option<Point>,
    current: Option<Point>,
}
struct Stage {
    marks: Vec<Redaction>,
}

impl canvas::Program<Message> for Stage {
    type State = Drag;
    fn update(&self, drag: &mut Drag, event: &Event, bounds: Rectangle, cursor: Cursor) -> Option<canvas::Action<Message>> {
        let point =
            || cursor.position().map(|p| Point::new((p.x - bounds.x).clamp(0.0, bounds.width), (p.y - bounds.y).clamp(0.0, bounds.height)));
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = cursor.position_in(bounds)?;
                drag.start = Some(p);
                drag.current = Some(p);
                Some(canvas::Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if drag.start.is_some() => {
                drag.current = point();
                Some(canvas::Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if drag.start.is_some() => {
                let start = drag.start.take()?;
                let end = point().or(drag.current.take())?;
                drag.current = None;
                let (x, y) = (start.x.min(end.x) / bounds.width, start.y.min(end.y) / bounds.height);
                let (w, h) = ((start.x - end.x).abs() / bounds.width, (start.y - end.y).abs() / bounds.height);
                Some(canvas::Action::publish(Message::Option(T, "area", format!("{x},{y},{w},{h}"))).and_capture())
            }
            _ => None,
        }
    }
    fn draw(&self, drag: &Drag, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        for m in &self.marks {
            frame.fill(
                &Path::rectangle(
                    Point::new(m.x * bounds.width, m.y * bounds.height),
                    iced::Size::new(m.width * bounds.width, m.height * bounds.height),
                ),
                Color::BLACK,
            );
        }
        if let (Some(a), Some(b)) = (drag.start, drag.current) {
            frame.fill(
                &Path::rectangle(Point::new(a.x.min(b.x), a.y.min(b.y)), iced::Size::new((a.x - b.x).abs(), (a.y - b.y).abs())),
                Color::from_rgba(0.0, 0.0, 0.0, 0.65),
            );
        }
        vec![frame.into_geometry()]
    }
    fn mouse_interaction(&self, _: &Drag, bounds: Rectangle, cursor: Cursor) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}
