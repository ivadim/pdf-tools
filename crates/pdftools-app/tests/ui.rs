//! End-to-end tests of the real app on Slint's testing backend: the UI is
//! driven through its accessibility tree (as assistive technology would),
//! dialogs are scripted, and output files are checked with the PDF engine.
//!
//! Slint UIs live on one thread, so all flows run inside a single test.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use lopdf::Object;
use pdftools_app::app::App;
use pdftools_app::dialogs::ScriptedDialogs;
use pdftools_app::jobs;
use pdftools_app::tools::id;
use pdftools_app::{AppWindow, Merge, Sign, Tools, Viewer};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model};

struct Harness {
    ui: AppWindow,
    app: Rc<RefCell<App>>,
    dialogs: Rc<ScriptedDialogs>,
    out: PathBuf,
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdfjs").join(name)
}

impl Harness {
    fn new(name: &str) -> Self {
        jobs::set_inline(true);
        let ui = AppWindow::new().expect("window");
        // Conditional parts of the UI only exist once the window is shown, and
        // only while they're inside the visible part of a scroll view, so the
        // window is tall enough to show every option panel in full.
        ui.window().set_size(slint::LogicalSize::new(1180.0, 1600.0));
        ui.show().expect("show window");
        let dialogs = Rc::new(ScriptedDialogs::default());
        let app = App::new(&ui, dialogs.clone());
        let out = std::env::temp_dir().join(format!("pdftools-ui-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        settle();
        Self { ui, app, dialogs, out }
    }

    fn out(&self, name: &str) -> PathBuf {
        self.out.join(name)
    }

    fn all(&self, label: &str) -> Vec<ElementHandle> {
        ElementHandle::find_by_accessible_label(&self.ui, label).collect()
    }

    fn find(&self, label: &str) -> ElementHandle {
        self.all(label).into_iter().next().unwrap_or_else(|| panic!("no element labelled {label:?}"))
    }

    fn exists(&self, label: &str) -> bool {
        !self.all(label).is_empty()
    }

    fn click(&self, label: &str) {
        self.find(label).invoke_accessible_default_action();
        settle();
    }

    fn type_into(&self, label: &str, text: &str) {
        self.input(label).set_accessible_value(text);
        settle();
    }

    /// The text field, spin box or slider with this label.
    fn input(&self, label: &str) -> ElementHandle {
        self.all(label)
            .into_iter()
            .find(|e| {
                matches!(
                    e.accessible_role(),
                    Some(AccessibleRole::TextInput | AccessibleRole::Spinbox | AccessibleRole::Slider)
                )
            })
            .unwrap_or_else(|| panic!("no input {label:?}"))
    }

    /// Moves the mouse pointer to the middle of `element`, as a real mouse
    /// would (accessibility actions skip hover and hit-testing).
    fn hover(&self, element: &ElementHandle) -> LogicalPosition {
        let (p, s) = (element.absolute_position(), element.size());
        let at = LogicalPosition::new(p.x + s.width / 2.0, p.y + s.height / 2.0);
        self.ui.window().dispatch_event(WindowEvent::PointerMoved { position: at });
        settle();
        at
    }

    /// Moves the pointer onto `element` and clicks it with the mouse.
    fn mouse_click(&self, element: &ElementHandle) {
        let at = self.hover(element);
        self.ui.window().dispatch_event(WindowEvent::PointerPressed { position: at, button: PointerEventButton::Left });
        self.ui.window().dispatch_event(WindowEvent::PointerReleased { position: at, button: PointerEventButton::Left });
        settle();
    }

    /// The page tile at `index` (0-based) in the visible grid.
    fn tile(&self, index: usize) -> ElementHandle {
        self.all(&format!("Page {}", index + 1))
            .into_iter()
            .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
            .unwrap_or_else(|| panic!("no tile {index}"))
    }

    /// Opens a tool from the sidebar.
    fn open(&self, name: &str) {
        let tab = self
            .all(name)
            .into_iter()
            .find(|e| e.accessible_role() == Some(AccessibleRole::Tab))
            .unwrap_or_else(|| panic!("no tab {name:?}"));
        tab.invoke_accessible_default_action();
        settle();
    }

    fn view(&self, tool: usize) -> pdftools_app::ToolView {
        self.ui.global::<Tools>().get_views().row_data(tool).unwrap()
    }

    fn status(&self, tool: usize) -> String {
        let run = self.view(tool).run;
        match run.kind {
            0 if run.hint.is_empty() => "Ready.".into(),
            0 => run.hint.to_string(),
            _ => format!("{} {}", run.message, run.output_name),
        }
    }

    fn choose(&self, tool: usize, file: &Path) {
        self.dialogs.open(&[file]);
        self.ui.global::<Tools>().invoke_pick_file(tool as i32);
        settle();
    }

    /// Clicks the run button and returns the outcome text.
    fn run(&self, tool: usize, save_as: Option<&Path>) -> String {
        if let Some(path) = save_as {
            self.dialogs.save(Some(path));
        }
        let label = self.view(tool).run.label.to_string();
        let button = self
            .all(&label)
            .into_iter()
            .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .unwrap_or_else(|| panic!("no run button {label:?}"));
        assert_eq!(button.accessible_enabled(), Some(true), "run disabled: {}", self.status(tool));
        button.invoke_accessible_default_action();
        settle();
        let run = self.view(tool).run;
        assert_eq!(run.kind, 2, "run failed: {}", run.message);
        self.status(tool)
    }

    fn tools(&self) -> Tools<'_> {
        self.ui.global::<Tools>()
    }

    fn pages(&self, tool: usize) -> Vec<pdftools_app::PageTile> {
        self.view(tool).pages.iter().collect()
    }
}

/// Delivers all background results (they run inline in tests).
fn settle() {
    for _ in 0..1000 {
        jobs::drain();
        if jobs::queued() == 0 {
            return;
        }
    }
}

fn page_count(path: &Path) -> usize {
    pdftools_core::page_count(path).unwrap()
}

fn rotations(path: &Path) -> Vec<i64> {
    let doc = pdftools_core::load(path).unwrap();
    doc.get_pages()
        .into_values()
        .map(|id| doc.get_dictionary(id).unwrap().get(b"Rotate").and_then(Object::as_i64).unwrap_or(0))
        .collect()
}

fn text(path: &Path, page: u32) -> String {
    pdftools_core::load(path).unwrap().extract_text(&[page]).unwrap_or_default()
}

fn content(path: &Path, page: u32) -> String {
    let doc = pdftools_core::load(path).unwrap();
    String::from_utf8_lossy(&doc.get_page_content(doc.get_pages()[&page])).into_owned()
}

#[test]
fn ui_flows() {
    i_slint_backend_testing::init_integration_test_with_system_time();
    home_and_navigation();
    rotate_with_viewer();
    rearrange_pages();
    merge_files();
    split_modes();
    compress_and_grayscale();
    metadata_fields();
    protect_and_unlock();
    page_numbers();
    watermark();
    signature();
    images_to_pdf();
    mouse_controls();
    about_page();
    file_errors_and_drops();
}

fn home_and_navigation() {
    let h = Harness::new("home");
    assert_eq!(h.tools().get_active(), "home");
    let catalog = h.find("Full tool catalog");
    assert_eq!(catalog.accessible_expanded(), Some(true));
    catalog.invoke_accessible_default_action();
    assert!(!h.tools().get_catalog_open());
    h.click("Full tool catalog");
    for (index, name) in ["Merge", "Split", "Compress", "Protect", "Unlock", "Rotate", "Rearrange", "Page numbers", "Watermark", "Metadata", "Signature", "Grayscale"].iter().enumerate() {
        h.open(name);
        assert_eq!(h.tools().get_active(), id::NAMES[index]);
        if index != id::MERGE {
            assert_eq!(h.status(index), "Choose a PDF to get started.", "{name}");
        }
    }
    h.open("All tools");
    // A tool card on the home screen opens its tool too.
    h.all("Watermark").into_iter().find(|e| e.accessible_role() == Some(AccessibleRole::Button)).unwrap().invoke_accessible_default_action();
    assert_eq!(h.tools().get_active(), "watermark");
}

fn rotate_with_viewer() {
    let h = Harness::new("rotate");
    h.open("Rotate");
    h.click("Choose a PDF");
    assert_eq!(h.view(id::ROTATE).file.state, 0, "cancelled dialog leaves the tool empty");
    h.choose(id::ROTATE, &fixture("rotation.pdf"));
    let tiles = h.pages(id::ROTATE);
    assert_eq!(tiles.len(), 2);
    assert!(tiles.iter().all(|t| t.loaded), "previews rendered");
    assert_eq!(h.view(id::ROTATE).file.details, "2 pages · 6.5 KB");
    assert!(h.status(id::ROTATE).starts_with("Rotate at least one page"));

    let tools = h.tools();
    tools.invoke_tile_action(id::ROTATE as i32, "rotate-right".into(), 0);
    tools.invoke_grid_action(id::ROTATE as i32, "rotate-all-left".into());
    tools.invoke_grid_action(id::ROTATE as i32, "rotate-all-right".into());
    settle();
    assert_eq!(h.pages(id::ROTATE)[0].rotation, 90);

    // The full-size viewer: open, step, rotate, close.
    tools.invoke_tile_action(id::ROTATE as i32, "view".into(), 0);
    settle();
    let viewer = h.ui.global::<Viewer>();
    assert!(viewer.get_open() && viewer.get_tile().loaded);
    assert_eq!(viewer.get_caption(), "Page 1 of 2 · rotated 90°");
    viewer.invoke_step(1);
    settle();
    assert!(!viewer.get_can_next() && viewer.get_can_prev());
    viewer.invoke_rotate(-90);
    settle();
    assert_eq!(h.pages(id::ROTATE)[1].rotation, 270, "extra rotation, on top of the page's own");
    viewer.invoke_step(5);
    viewer.invoke_close();
    settle();
    assert!(!viewer.get_open());

    // Leave page 2 out with the keyboard-equivalent action.
    tools.invoke_tile_action(id::ROTATE as i32, "toggle-include".into(), 1);
    settle();
    assert_eq!(h.view(id::ROTATE).summary, "1 of 2 pages kept");
    assert!(h.view(id::ROTATE).include_all);
    let out = h.out("rotated.pdf");
    let status = h.run(id::ROTATE, Some(&out));
    assert!(status.contains("Rotated 1 page (1 page removed) into rotated.pdf"), "{status}");
    assert_eq!(rotations(&out), [90]);

    // Include everything again, reset, and the tool has nothing to do.
    tools.invoke_grid_action(id::ROTATE as i32, "include-all".into());
    tools.invoke_grid_action(id::ROTATE as i32, "reset".into());
    settle();
    assert!(h.status(id::ROTATE).starts_with("Rotate at least one page"));
    // Cancelling the save dialog leaves things as they were.
    tools.invoke_tile_action(id::ROTATE as i32, "rotate-left".into(), 0);
    settle();
    h.tools().invoke_run(id::ROTATE as i32);
    settle();
    assert_eq!(h.view(id::ROTATE).run.kind, 0, "a cancelled save dialog starts nothing");
    h.click("Remove");
    assert_eq!(h.view(id::ROTATE).file.state, 0);
}

fn rearrange_pages() {
    let h = Harness::new("rearrange");
    h.open("Rearrange");
    h.choose(id::REARRANGE, &fixture("sizes.pdf"));
    assert!(h.status(id::REARRANGE).starts_with("Drag pages"));
    let tools = h.tools();
    let t = id::REARRANGE as i32;
    tools.invoke_tile_moved(t, 2, 0);
    tools.invoke_tile_action(t, "duplicate".into(), 0);
    tools.invoke_tile_action(t, "delete".into(), 3);
    settle();
    let labels: Vec<String> = h.pages(id::REARRANGE).iter().map(|p| p.label.to_string()).collect();
    assert_eq!(labels, ["was p. 3", "was p. 3", "was p. 1", "was p. 2"].map(String::from).to_vec()[..3]);
    let out = h.out("rearranged.pdf");
    h.run(id::REARRANGE, Some(&out));
    assert_eq!(page_count(&out), 3);
    tools.invoke_grid_action(t, "reverse".into());
    tools.invoke_grid_action(t, "reset".into());
    settle();
    assert!(h.status(id::REARRANGE).starts_with("Drag pages"));
    for _ in 0..3 {
        tools.invoke_tile_action(t, "delete".into(), 0);
    }
    settle();
    assert_eq!(h.status(id::REARRANGE), "Keep at least one page.");
    assert!(h.view(id::REARRANGE).empty_text.contains("All pages were deleted"));
}

fn merge_files() {
    let h = Harness::new("merge");
    h.open("Merge");
    assert_eq!(h.status(id::MERGE), "Add PDF files to get started.");
    let merge = h.ui.global::<Merge>();
    let t = id::MERGE as i32;
    h.dialogs.open(&[&fixture("sizes.pdf"), &fixture("rotation.pdf"), &fixture("issue5010.pdf")]);
    merge.invoke_add_files();
    settle();
    assert_eq!(merge.get_files().row_count(), 3);
    assert_eq!(h.pages(id::MERGE).len(), 6);
    assert_eq!(h.view(id::MERGE).run.label, "Merge 3 files…");

    // Merge pages open full size too (their keys carry a file id).
    h.tools().invoke_tile_action(t, "view".into(), 0);
    settle();
    let viewer = h.ui.global::<Viewer>();
    assert!(viewer.get_open());
    let shown = viewer.get_tile();
    assert!(shown.loaded && !shown.failed, "viewer shows the page");
    viewer.invoke_close();
    settle();

    merge.invoke_sort_files();
    settle();
    let names: Vec<String> = merge.get_files().iter().map(|f| f.name.to_string()).collect();
    assert_eq!(names, ["issue5010.pdf", "rotation.pdf", "sizes.pdf"]);
    merge.invoke_file_action("down".into(), 0);
    merge.invoke_file_action("up".into(), 2);
    merge.invoke_file_moved(2, 0);
    merge.invoke_file_action("hover".into(), 0);
    settle();
    assert!(h.view(id::MERGE).flags.row_data(0).unwrap());
    merge.invoke_file_action("unhover".into(), 0);
    merge.invoke_file_action("remove".into(), 2);
    settle();
    let names: Vec<String> = merge.get_files().iter().map(|f| f.name.to_string()).collect();
    // down(0), up(2), drag 3rd to top, remove 3rd.
    assert_eq!(names, ["issue5010.pdf", "rotation.pdf"]);

    // Page-level edits: custom order, rotation, a page left out.
    let tools = h.tools();
    tools.invoke_tile_moved(t, 2, 0);
    settle();
    assert!(merge.get_custom_order());
    tools.invoke_grid_action(t, "rotate-all-right".into());
    tools.invoke_grid_action(t, "rotate-all-left".into());
    tools.invoke_tile_action(t, "rotate-right".into(), 0);
    tools.invoke_tile_action(t, "toggle-include".into(), 1);
    settle();
    assert_eq!(h.view(id::MERGE).summary, "2 of 3 pages kept");
    let out = h.out("merged.pdf");
    let status = h.run(id::MERGE, Some(&out));
    assert!(status.contains("Saved 2 pages from 2 files (1 page left out) to merged.pdf"), "{status}");
    assert_eq!(page_count(&out), 2);
    // rotation.pdf page 2 already has /Rotate 90.
    assert_eq!(rotations(&out), [180, 0]);

    tools.invoke_include_range(t, 0, 2, true);
    tools.invoke_include_range(t, 0, 2, false);
    settle();
    assert!(h.status(id::MERGE).starts_with("Every page is left out"));
    tools.invoke_grid_action(t, "include-all".into());
    tools.invoke_grid_action(t, "reset-order".into());
    settle();
    assert!(!merge.get_custom_order());

    // A locked file can't be merged, and can be handed to Unlock.
    let locked = h.out("locked.pdf");
    pdftools_core::security::protect_file(&fixture("sizes.pdf"), &pdftools_core::security::Protection::with_password("pw"), &locked).unwrap();
    h.dialogs.open(&[&locked]);
    merge.invoke_add_files();
    settle();
    assert!(h.status(id::MERGE).contains("can’t be merged"));
    merge.invoke_file_action("unlock".into(), 2);
    settle();
    assert_eq!(h.tools().get_active(), "unlock");
    assert_eq!(h.view(id::UNLOCK).file.name, "locked.pdf");
    merge.invoke_clear_files();
    settle();
    assert_eq!(merge.get_files().row_count(), 0);
}

fn split_modes() {
    let h = Harness::new("split");
    h.open("Split");
    h.choose(id::SPLIT, &fixture("tracemonkey.pdf"));
    let texts: Vec<String> = h.view(id::SPLIT).texts.iter().map(|s| s.to_string()).collect();
    assert_eq!(&texts[..2], ["1-7, 8-14", "tracemonkey"]);
    let badges: Vec<String> = h.pages(id::SPLIT).iter().map(|p| p.badge.to_string()).collect();
    assert_eq!((badges[0].as_str(), badges[13].as_str()), ("File 1", "File 2"));

    h.type_into("Page ranges", "1-3, 40");
    assert_eq!(h.status(id::SPLIT), "“40” is outside 1–14");
    h.type_into("Page ranges", "2");
    assert!(h.status(id::SPLIT).starts_with("Enter at least two ranges"));
    h.type_into("Page ranges", "1-2, 3");
    assert!(h.pages(id::SPLIT)[5].dimmed, "pages outside every range are dimmed");
    h.type_into("File names", "");
    assert_eq!(h.status(id::SPLIT), "Enter a file name.");
    h.type_into("File names", "part");
    let folder = h.out("ranges");
    std::fs::create_dir_all(&folder).unwrap();
    h.dialogs.open(&[&folder]);
    assert!(h.run(id::SPLIT, None).contains("Created 2 files"));
    assert_eq!(page_count(&folder.join("part-1.pdf")), 2);

    // Same folder again: decline, then accept the overwrite.
    h.dialogs.open(&[&folder]);
    h.dialogs.answer(false);
    h.tools().invoke_run(id::SPLIT as i32);
    settle();
    h.dialogs.open(&[&folder]);
    h.dialogs.answer(true);
    assert!(h.run(id::SPLIT, None).contains("Created 2 files"));

    h.click("Every N pages");
    h.type_into("Pages per file", "5");
    assert_eq!(h.pages(id::SPLIT)[5].badge, "File 2");
    h.click("Each page");
    assert_eq!(h.status(id::SPLIT), "Ready.");
    h.tools().invoke_option(id::SPLIT as i32, "every".into(), "14".into());
    h.click("Every N pages");
    assert_eq!(h.status(id::SPLIT), "That would produce a single file.");

    h.click("Extract pages");
    assert_eq!(h.status(id::SPLIT), "Select the pages to extract.");
    let tools = h.tools();
    let t = id::SPLIT as i32;
    tools.invoke_tile_clicked(t, 1, false, false);
    tools.invoke_tile_clicked(t, 3, true, false);
    tools.invoke_tile_clicked(t, 2, false, true);
    settle();
    let selected: Vec<i32> = h.pages(id::SPLIT).iter().filter(|p| p.selected).map(|p| p.page).collect();
    assert_eq!(selected, [2, 4]);
    let out = h.out("extract.pdf");
    assert!(h.run(id::SPLIT, Some(&out)).contains("Extracted 2 pages"));
    assert_eq!(page_count(&out), 2);
    tools.invoke_grid_action(t, "select-all".into());
    settle();
    assert!(h.pages(id::SPLIT).iter().all(|p| p.selected));
    tools.invoke_grid_action(t, "clear-selection".into());
    settle();
    assert!(h.pages(id::SPLIT).iter().all(|p| !p.selected));
}

fn compress_and_grayscale() {
    let h = Harness::new("compress");
    h.open("Compress");
    h.choose(id::COMPRESS, &fixture("TAMReview.pdf"));
    for level in ["Lossless", "Strong", "Balanced"] {
        h.click(level);
        let out = h.out(&format!("{level}.pdf"));
        let status = h.run(id::COMPRESS, Some(&out));
        assert!(status.contains("smaller") || status.contains("Already compact"), "{status}");
        assert_eq!(page_count(&out), 23);
    }
    h.tools().invoke_tile_action(id::COMPRESS as i32, "toggle-include".into(), 0);
    settle();
    let out = h.out("trimmed.pdf");
    assert!(h.run(id::COMPRESS, Some(&out)).contains("(1 page removed)"));
    assert_eq!(page_count(&out), 22);

    h.open("Grayscale");
    h.choose(id::GRAYSCALE, &fixture("calrgb.pdf"));
    let preview = &h.pages(id::GRAYSCALE)[0];
    assert!(preview.loaded);
    h.click("Convert images");
    h.click("Convert images");
    let out = h.out("gray.pdf");
    assert!(h.run(id::GRAYSCALE, Some(&out)).contains("Grayscale copy saved as gray.pdf"));
    assert!(!content(&out, 1).contains(" rg"), "colour operators converted");
}

fn metadata_fields() {
    let h = Harness::new("metadata");
    h.open("Metadata");
    h.choose(id::METADATA, &fixture("tracemonkey.pdf"));
    assert_eq!(h.status(id::METADATA), "Edit a field to save new metadata.");
    h.type_into("Title", "Trace-based JIT — reviewed");
    h.type_into("Author", "Ада Лавлейс");
    let out = h.out("meta.pdf");
    h.run(id::METADATA, Some(&out));
    let info = pdftools_core::inspect(&out).unwrap().metadata;
    assert_eq!(info.title.as_deref(), Some("Trace-based JIT — reviewed"));
    assert_eq!(info.author.as_deref(), Some("Ада Лавлейс"));

    h.choose(id::METADATA, &out);
    let texts: Vec<String> = h.view(id::METADATA).texts.iter().map(|s| s.to_string()).collect();
    assert_eq!(texts[0], "Trace-based JIT — reviewed");
    assert_ne!(texts[7], "—", "modified date shown");
    h.click("Clear all fields");
    let out2 = h.out("cleared.pdf");
    h.run(id::METADATA, Some(&out2));
    assert_eq!(pdftools_core::inspect(&out2).unwrap().metadata.title, None);
}

fn protect_and_unlock() {
    let h = Harness::new("security");
    h.open("Protect");
    h.choose(id::PROTECT, &fixture("sizes.pdf"));
    assert_eq!(h.status(id::PROTECT), "Enter a password.");
    h.type_into("Password", "open sesame");
    assert_eq!(h.status(id::PROTECT), "The passwords don’t match.");
    h.type_into("Repeat password", "open sesame");
    for label in ["Printing", "Copying text and images", "Editing pages", "Comments and form filling"] {
        h.click(label);
    }
    h.type_into("Permissions password", "owner");
    h.click("Show passwords");
    let locked = h.out("locked.pdf");
    assert!(h.run(id::PROTECT, Some(&locked)).contains("Password-protected copy saved as locked.pdf"));
    assert!(pdftools_core::inspect(&locked).unwrap().needs_password);

    // Another tool explains what to do with a locked file.
    h.open("Compress");
    h.choose(id::COMPRESS, &locked);
    assert_eq!(h.status(id::COMPRESS), "Unlock the file first — use “Unlock…” above.");

    h.open("Unlock");
    h.choose(id::UNLOCK, &fixture("sizes.pdf"));
    assert_eq!(h.status(id::UNLOCK), "This file isn’t protected.");
    assert!(h.exists("This file has no password or restrictions."));
    h.choose(id::UNLOCK, &locked);
    assert!(h.view(id::UNLOCK).empty_text.contains("once the file is unlocked"));
    assert_eq!(h.status(id::UNLOCK), "Enter the file’s password.");
    h.type_into("Password", "wrong");
    h.dialogs.save(Some(&h.out("never.pdf")));
    h.tools().invoke_run(id::UNLOCK as i32);
    settle();
    assert_eq!(h.view(id::UNLOCK).run.kind, 3);
    assert!(h.view(id::UNLOCK).run.message.contains("incorrect"));
    h.type_into("Password", "open sesame");
    let unlocked = h.out("unlocked.pdf");
    assert!(h.run(id::UNLOCK, Some(&unlocked)).contains("Unlocked copy saved as"));
    assert!(!pdftools_core::inspect(&unlocked).unwrap().encrypted);

    // A locked file in another tool offers the hand-off.
    h.open("Compress");
    h.choose(id::COMPRESS, &locked);
    assert!(h.view(id::COMPRESS).file.locked);
    h.click("Unlock it");
    assert_eq!(h.tools().get_active(), "unlock");
}

fn page_numbers() {
    let h = Harness::new("numbers");
    h.open("Page numbers");
    h.choose(id::PAGE_NUMBERS, &fixture("TAMReview.pdf"));
    let t = id::PAGE_NUMBERS as i32;
    h.click("Page 1 of 9".replace("1 of 9", "{n} of {total}").as_str());
    h.tools().invoke_tile_action(t, "toggle-include".into(), 0);
    settle();
    let tiles = h.pages(id::PAGE_NUMBERS);
    assert_eq!(tiles[1].overlay_text, "Page 1 of 22", "preview counts only kept pages");
    h.click("top left");
    for (key, value) in [("size", "14"), ("margin", "36"), ("color", "3"), ("start", "10")] {
        h.tools().invoke_option(t, key.into(), value.into());
    }
    settle();
    assert!(h.pages(id::PAGE_NUMBERS)[1].overlay_x > 0.0 && h.pages(id::PAGE_NUMBERS)[1].overlay_align_x == 0.0);
    let out = h.out("numbered.pdf");
    h.run(id::PAGE_NUMBERS, Some(&out));
    assert_eq!(page_count(&out), 22);
    assert!(content(&out, 1).contains("(Page 10 of 31)"));

    h.click("Some pages");
    h.type_into("Pages to number", "3-4");
    settle();
    let out = h.out("some.pdf");
    h.run(id::PAGE_NUMBERS, Some(&out));
    assert!(content(&out, 2).contains("(Page 10 of 11)") && !content(&out, 1).contains("of 11"));
    h.type_into("Pages to number", "1");
    assert_eq!(h.status(id::PAGE_NUMBERS), "None of the chosen pages are kept.");
    h.type_into("Pages to number", "x");
    assert!(h.status(id::PAGE_NUMBERS).contains("isn’t a page"));
    h.type_into("Format", "Страница {n}");
    assert_eq!(h.status(id::PAGE_NUMBERS), "Page numbers support Latin characters only.");
    h.type_into("Format", " ");
    assert_eq!(h.status(id::PAGE_NUMBERS), "Enter a format, e.g. Page {n}.");
}

fn watermark() {
    let h = Harness::new("watermark");
    h.open("Watermark");
    h.choose(id::WATERMARK, &fixture("sizes.pdf"));
    let t = id::WATERMARK as i32;
    assert!(h.pages(id::WATERMARK)[0].has_overlay_image, "preview shows the stamp");
    for (key, value) in [("font", "Serif"), ("bold", "0"), ("color", "2"), ("width", "50"), ("angle", "30"), ("opacity", "60"), ("under", "1")] {
        h.tools().invoke_option(t, key.into(), value.into());
    }
    h.click("bottom right");
    let out = h.out("text.pdf");
    assert!(h.run(id::WATERMARK, Some(&out)).contains("Watermarked 3 pages"));
    assert!(content(&out, 1).contains("/PdfToolsWm0 Do"));

    h.type_into("Watermark text", "  ");
    assert_eq!(h.status(id::WATERMARK), "Enter the watermark text.");

    // An image watermark on some pages.
    h.click("Image");
    assert_eq!(h.status(id::WATERMARK), "Choose an image.");
    let bad = h.out("bad.png");
    std::fs::write(&bad, b"not an image").unwrap();
    h.dialogs.open(&[&bad]);
    h.tools().invoke_option(t, "choose-image".into(), "".into());
    settle();
    assert!(h.status(id::WATERMARK).starts_with("Couldn’t read the image"));
    let logo = h.out("logo.png");
    std::fs::write(&logo, pdftools_app::raster::render_text("LOGO", pdftools_app::raster::Family::Sans, true, [0, 120, 60], 60.0).unwrap().to_png()).unwrap();
    h.dialogs.open(&[&logo]);
    h.tools().invoke_option(t, "choose-image".into(), "".into());
    settle();
    assert_eq!(h.view(id::WATERMARK).texts.row_data(0).unwrap(), "logo.png");
    h.click("Some pages");
    h.type_into("Pages to watermark", "2");
    let out = h.out("image.pdf");
    assert!(h.run(id::WATERMARK, Some(&out)).contains("Watermarked 1 page"));
    assert!(content(&out, 2).contains("PdfToolsWm") && !content(&out, 1).contains("PdfToolsWm"));
    h.type_into("Pages to watermark", "9");
    assert!(h.status(id::WATERMARK).contains("outside"));

    // Leaving the tool keeps what the user chose on screen.
    h.open("Split");
    h.open("Watermark");
    assert_eq!(h.input("Pages to watermark").accessible_value().unwrap(), "9");
    assert!(h.status(id::WATERMARK).contains("outside"));
}

fn signature() {
    let h = Harness::new("signature");
    h.open("Signature");
    h.choose(id::SIGNATURE, &fixture("sizes.pdf"));
    let sign = h.ui.global::<Sign>();
    assert_eq!(sign.get_page_label(), "Page 1 of 3");
    assert_eq!(h.status(id::SIGNATURE), "Draw your signature.");

    // Draw two strokes, clear, draw again.
    let stroke = |points: &[(f32, f32)]| {
        sign.invoke_pad_down(points[0].0, points[0].1);
        for (x, y) in &points[1..] {
            sign.invoke_pad_move(*x, *y);
        }
        sign.invoke_pad_up();
        settle();
    };
    stroke(&[(10.0, 50.0), (60.0, 20.0), (110.0, 60.0)]);
    assert!(sign.get_has_signature());
    sign.invoke_pad_clear();
    settle();
    assert!(!sign.get_has_signature());
    stroke(&[(10.0, 50.0), (60.0, 20.0), (110.0, 60.0), (160.0, 30.0)]);
    stroke(&[(40.0, 80.0)]);
    h.tools().invoke_option(id::SIGNATURE as i32, "ink".into(), "2".into());
    settle();
    assert_eq!(h.status(id::SIGNATURE), "Add the signature to a page.");

    sign.invoke_add_box(true);
    settle();
    assert_eq!(sign.get_boxes().row_count(), 1, "only the current page's box is shown");
    let first = sign.get_boxes().row_data(0).unwrap();
    sign.invoke_box_changed(first.id, 0.1, 0.1, 0.4, 0.0);
    settle();
    let moved = sign.get_boxes().row_data(0).unwrap();
    assert!((moved.x - 0.1).abs() < 1e-6 && (moved.width - 0.4).abs() < 1e-6);
    sign.invoke_page_step(1);
    settle();
    assert_eq!(sign.get_page_label(), "Page 2 of 3 · 1 signature");
    let second = sign.get_boxes().row_data(0).unwrap();
    sign.invoke_box_removed(second.id);
    sign.invoke_page_step(5);
    settle();
    let out = h.out("signed.pdf");
    assert!(h.run(id::SIGNATURE, Some(&out)).contains("Signed 2 pages"));
    assert!(content(&out, 1).contains("PdfToolsSig") && !content(&out, 2).contains("PdfToolsSig"));

    // Typed and uploaded signatures.
    h.click("Type");
    assert_eq!(h.status(id::SIGNATURE), "Type your name.");
    h.type_into("Name", "Ada Lovelace");
    h.tools().invoke_option(id::SIGNATURE as i32, "font".into(), "Serif".into());
    settle();
    assert!(sign.get_signature_aspect() > 2.0);
    h.click("Image");
    assert_eq!(h.status(id::SIGNATURE), "Choose a signature image.");
    let png = h.out("sig.png");
    std::fs::write(&png, pdftools_app::raster::render_text("A.L.", pdftools_app::raster::Family::Script, false, [0, 0, 0], 60.0).unwrap().to_png()).unwrap();
    h.dialogs.open(&[&png]);
    sign.invoke_choose_image();
    settle();
    assert_eq!(sign.get_upload_name(), "sig.png");
    sign.invoke_add_box(false);
    settle();
    let out = h.out("signed-image.pdf");
    assert!(h.run(id::SIGNATURE, Some(&out)).contains("Signed"));
}

fn images_to_pdf() {
    use image::{Rgb, RgbImage, Rgba, RgbaImage};
    use pdftools_app::previews::{render, DocCache, PreviewRequest};

    let h = Harness::new("images");
    h.open("Images to PDF");
    assert_eq!(h.status(id::IMAGES), "Add images to get started.");
    let t = id::IMAGES as i32;

    // A wide JPEG, red on the left and blue on the right; a tall PNG with a
    // transparent half; and a file that isn't an image.
    let wide = h.out("b-wide.jpg");
    RgbImage::from_fn(160, 100, |x, _| if x < 80 { Rgb([230, 20, 20]) } else { Rgb([20, 20, 230]) }).save(&wide).unwrap();
    let tall = h.out("a-tall.png");
    RgbaImage::from_fn(50, 100, |_, y| Rgba([0, 150, 0, if y < 50 { 255 } else { 0 }])).save(&tall).unwrap();
    std::fs::write(h.out("notes.txt"), "text").unwrap();
    h.dialogs.open(&[&wide, &tall, &h.out("notes.txt")]);
    h.click("Add images…");
    let pages = h.pages(id::IMAGES);
    assert_eq!(pages.len(), 2, "the text file is ignored");
    assert!(pages.iter().all(|p| p.loaded && p.has_overlay_image), "thumbnails shown");
    assert!(pages[0].aspect > 1.0 && pages[1].aspect < 1.0, "Auto: landscape page for the wide image");
    assert_eq!(h.view(id::IMAGES).summary, "2 images");

    h.click("Sort by name");
    let labels: Vec<String> = h.pages(id::IMAGES).iter().map(|p| p.label.to_string()).collect();
    assert_eq!(labels, ["a-tall.png", "b-wide.jpg"]);
    h.tools().invoke_tile_moved(t, 1, 0);
    settle();
    assert_eq!(h.pages(id::IMAGES)[0].label, "b-wide.jpg");

    // Turn the wide image upright: its page turns portrait.
    h.tools().invoke_tile_action(t, "rotate-right".into(), 0);
    settle();
    assert!(h.pages(id::IMAGES)[0].aspect < 1.0);
    h.click("Letter");
    h.click("Large");
    let tile = &h.pages(id::IMAGES)[0];
    assert!(tile.overlay_image_width < 1.0, "a margin around the image");

    // The viewer shows the image placed on its page.
    h.tools().invoke_tile_action(t, "view".into(), 0);
    settle();
    let viewer = h.ui.global::<Viewer>();
    assert!(viewer.get_open() && viewer.get_tile().has_overlay_image);
    assert!(viewer.get_tile().overlay_image.size().width > tile.overlay_image.size().width, "a sharper image");
    viewer.invoke_close();

    // A broken image blocks the run until it's removed.
    let broken = h.out("broken.jpg");
    std::fs::write(&broken, b"not a jpeg").unwrap();
    h.app.borrow_mut().files_dropped(vec![broken]);
    settle();
    assert!(h.pages(id::IMAGES)[2].failed);
    assert_eq!(h.status(id::IMAGES), "Remove the image that can’t be read to continue.");
    h.tools().invoke_tile_action(t, "delete".into(), 2);
    settle();
    assert_eq!(h.status(id::IMAGES), "Ready.");

    let out = h.out("images.pdf");
    assert!(h.run(id::IMAGES, Some(&out)).contains("Saved 2 pages"));
    let doc = lopdf::Document::load(&out).unwrap();
    let boxes: Vec<Vec<f32>> = doc
        .get_pages()
        .values()
        .map(|id| {
            let page = doc.get_dictionary(*id).unwrap();
            page.get(b"MediaBox").unwrap().as_array().unwrap().iter().map(|o| o.as_float().unwrap()).collect()
        })
        .collect();
    assert_eq!(boxes, vec![vec![0.0, 0.0, 612.0, 792.0]; 2], "Letter, portrait");

    // The turned photo: red, once on the left, is now at the top.
    let request = PreviewRequest { path: out.clone(), page: 1, width: 100, rotation: 0, gray: false };
    let page = render(&request, &mut DocCache::default()).unwrap();
    let pixel = |x: u32, y: u32| {
        let i = ((y * page.width + x) * 4) as usize;
        [page.pixels[i], page.pixels[i + 1], page.pixels[i + 2]]
    };
    let (top, bottom) = (pixel(50, page.height * 3 / 10), pixel(50, page.height * 7 / 10));
    assert!(top[0] > 150 && top[2] < 100, "top is red: {top:?}");
    assert!(bottom[2] > 150 && bottom[0] < 100, "bottom is blue: {bottom:?}");

    h.click("Clear");
    assert!(h.pages(id::IMAGES).is_empty());
}

/// Page controls used with a real mouse: the hover buttons must still be
/// there when the pointer reaches them, and the viewer's buttons must not move.
fn mouse_controls() {
    let h = Harness::new("mouse");
    h.open("Merge");
    h.dialogs.open(&[&fixture("sizes.pdf")]);
    h.ui.global::<Merge>().invoke_add_files();
    settle();

    // Hovering the page shows its buttons; moving onto one keeps them.
    let tile = h.tile(0);
    let (p, s) = (tile.absolute_position(), tile.size());
    h.ui.window().dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(p.x + s.width / 2.0, p.y + 60.0) });
    settle();
    let rotate = h.find("Rotate right");
    h.mouse_click(&rotate);
    assert_eq!(h.pages(id::MERGE)[0].rotation, 90, "the hover button rotates the page");
    assert!(!h.ui.global::<Viewer>().get_open(), "the click didn't fall through to the page");
    // Back over the page on the way to its checkbox.
    h.ui.window().dispatch_event(WindowEvent::PointerMoved { position: LogicalPosition::new(p.x + s.width / 2.0, p.y + 60.0) });
    settle();
    let checkbox = h.find("Leave this page out");
    h.mouse_click(&checkbox);
    assert!(h.pages(id::MERGE)[0].excluded, "the include checkbox works with the mouse");

    // The viewer's rotate buttons stay put although the caption changes.
    h.tools().invoke_tile_action(id::MERGE as i32, "view".into(), 1);
    settle();
    let before = h.find("Rotate left").absolute_position();
    h.mouse_click(&h.find("Rotate right"));
    let viewer = h.ui.global::<Viewer>();
    assert!(viewer.get_caption().contains("rotated 90°"), "{}", viewer.get_caption());
    assert_eq!(h.find("Rotate left").absolute_position(), before);
    h.mouse_click(&h.find("Rotate right"));
    assert_eq!(h.pages(id::MERGE)[1].rotation, 180, "both clicks hit the button");
    viewer.invoke_close();
    settle();
}

fn about_page() {
    let h = Harness::new("about");
    h.open("About");
    assert!(h.exists("version"));
    let about = h.ui.global::<pdftools_app::About>();
    assert_eq!(about.get_version(), env!("CARGO_PKG_VERSION"));
    assert_eq!(about.get_platform(), pdftools_app::app::platform());

    let repo = env!("CARGO_PKG_REPOSITORY");
    assert_eq!(repo, "https://github.com/ivadim/pdf-tools");
    h.click("GitHub");
    h.click("Report a problem");
    h.mouse_click(&h.find("MIT licence"));
    assert_eq!(
        *h.dialogs.opened_urls.borrow(),
        [repo.to_owned(), format!("{repo}/issues/new"), format!("{repo}/blob/main/LICENSE")]
    );
}

fn file_errors_and_drops() {
    let h = Harness::new("errors");
    h.open("Compress");
    let broken = h.out("broken.pdf");
    std::fs::write(&broken, b"%PDF-1.7\nnot really").unwrap();
    h.choose(id::COMPRESS, &broken);
    assert_eq!(h.view(id::COMPRESS).file.state, 3);
    assert!(h.exists("Choose another"));

    // Dropping files onto the window opens them in the active tool.
    h.app.borrow_mut().files_dropped(vec![h.out("notes.txt"), fixture("sizes.pdf")]);
    settle();
    assert_eq!(h.view(id::COMPRESS).file.name, "sizes.pdf");
    h.open("Merge");
    h.app.borrow_mut().files_dropped(vec![fixture("sizes.pdf"), fixture("canvas.pdf")]);
    settle();
    assert_eq!(h.ui.global::<Merge>().get_files().row_count(), 2);
    h.open("All tools");
    h.app.borrow_mut().files_dropped(vec![fixture("sizes.pdf")]);
    settle();

    // Overwriting the input reloads it.
    h.open("Compress");
    let copy = h.out("copy.pdf");
    std::fs::copy(fixture("tracemonkey.pdf"), &copy).unwrap();
    h.choose(id::COMPRESS, &copy);
    h.run(id::COMPRESS, Some(&copy));
    assert_eq!(h.view(id::COMPRESS).file.name, "copy.pdf");
    assert_eq!(h.view(id::COMPRESS).run.kind, 2, "result kept after reload");
    let _ = text(&copy, 1);
}

