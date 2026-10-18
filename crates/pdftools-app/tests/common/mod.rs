//! The test harness shared by the UI and acceptance tests: the real app,
//! driven through iced's headless simulator and by the messages its widgets
//! send, with scripted dialogs.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::rc::Rc;

use iced::{Settings, Size};
use iced_test::simulator::Simulator;
use pdftools_app::app::{App, Message, Screen};
use pdftools_app::catalog::ToolId;
use pdftools_app::dialogs::ScriptedDialogs;
use pdftools_app::theme;
use pdftools_app::widgets::{Run, TileEvent};

pub struct Harness {
    pub app: App,
    pub dialogs: Rc<ScriptedDialogs>,
    pub out: PathBuf,
}

/// A file in the repository. Bazel runs tests from the workspace's runfiles,
/// Cargo from the crate's directory.
pub fn repo(path: &str) -> PathBuf {
    if std::env::var_os("TEST_SRCDIR").is_some() {
        PathBuf::from(path)
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(path)
    }
}

/// Where tests save files for a person to look at: Bazel's test outputs
/// (bazel-testlogs/<test>/test.outputs), or `target/` with Cargo.
pub fn saved(path: &str) -> PathBuf {
    match std::env::var_os("TEST_UNDECLARED_OUTPUTS_DIR") {
        Some(dir) => PathBuf::from(dir).join(path),
        None => repo("target").join(path),
    }
}

pub fn fixture(name: &str) -> PathBuf {
    repo("tests/fixtures/pdfjs").join(name)
}

pub fn page_count(path: &Path) -> usize {
    pdftools_core::page_count(path).unwrap()
}

pub fn rotations(path: &Path) -> Vec<i64> {
    let doc = lopdf::Document::load(path).unwrap();
    doc.get_pages()
        .values()
        .map(|id| doc.get_dictionary(*id).unwrap().get(b"Rotate").and_then(lopdf::Object::as_i64).unwrap_or(0))
        .collect()
}

pub fn content(path: &Path, page: u32) -> String {
    let doc = lopdf::Document::load(path).unwrap();
    let id = doc.get_pages()[&page];
    String::from_utf8_lossy(&doc.get_page_content(id)).into_owned()
}

impl Harness {
    pub fn new(name: &str) -> Self {
        let dialogs = Rc::new(ScriptedDialogs::default());
        let app = App::new(dialogs.clone(), true);
        let out = std::env::temp_dir().join(format!("pdftools-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        Self { app, dialogs, out }
    }

    /// Delivers a message, then everything the background work produced.
    pub fn send(&mut self, message: Message) {
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

    pub fn simulator(&self) -> Simulator<'_, Message> {
        let settings = Settings { default_text_size: theme::FONT.into(), ..Settings::default() };
        Simulator::with_size(settings, Size::new(1180.0, 760.0), self.app.view())
    }

    /// Clicks the element showing `label`, as a user would.
    pub fn click(&mut self, label: &str) {
        let messages: Vec<Message> = {
            let mut simulator = self.simulator();
            simulator.click(label).unwrap_or_else(|e| panic!("no {label:?} to click: {e:?}"));
            simulator.into_messages().collect()
        };
        for message in messages {
            self.send(message);
        }
    }

    pub fn shows(&self, label: &str) -> bool {
        self.simulator().find(label).is_ok()
    }

    /// Renders previews (the grid asks for them as they scroll into view;
    /// the headless simulator doesn't scroll).
    pub fn render_pages(&mut self, tool: ToolId, pages: usize) {
        for index in 0..pages {
            self.send(Message::Tile(tool, index, TileEvent::Shown));
        }
    }

    pub fn screenshot(&self, name: &str) {
        let dir = saved("iced-shots");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.png"));
        // iced names the file after the renderer; remove the old one so a
        // fresh image is written rather than compared.
        let _ = std::fs::remove_file(dir.join(format!("{name}-tiny-skia.png")));
        let mut simulator = self.simulator();
        simulator.snapshot(&theme::theme()).unwrap().matches_image(&path).unwrap();
    }

    /// A screenshot at another window size, in `<dir>` of the saved files.
    pub fn screenshot_at(&self, dir: &str, name: &str, size: Size) {
        let dir = saved(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(dir.join(format!("{name}-tiny-skia.png")));
        let settings = Settings { default_text_size: theme::FONT.into(), ..Settings::default() };
        let mut simulator = Simulator::with_size(settings, size, self.app.view());
        simulator.snapshot(&theme::theme()).unwrap().matches_image(dir.join(format!("{name}.png"))).unwrap();
    }

    pub fn out(&self, name: &str) -> PathBuf {
        self.out.join(name)
    }

    pub fn option(&mut self, tool: ToolId, key: &'static str, value: &str) {
        self.send(Message::Option(tool, key, value.into()));
    }

    pub fn choose(&mut self, tool: ToolId, file: &Path) {
        self.send(Message::Navigate(Screen::Tool(tool)));
        self.dialogs.open(&[file]);
        self.send(Message::PickFile(tool));
    }

    /// Clicks the run button (by its label) and returns what the run bar says.
    pub fn run(&mut self, label: &str, save_as: Option<&Path>) -> String {
        if let Some(path) = save_as {
            self.dialogs.save(Some(path));
        }
        self.click(label);
        // A blocked run leaves its scripted answers unused; drop them so they
        // don't answer the next dialog.
        self.dialogs.saves.borrow_mut().clear();
        self.dialogs.files.borrow_mut().clear();
        self.status()
    }

    /// The run bar's state for the current screen.
    pub fn status(&self) -> String {
        let run = match self.app.screen {
            Screen::Tool(ToolId::Merge) => self.app.merge.run.clone(),
            Screen::Tool(ToolId::Images) => self.app.images.run.clone(),
            Screen::Tool(tool) => self.app.run_of(tool),
            _ => Run::Idle,
        };
        match run {
            Run::Done { message, .. } => message,
            Run::Failed(message) => format!("failed: {message}"),
            Run::Running(message) => format!("running: {message}"),
            Run::Idle => "idle".into(),
        }
    }
}
