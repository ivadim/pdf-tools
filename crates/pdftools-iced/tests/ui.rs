//! End-to-end tests of the iced app: the real `update` and `view`, driven
//! through iced's headless simulator (clicking by visible text) and by the
//! messages the widgets send, with scripted dialogs. Output files are
//! checked with the PDF engine. Screenshots of key screens are written to
//! target/iced-shots.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use iced::{Settings, Size};
use iced_test::simulator::Simulator;
use pdftools_iced::app::{App, Message, Screen};
use pdftools_iced::catalog::ToolId;
use pdftools_iced::dialogs::ScriptedDialogs;
use pdftools_iced::theme;
use pdftools_iced::widgets::{Run, TileEvent};
use pdftools_iced::{images, merge};

struct Harness {
    app: App,
    dialogs: Rc<ScriptedDialogs>,
    out: PathBuf,
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdfjs").join(name)
}

fn page_count(path: &Path) -> usize {
    pdftools_core::page_count(path).unwrap()
}

fn rotations(path: &Path) -> Vec<i64> {
    let doc = lopdf::Document::load(path).unwrap();
    doc.get_pages()
        .values()
        .map(|id| doc.get_dictionary(*id).unwrap().get(b"Rotate").and_then(lopdf::Object::as_i64).unwrap_or(0))
        .collect()
}

fn content(path: &Path, page: u32) -> String {
    let doc = lopdf::Document::load(path).unwrap();
    let id = doc.get_pages()[&page];
    String::from_utf8_lossy(&doc.get_page_content(id)).into_owned()
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

    /// Renders previews (the grid asks for them as they scroll into view;
    /// the headless simulator doesn't scroll).
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

    fn option(&mut self, tool: ToolId, key: &'static str, value: &str) {
        self.send(Message::Option(tool, key, value.into()));
    }

    fn choose(&mut self, tool: ToolId, file: &Path) {
        self.send(Message::Navigate(Screen::Tool(tool)));
        self.dialogs.open(&[file]);
        self.send(Message::PickFile(tool));
    }

    /// Clicks the run button (by its label) and returns what the run bar says.
    fn run(&mut self, label: &str, save_as: Option<&Path>) -> String {
        if let Some(path) = save_as {
            self.dialogs.save(Some(path));
        }
        self.click(label);
        self.status()
    }

    /// The run bar's state for the current screen.
    fn status(&self) -> String {
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

#[test]
fn screens_render() {
    let mut h = Harness::new("screens");
    h.screenshot("home");
    assert!(h.shows("FULL TOOL CATALOG"));

    h.click("About");
    assert_eq!(h.app.screen, Screen::About);
    h.click("GitHub");
    assert_eq!(*h.dialogs.opened_urls.borrow(), ["https://github.com/ivadim/pdf-tools"]);
    h.screenshot("about");

    h.click("Compress");
    h.dialogs.open(&[&fixture("tracemonkey.pdf")]);
    h.click("Choose a PDF");
    h.render_pages(ToolId::Compress, 14);
    h.screenshot("compress");
    let out = h.out("small.pdf");
    assert!(h.run("Compress PDF…", Some(&out)).contains("Saved as"));
    assert_eq!(page_count(&out), 14);
}

#[test]
fn single_file_tools() {
    let mut h = Harness::new("single");

    // Split by ranges into a folder; extract selected pages.
    h.choose(ToolId::Split, &fixture("tracemonkey.pdf"));
    h.render_pages(ToolId::Split, 14);
    h.screenshot("split");
    assert!(h.shows("File 1") && h.shows("File 2"));
    h.option(ToolId::Split, "ranges", "1-3, 40");
    assert!(h.shows("“40” is outside 1–14"));
    h.option(ToolId::Split, "ranges", "1-2, 3");
    let folder = h.out("parts");
    std::fs::create_dir_all(&folder).unwrap();
    h.dialogs.open(&[&folder]);
    assert!(h.run("Split PDF…", None).contains("Created 2 files"));
    assert_eq!(page_count(&folder.join("tracemonkey-1.pdf")), 2);
    h.option(ToolId::Split, "mode", "extract");
    assert!(h.shows("Select the pages to extract."));
    h.send(Message::Tile(ToolId::Split, 1, TileEvent::View));
    h.send(Message::Tile(ToolId::Split, 3, TileEvent::View));
    let out = h.out("extract.pdf");
    assert!(h.run("Split PDF…", Some(&out)).contains("Extracted 2 pages"));
    assert_eq!(page_count(&out), 2);

    // Rotate pages and save; the viewer rotates too.
    h.choose(ToolId::Rotate, &fixture("sizes.pdf"));
    assert!(h.shows("Rotate at least one page — use the buttons on each page or above."));
    h.send(Message::Tile(ToolId::Rotate, 0, TileEvent::RotateRight));
    h.send(Message::Tile(ToolId::Rotate, 1, TileEvent::View));
    h.send(Message::ViewerRotate(-90));
    h.render_pages(ToolId::Rotate, 3);
    h.screenshot("rotate-viewer");
    assert!(h.shows("Page 2 of 3 · rotated 270°"));
    h.send(Message::CloseViewer);
    h.send(Message::Tile(ToolId::Rotate, 2, TileEvent::ToggleInclude));
    let out = h.out("rotated.pdf");
    assert!(h.run("Save rotated PDF…", Some(&out)).contains("Rotated 2 pages (1 page removed)"));
    assert_eq!(rotations(&out), [90, 270]);

    // Rearrange: move, duplicate, delete.
    h.choose(ToolId::Rearrange, &fixture("sizes.pdf"));
    h.send(Message::Tile(ToolId::Rearrange, 2, TileEvent::MoveEarlier));
    h.send(Message::Tile(ToolId::Rearrange, 0, TileEvent::Duplicate));
    h.send(Message::Tile(ToolId::Rearrange, 3, TileEvent::Delete));
    let out = h.out("rearranged.pdf");
    assert!(h.run("Save rearranged PDF…", Some(&out)).contains("Saved 3 pages"));
    assert_eq!(page_count(&out), 3);

    // Protect, then Unlock with the password.
    h.choose(ToolId::Protect, &fixture("sizes.pdf"));
    h.option(ToolId::Protect, "password", "s3cret");
    assert!(h.shows("The passwords don’t match."));
    h.option(ToolId::Protect, "confirm", "s3cret");
    let locked = h.out("locked.pdf");
    assert!(h.run("Protect PDF…", Some(&locked)).contains("Password-protected copy saved as"));
    assert!(pdftools_core::inspect(&locked).unwrap().needs_password);
    h.choose(ToolId::Compress, &locked);
    assert!(h.shows("This file is password-protected."));
    h.click("Unlock…");
    assert_eq!(h.app.screen, Screen::Tool(ToolId::Unlock));
    h.option(ToolId::Unlock, "password", "s3cret");
    let unlocked = h.out("unlocked.pdf");
    assert!(h.run("Unlock PDF…", Some(&unlocked)).contains("Unlocked copy"));
    assert_eq!(page_count(&unlocked), 3);

    // Page numbers and watermark previews, then the real thing.
    h.choose(ToolId::PageNumbers, &fixture("TAMReview.pdf"));
    h.option(ToolId::PageNumbers, "format", "Page {n} of {total}");
    h.option(ToolId::PageNumbers, "anchor", "top right");
    h.render_pages(ToolId::PageNumbers, 4);
    h.screenshot("page-numbers");
    let out = h.out("numbered.pdf");
    assert!(h.run("Add page numbers…", Some(&out)).contains("Numbered 23 pages"));
    assert!(content(&out, 2).contains("(Page 2 of 23)"));

    h.choose(ToolId::Watermark, &fixture("tracemonkey.pdf"));
    h.render_pages(ToolId::Watermark, 6);
    h.screenshot("watermark");
    let out = h.out("watermarked.pdf");
    assert!(h.run("Add watermark…", Some(&out)).contains("Watermarked 14 pages"));
    assert!(content(&out, 1).contains("/PdfToolsWm0 Do"));

    // Metadata and grayscale.
    h.choose(ToolId::Metadata, &fixture("tracemonkey.pdf"));
    assert!(h.shows("Edit a field to save new metadata."));
    h.option(ToolId::Metadata, "title", "A new title");
    let out = h.out("meta.pdf");
    assert!(h.run("Save metadata…", Some(&out)).contains("Updated properties"));
    assert_eq!(pdftools_core::inspect(&out).unwrap().metadata.title.as_deref(), Some("A new title"));

    h.choose(ToolId::Grayscale, &fixture("canvas.pdf"));
    let out = h.out("gray.pdf");
    assert!(h.run("Convert to grayscale…", Some(&out)).contains("Grayscale copy"));
}

#[test]
fn merge_and_images() {
    use image::{Rgb, RgbImage};
    let mut h = Harness::new("multi");

    h.click("Merge");
    assert!(h.shows("Add PDF files to get started."));
    std::fs::write(h.out("notes.txt"), "text").unwrap();
    h.dialogs.open(&[&fixture("sizes.pdf"), &fixture("rotation.pdf"), &h.out("notes.txt")]);
    h.click("Add files…");
    assert_eq!(h.app.merge.entries.len(), 2, "only PDFs are added");
    for i in 0..5 {
        h.send(Message::Merge(merge::Msg::Tile(i, TileEvent::Shown)));
    }
    h.screenshot("merge");
    h.send(Message::Merge(merge::Msg::Tile(4, TileEvent::MoveEarlier)));
    assert!(h.shows("Reset to file order"));
    h.send(Message::Merge(merge::Msg::Tile(0, TileEvent::RotateRight)));
    h.send(Message::Merge(merge::Msg::Tile(1, TileEvent::ToggleInclude)));
    let out = h.out("merged.pdf");
    assert!(h.run("Merge files…", Some(&out)).contains("Saved 4 pages from 2 files (1 page left out)"));
    assert_eq!(page_count(&out), 4);

    h.click("Images to PDF");
    let wide = h.out("b-wide.jpg");
    RgbImage::from_fn(160, 100, |x, _| if x < 80 { Rgb([230, 20, 20]) } else { Rgb([20, 20, 230]) }).save(&wide).unwrap();
    let tall = h.out("a-tall.png");
    RgbImage::from_pixel(50, 100, Rgb([0, 150, 0])).save(&tall).unwrap();
    h.dialogs.open(&[&wide, &tall]);
    h.click("Add images…");
    assert_eq!(h.app.images.entries.len(), 2);
    h.click("Sort by name");
    assert_eq!(h.app.images.entries[0].name, "a-tall.png");
    h.click("Letter");
    h.screenshot("images");
    h.send(Message::Images(images::Msg::Tile(1, TileEvent::View)));
    h.send(Message::ViewerRotate(90));
    assert_eq!(h.app.images.entries[1].rotation, 90);
    h.send(Message::CloseViewer);
    let out = h.out("images.pdf");
    assert!(h.run("Create PDF…", Some(&out)).contains("Saved 2 pages"));
    assert_eq!(page_count(&out), 2);
}

#[test]
fn signature() {
    let mut h = Harness::new("signature");
    h.choose(ToolId::Signature, &fixture("sizes.pdf"));
    assert!(h.shows("Draw your signature."));
    assert!(h.shows("Page 1 of 3"));
    // Strokes as the drawing pad reports them.
    for (key, at) in [("pad-down", "20,90"), ("pad-move", "60,40"), ("pad-move", "100,95"), ("pad-move", "160,50"), ("pad-up", "")] {
        h.option(ToolId::Signature, key, at);
    }
    assert!(h.shows("Add the signature to a page."));
    h.click("Every page");
    assert!(h.shows("Page 1 of 3 · 1 signature"));
    // Drag the box to the top left, as the page stage reports it.
    h.option(ToolId::Signature, "box", "1,0.05,0.05,0.4");
    h.screenshot("signature");
    h.option(ToolId::Signature, "page", "1");
    assert!(h.shows("Page 2 of 3 · 1 signature"));
    h.option(ToolId::Signature, "remove", "2");
    let out = h.out("signed.pdf");
    assert!(h.run("Sign PDF…", Some(&out)).contains("Signed 2 pages"));
    assert!(content(&out, 1).contains("PdfToolsSig") && !content(&out, 2).contains("PdfToolsSig"));
}
