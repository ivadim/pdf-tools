//! End-to-end tests of the iced app: the real `update` and `view`, driven
//! through iced's headless simulator (clicking and typing by visible text),
//! with scripted dialogs. Output files are checked with the PDF engine.
//! Screenshots of key screens are written to target/iced-shots.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use iced::{Settings, Size};
use iced_test::simulator::Simulator;
use pdftools_iced::app::{App, Message, Screen};
use pdftools_iced::catalog::ToolId;
use pdftools_iced::dialogs::ScriptedDialogs;
use pdftools_iced::theme;
use pdftools_iced::widgets::TileEvent;

struct Harness {
    app: App,
    dialogs: Rc<ScriptedDialogs>,
    out: PathBuf,
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdfjs").join(name)
}

impl Harness {
    fn new(name: &str) -> Self {
        let dialogs = Rc::new(ScriptedDialogs::default());
        let app = App::new(dialogs.clone(), true);
        let out = std::env::temp_dir().join(format!("pdftools-iced-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        Self { app, dialogs, out }
    }

    /// Delivers a message, then everything the background work produced.
    fn send(&mut self, message: Message) {
        let _ = self.app.update(message);
        loop {
            let queued = std::mem::take(&mut self.app.queued);
            if queued.is_empty() {
                break;
            }
            for message in queued {
                let _ = self.app.update(message);
            }
        }
    }

    fn simulator(&self) -> Simulator<'_, Message> {
        let settings = Settings { default_text_size: theme::FONT.into(), ..Settings::default() };
        Simulator::with_size(settings, Size::new(1180.0, 760.0), self.app.view())
    }

    /// Clicks the element showing `label`, as a user would.
    fn click(&mut self, label: &str) {
        let messages: Vec<Message> = {
            let mut simulator = self.simulator();
            simulator.click(label).unwrap_or_else(|e| panic!("no {label:?} to click: {e:?}"));
            simulator.into_messages().collect()
        };
        for message in messages {
            self.send(message);
        }
    }

    fn shows(&self, label: &str) -> bool {
        self.simulator().find(label).is_ok()
    }

    /// Renders every page's preview (the grid asks for them as they scroll
    /// into view; the headless simulator doesn't scroll).
    fn render_pages(&mut self, tool: ToolId, pages: usize) {
        for index in 0..pages {
            self.send(Message::Tile(tool, index, TileEvent::Shown));
        }
    }

    fn screenshot(&self, name: &str) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/iced-shots");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.png"));
        // iced names the file after the renderer; remove the old one so a
        // fresh image is written rather than compared.
        let _ = std::fs::remove_file(dir.join(format!("{name}-tiny-skia.png")));
        let mut simulator = self.simulator();
        simulator.snapshot(&theme::theme()).unwrap().matches_image(&path).unwrap();
    }

    fn out(&self, name: &str) -> PathBuf {
        self.out.join(name)
    }
}

#[test]
fn screens_render() {
    let mut h = Harness::new("screens");
    h.screenshot("home");
    assert!(h.shows("FULL TOOL CATALOG"));

    h.click("About");
    assert_eq!(h.app.screen, Screen::About);
    h.screenshot("about");

    h.click("Compress");
    h.dialogs.open(&[&fixture("tracemonkey.pdf")]);
    h.click("Choose a PDF");
    h.render_pages(ToolId::Compress, 14);
    h.screenshot("compress");
    let _ = h.out("unused");
}
