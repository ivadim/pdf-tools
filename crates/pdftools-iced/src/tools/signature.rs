//! Signature: draw, type or upload a signature, then place it on pages.
//!
//! The drawing pad and the page stage are canvases: they turn mouse events
//! into option messages ("pad-down", "box", …) like any other control.

use std::path::PathBuf;

use iced::mouse::{self, Cursor};
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::widget::image::Handle;
use iced::widget::{center, column, container, image, pin, responsive, row, stack, text, text_input};
use iced::{Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme};
use pdftools_core::stamps::Placement;
use pdftools_render::raster::{load_picture, render_strokes, render_text, Family, Picture};

use super::set;
use crate::app::Message;
use crate::catalog::ToolId;
use crate::dialogs::Dialogs;
use crate::icons;
use crate::previews::Thumb;
use crate::single::{choose_output, file_name, plural, Job, Single, ToolOptions};
use crate::theme;
use crate::widgets::{action, field, icon_button, muted, section, segmented, Caps};

const INKS: [[u8; 3]; 4] = [[0x1d, 0x2c, 0x6b], [0x00, 0x00, 0x00], [0x2f, 0x5b, 0xd3], [0xc4, 0x3d, 0x3d]];
/// The page shown on the stage is rendered this wide.
pub const STAGE_WIDTH: u32 = 1000;
const T: ToolId = ToolId::Signature;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Draw,
    Type,
    Upload,
}

/// A placed signature, in fractions of its page (from the top left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SigBox {
    pub id: i32,
    pub page: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub struct Signature {
    source: Source,
    strokes: Vec<Vec<(f32, f32)>>,
    ink: [u8; 3],
    name: String,
    family: Family,
    upload: Option<(PathBuf, Vec<u8>, Picture)>,
    /// PNG bytes, pixels and image of the current signature.
    signature: Option<(Vec<u8>, Picture, Handle)>,
    pub boxes: Vec<SigBox>,
    next_box: i32,
    pub page: usize,
    pages: usize,
    /// Width / height of the page on the stage, once rendered.
    page_aspect: f32,
}

impl Default for Signature {
    fn default() -> Self {
        Self {
            source: Source::Draw,
            strokes: Vec::new(),
            ink: INKS[0],
            name: String::new(),
            family: Family::Script,
            upload: None,
            signature: None,
            boxes: Vec::new(),
            next_box: 1,
            page: 1,
            pages: 0,
            page_aspect: 1.0 / 1.414,
        }
    }
}

fn point(value: &str) -> Option<(f32, f32)> {
    let (x, y) = value.split_once(',')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

impl Signature {
    fn signature_aspect(&self) -> f32 {
        self.signature.as_ref().map_or(3.0, |(_, p, _)| p.aspect())
    }

    /// Box height (as a fraction of the page) for a width, keeping the
    /// signature's proportions on the page.
    fn box_height(&self, width: f32) -> f32 {
        width * self.page_aspect / self.signature_aspect()
    }

    /// Re-creates the signature from the current source.
    fn rebuild(&mut self) {
        let picture = match self.source {
            Source::Draw => render_strokes(&self.strokes, self.ink, 3.0, 3.0),
            Source::Type => render_text(&self.name, self.family, false, self.ink, 140.0),
            Source::Upload => self.upload.as_ref().map(|(_, _, p)| p.clone()),
        };
        self.signature = picture.map(|p| {
            let bytes = match (&self.source, &self.upload) {
                (Source::Upload, Some((_, bytes, _))) => bytes.clone(),
                _ => p.to_png(),
            };
            let handle = Handle::from_rgba(p.width, p.height, p.pixels.clone());
            (bytes, p, handle)
        });
        // Keep placed signatures in proportion with the new image.
        for i in 0..self.boxes.len() {
            self.boxes[i].height = self.box_height(self.boxes[i].width).min(1.0);
        }
    }

    fn add_boxes(&mut self, every: bool) {
        if self.signature.is_none() || self.pages == 0 {
            return;
        }
        let pages: Vec<usize> = if every { (1..=self.pages).collect() } else { vec![self.page] };
        for page in pages {
            let width = 0.28;
            let height = self.box_height(width);
            self.boxes.push(SigBox { id: self.next_box, page, x: 1.0 - width - 0.07, y: (1.0 - height - 0.08).max(0.0), width, height });
            self.next_box += 1;
        }
    }

    fn move_box(&mut self, id: i32, x: f32, y: f32, width: f32) {
        let height = self.box_height(width.clamp(0.05, 1.0));
        if let Some(b) = self.boxes.iter_mut().find(|b| b.id == id) {
            b.width = width.clamp(0.05, 1.0);
            b.height = height.min(1.0);
            b.x = x.clamp(0.0, 1.0 - b.width);
            b.y = y.clamp(0.0, 1.0 - b.height);
        }
    }
}

impl ToolOptions for Signature {
    fn id(&self) -> ToolId {
        T
    }

    fn view<'a>(&'a self, file: &'a Single, _closeup: Option<&'a Thumb>) -> Element<'a, Message> {
        let source = match self.source {
            Source::Draw => "draw",
            Source::Type => "type",
            Source::Upload => "upload",
        };
        let ink = Color::from_rgb8(self.ink[0], self.ink[1], self.ink[2]);
        let input: Element<'a, Message> = match self.source {
            Source::Draw => column![
                container(Canvas::new(Pad { strokes: &self.strokes, ink }).width(Length::Fill).height(150))
                    .style(theme::paper),
                row![muted("Draw with the mouse or a pen."), iced::widget::space::horizontal(), action("Clear", None, false, Some(Message::Option(T, "pad-clear", String::new())))]
                    .align_y(iced::Alignment::Center),
            ]
            .spacing(6)
            .into(),
            Source::Type => {
                let family = match self.family {
                    Family::Script => "script",
                    Family::Serif => "serif",
                    Family::Sans => "sans",
                    Family::Mono => "mono",
                };
                column![
                    field("Your name", text_input("Your name", &self.name).on_input(set(T, "name")).padding(8).style(theme::input)),
                    segmented(&[("script", "Handwriting"), ("serif", "Serif"), ("sans", "Sans")], family, |v| Message::Option(T, "font", v.into())),
                ]
                .spacing(10)
                .into()
            }
            Source::Upload => {
                let name = self.upload.as_ref().map(|(p, _, _)| file_name(p));
                column![
                    action(name.unwrap_or_else(|| "Choose image…".into()), Some(icons::image()), false, Some(Message::Option(T, "choose-image", String::new()))),
                    muted("A PNG with a transparent background works best."),
                ]
                .spacing(6)
                .into()
            }
        };
        let inks = row(INKS.iter().enumerate().map(|(i, c)| {
            let selected = *c == self.ink;
            let fill = Color::from_rgb8(c[0], c[1], c[2]);
            iced::widget::button(iced::widget::space())
                .width(24)
                .height(24)
                .style(move |_, _| iced::widget::button::Style {
                    background: Some(fill.into()),
                    border: iced::Border {
                        width: if selected { 3.0 } else { 1.0 },
                        color: if selected { theme::c().accent } else { theme::c().border },
                        radius: 12.0.into(),
                    },
                    ..Default::default()
                })
                .on_press(Message::Option(T, "ink", i.to_string()))
                .into()
        }))
        .spacing(6);
        let mut signature = column![
            segmented(&[("draw", "Draw"), ("type", "Type"), ("upload", "Image")], source, |v| Message::Option(T, "source", v.into())),
            input,
        ]
        .spacing(10);
        // Typed and uploaded signatures are shown as they'll be placed.
        if self.source != Source::Draw {
            if let Some((_, _, handle)) = &self.signature {
                signature = signature.push(
                    container(image(handle.clone()).height(64))
                        .width(Length::Fill)
                        .center_x(Length::Fill)
                        .padding(10)
                        .style(theme::paper),
                );
            }
        }
        let mut content = column![section("Signature", [signature.into()])].spacing(18);
        if self.source != Source::Upload {
            content = content.push(field("Ink", inks));
        }
        let ready = self.signature.is_some() && file.loaded().is_some();
        content = content.push(section(
            "Place it",
            [
                row![
                    action("Add to this page", Some(icons::plus()), true, ready.then(|| Message::Option(T, "add", String::new()))),
                    action(
                        if self.pages > 1 { format!("Add to all {} pages", self.pages) } else { "Add to every page".into() },
                        None,
                        false,
                        (ready && self.pages > 1).then(|| Message::Option(T, "add-every", String::new())),
                    ),
                ]
                .spacing(8)
                .into(),
                muted("Drag a signature to move it, drag its corner to resize, or click × to remove it."),
            ],
        ));
        content.into()
    }

    fn set(&mut self, key: &str, value: String, _file: &Single, dialogs: &dyn Dialogs) {
        match key {
            "source" => {
                self.source = match value.as_str() {
                    "type" => Source::Type,
                    "upload" => Source::Upload,
                    _ => Source::Draw,
                }
            }
            "pad-down" => {
                if let Some(p) = point(&value) {
                    self.strokes.push(vec![p]);
                }
                return;
            }
            "pad-move" => {
                if let (Some(p), Some(stroke)) = (point(&value), self.strokes.last_mut()) {
                    stroke.push(p);
                }
                return;
            }
            "pad-up" => {}
            "pad-clear" => self.strokes.clear(),
            "name" => self.name = value,
            "font" => {
                self.family = match value.as_str() {
                    "serif" => Family::Serif,
                    "sans" => Family::Sans,
                    _ => Family::Script,
                }
            }
            "ink" => self.ink = INKS[value.parse::<usize>().unwrap_or(0).min(INKS.len() - 1)],
            "choose-image" => {
                let Some(path) = dialogs.pick_image() else { return };
                self.upload = load_picture(&path).ok().map(|(bytes, picture)| (path, bytes, picture));
            }
            "add" | "add-every" => {
                self.add_boxes(key == "add-every");
                return;
            }
            "box" => {
                let v: Vec<f32> = value.split(',').filter_map(|n| n.parse().ok()).collect();
                if let [id, x, y, width] = v[..] {
                    self.move_box(id as i32, x, y, width);
                }
                return;
            }
            "remove" => {
                let id: i32 = value.parse().unwrap_or(-1);
                self.boxes.retain(|b| b.id != id);
                return;
            }
            "page" => {
                let target = self.page as i64 + value.parse::<i64>().unwrap_or(0);
                if target >= 1 && target as usize <= self.pages {
                    self.page = target as usize;
                }
                return;
            }
            "page-aspect" => {
                self.page_aspect = value.parse().unwrap_or(self.page_aspect);
                return;
            }
            _ => return,
        }
        self.rebuild();
    }

    fn caps(&self) -> Caps {
        Caps::default()
    }

    fn file_changed(&mut self, file: &Single) {
        self.boxes.clear();
        self.page = 1;
        self.pages = file.page_count();
    }

    fn stage_page(&self) -> Option<usize> {
        (self.pages > 0).then_some(self.page)
    }

    fn stage<'a>(&'a self, file: &'a Single, page: Option<&'a Thumb>) -> Element<'a, Message> {
        let on_page = self.boxes.iter().filter(|b| b.page == self.page).count();
        let label = if on_page > 0 {
            format!("Page {} of {} · {}", self.page, self.pages, plural(on_page, "signature"))
        } else {
            format!("Page {} of {}", self.page, self.pages)
        };
        let header = row![
            icon_button(icons::chevron_left(), "Previous page", 30.0, (self.page > 1).then(|| Message::Option(T, "page", "-1".into()))),
            center(text(label).color(theme::c().muted)).height(30),
            icon_button(icons::chevron_right(), "Next page", 30.0, (self.page < self.pages).then(|| Message::Option(T, "page", "1".into()))),
        ]
        .padding([8, 10])
        .align_y(iced::Alignment::Center);
        let Some(page) = page else {
            return column![header, center(muted("Rendering the page…"))].into();
        };
        let _ = file;
        let boxes: Vec<SigBox> = self.boxes.iter().copied().filter(|b| b.page == self.page).collect();
        let signature = self.signature.as_ref().map(|(_, _, h)| h.clone());
        let handle = page.handle.clone();
        let aspect = page.aspect;
        let stage = responsive(move |size| {
            let (w, h) = if aspect > size.width / size.height.max(1.0) {
                (size.width, size.width / aspect)
            } else {
                (size.height * aspect, size.height)
            };
            let mut layers = stack![image(handle.clone()).width(w).height(h)].width(w).height(h);
            if let Some(sig) = &signature {
                for b in &boxes {
                    layers = layers.push(pin(image(sig.clone()).width(b.width * w).height(b.height * h)).x(b.x * w).y(b.y * h));
                }
            }
            layers = layers.push(Canvas::new(Stage { boxes: boxes.clone() }).width(w).height(h));
            center(container(layers).style(theme::page)).into()
        });
        column![header, container(stage).padding(16).height(Length::Fill)].into()
    }

    fn validate(&self, _file: &Single) -> Option<String> {
        if self.signature.is_none() {
            return Some(
                match self.source {
                    Source::Draw => "Draw your signature.",
                    Source::Type => "Type your name.",
                    Source::Upload => "Choose a signature image.",
                }
                .into(),
            );
        }
        self.boxes.is_empty().then(|| "Add the signature to a page.".into())
    }

    fn run_label(&self) -> &'static str {
        "Sign PDF…"
    }

    fn start(&mut self, file: &Single, dialogs: &dyn Dialogs) -> Option<Job> {
        let input = file.path()?.to_path_buf();
        let (bytes, _, _) = self.signature.clone()?;
        let output = choose_output(dialogs, file, "signed")?;
        let placements: Vec<Placement> =
            self.boxes.iter().map(|b| Placement { page: b.page, x: b.x, y: b.y, width: b.width, height: b.height }).collect();
        let mut pages: Vec<usize> = placements.iter().map(|p| p.page).collect();
        pages.sort_unstable();
        pages.dedup();
        let count = pages.len();
        Some(Box::new(move || {
            pdftools_core::stamps::add_signature_file(&input, &bytes, &placements, &output).map_err(|e| e.to_string())?;
            Ok((format!("Signed {} in", plural(count, "page")), Some(output)))
        }))
    }
}

// ── Drawing pad ──────────────────────────────────────────────────────────

struct Pad<'a> {
    strokes: &'a [Vec<(f32, f32)>],
    ink: Color,
}

impl canvas::Program<Message> for Pad<'_> {
    /// Whether the pen is down.
    type State = bool;

    fn update(&self, drawing: &mut bool, event: &Event, bounds: Rectangle, cursor: Cursor) -> Option<canvas::Action<Message>> {
        let at = |p: Point| format!("{},{}", p.x, p.y);
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = cursor.position_in(bounds)?;
                *drawing = true;
                Some(canvas::Action::publish(Message::Option(T, "pad-down", at(p))).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if *drawing => {
                let p = cursor.position_in(bounds)?;
                Some(canvas::Action::publish(Message::Option(T, "pad-move", at(p))).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if *drawing => {
                *drawing = false;
                Some(canvas::Action::publish(Message::Option(T, "pad-up", String::new())))
            }
            _ => None,
        }
    }

    fn draw(&self, _: &bool, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let baseline = bounds.height * 0.75;
        frame.stroke(
            &Path::line(Point::new(16.0, baseline), Point::new(bounds.width - 16.0, baseline)),
            Stroke::default().with_color(theme::LIGHT.border).with_width(1.0),
        );
        for stroke in self.strokes {
            let path = Path::new(|p| {
                if let Some((x, y)) = stroke.first() {
                    p.move_to(Point::new(*x, *y));
                    for (x, y) in &stroke[1..] {
                        p.line_to(Point::new(*x, *y));
                    }
                    if stroke.len() == 1 {
                        p.line_to(Point::new(x + 0.1, *y));
                    }
                }
            });
            frame.stroke(&path, Stroke::default().with_color(self.ink).with_width(3.0).with_line_cap(canvas::LineCap::Round).with_line_join(canvas::LineJoin::Round));
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, _: &bool, bounds: Rectangle, cursor: Cursor) -> mouse::Interaction {
        if cursor.is_over(bounds) { mouse::Interaction::Crosshair } else { mouse::Interaction::default() }
    }
}

// ── Page stage ───────────────────────────────────────────────────────────

/// Size of the resize handle and the remove button, in pixels.
const HANDLE: f32 = 14.0;

#[derive(Debug, Clone, Copy)]
enum Grab {
    Move,
    Resize,
}

#[derive(Default)]
struct Dragging(Option<(i32, Grab, Point, SigBox)>);

struct Stage {
    boxes: Vec<SigBox>,
}

impl Stage {
    fn rect(b: &SigBox, size: Size) -> Rectangle {
        Rectangle::new(Point::new(b.x * size.width, b.y * size.height), Size::new(b.width * size.width, b.height * size.height))
    }

    fn remove_button(r: Rectangle) -> Rectangle {
        Rectangle::new(Point::new(r.x + r.width - HANDLE / 2.0, r.y - HANDLE / 2.0), Size::new(HANDLE, HANDLE))
    }

    fn resize_handle(r: Rectangle) -> Rectangle {
        Rectangle::new(Point::new(r.x + r.width - HANDLE / 2.0, r.y + r.height - HANDLE / 2.0), Size::new(HANDLE, HANDLE))
    }
}

impl canvas::Program<Message> for Stage {
    type State = Dragging;

    fn update(&self, drag: &mut Dragging, event: &Event, bounds: Rectangle, cursor: Cursor) -> Option<canvas::Action<Message>> {
        let size = bounds.size();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = cursor.position_in(bounds)?;
                // Topmost box first.
                for b in self.boxes.iter().rev() {
                    let r = Self::rect(b, size);
                    if Self::remove_button(r).contains(p) {
                        return Some(canvas::Action::publish(Message::Option(T, "remove", b.id.to_string())).and_capture());
                    }
                    let grab = if Self::resize_handle(r).contains(p) {
                        Grab::Resize
                    } else if r.contains(p) {
                        Grab::Move
                    } else {
                        continue;
                    };
                    drag.0 = Some((b.id, grab, p, *b));
                    return Some(canvas::Action::capture());
                }
                None
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let (id, grab, start, b) = drag.0?;
                let p = cursor.position_in(bounds).unwrap_or(start);
                let (dx, dy) = ((p.x - start.x) / size.width, (p.y - start.y) / size.height);
                let (x, y, width) = match grab {
                    Grab::Move => (b.x + dx, b.y + dy, b.width),
                    Grab::Resize => (b.x, b.y, b.width + dx),
                };
                Some(canvas::Action::publish(Message::Option(T, "box", format!("{id},{x},{y},{width}"))).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if drag.0.is_some() => {
                drag.0 = None;
                Some(canvas::Action::capture())
            }
            _ => None,
        }
    }

    fn draw(&self, _: &Dragging, renderer: &Renderer, _: &Theme, bounds: Rectangle, _: Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        for b in &self.boxes {
            let r = Self::rect(b, bounds.size());
            frame.stroke(
                &Path::rectangle(r.position(), r.size()),
                Stroke::default().with_color(theme::c().accent).with_width(1.5),
            );
            let handle = Self::resize_handle(r);
            frame.fill(&Path::rectangle(handle.position(), handle.size()), theme::c().accent);
            let remove = Self::remove_button(r);
            frame.fill(&Path::circle(remove.center(), HANDLE / 2.0), theme::c().error);
            let c = remove.center();
            let arm = HANDLE / 4.5;
            let cross = Path::new(|p| {
                p.move_to(Point::new(c.x - arm, c.y - arm));
                p.line_to(Point::new(c.x + arm, c.y + arm));
                p.move_to(Point::new(c.x + arm, c.y - arm));
                p.line_to(Point::new(c.x - arm, c.y + arm));
            });
            frame.stroke(&cross, Stroke::default().with_color(Color::WHITE).with_width(1.6));
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(&self, drag: &Dragging, bounds: Rectangle, cursor: Cursor) -> mouse::Interaction {
        if let Some((_, grab, _, _)) = drag.0 {
            return match grab {
                Grab::Move => mouse::Interaction::Grabbing,
                Grab::Resize => mouse::Interaction::ResizingDiagonallyDown,
            };
        }
        let Some(p) = cursor.position_in(bounds) else { return mouse::Interaction::default() };
        for b in self.boxes.iter().rev() {
            let r = Self::rect(b, bounds.size());
            if Self::remove_button(r).contains(p) {
                return mouse::Interaction::Pointer;
            }
            if Self::resize_handle(r).contains(p) {
                return mouse::Interaction::ResizingDiagonallyDown;
            }
            if r.contains(p) {
                return mouse::Interaction::Grab;
            }
        }
        mouse::Interaction::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialogs::ScriptedDialogs;

    #[test]
    fn draws_places_and_moves_signatures() {
        let mut tool = Signature { pages: 3, ..Default::default() };
        let (file, dialogs) = (Single::default(), ScriptedDialogs::default());
        assert_eq!(tool.validate(&file).as_deref(), Some("Draw your signature."));
        tool.set("pad-down", "10,10".into(), &file, &dialogs);
        tool.set("pad-move", "80,30".into(), &file, &dialogs);
        tool.set("pad-up", String::new(), &file, &dialogs);
        assert!(tool.signature.is_some());
        assert_eq!(tool.validate(&file).as_deref(), Some("Add the signature to a page."));
        tool.set("add-every", String::new(), &file, &dialogs);
        assert_eq!(tool.boxes.len(), 3);
        let id = tool.boxes[0].id;
        tool.set("box", format!("{id},0.9,0.9,0.5"), &file, &dialogs);
        let b = tool.boxes[0];
        assert!((b.width - 0.5).abs() < 1e-6 && b.x + b.width <= 1.0 && b.y + b.height <= 1.0, "kept on the page: {b:?}");
        tool.set("remove", id.to_string(), &file, &dialogs);
        assert_eq!(tool.boxes.len(), 2);
        tool.set("page", "1".into(), &file, &dialogs);
        tool.set("page", "5".into(), &file, &dialogs);
        assert_eq!(tool.page, 2);
        tool.set("source", "type".into(), &file, &dialogs);
        assert!(tool.signature.is_none(), "no name typed yet");
        tool.set("name", "Ada Lovelace".into(), &file, &dialogs);
        assert!(tool.signature.is_some());
    }
}
