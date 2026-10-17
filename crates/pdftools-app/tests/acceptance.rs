//! Acceptance scenarios: every tool, used as a person would, on real files,
//! including the awkward ones (locked, damaged, not PDFs, Unicode paths, a
//! 500-page document). Outputs are kept in `target/acceptance/<scenario>`
//! with a `manifest.tsv` of what an independent PDF reader should find in
//! them, so they can be checked with another engine too.
//!
//!     cargo test -p pdftools-app --test acceptance -- --ignored
//!
//! Ignored by default: they take a while and write outside the temp folder.

mod common;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use common::*;
use iced::Size;
use pdftools_app::app::{Message, Screen};
use pdftools_app::catalog::ToolId;
use pdftools_app::settings::{Settings, ThemeChoice};
use pdftools_app::widgets::TileEvent;
use pdftools_app::{images, merge};

/// One scenario's outputs and what to expect of them.
struct Scenario {
    h: Harness,
    dir: PathBuf,
    manifest: String,
}

impl Scenario {
    fn new(name: &str) -> Self {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/acceptance").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self { h: Harness::new(&format!("acc-{name}")), dir, manifest: String::new() }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Records a check for the independent reader: `pages N`, `text P SUBSTR`,
    /// `notext P SUBSTR`, `password PW`, `meta KEY VALUE`, `opens`.
    fn expect(&mut self, file: &Path, check: &str) {
        writeln!(self.manifest, "{}\t{check}", file.file_name().unwrap().to_string_lossy()).unwrap();
    }

    fn finish(self) {
        std::fs::write(self.dir.join("manifest.tsv"), self.manifest).unwrap();
    }
}

fn encrypted(name: &str) -> PathBuf {
    fixture("x").parent().unwrap().parent().unwrap().join("encrypted").join(name)
}

fn is_encrypted(path: &Path) -> bool {
    std::fs::read(path).unwrap().windows(8).any(|w| w == b"/Encrypt")
}

fn has_outlines(path: &Path) -> bool {
    let doc = lopdf::Document::load(path).unwrap();
    doc.catalog().unwrap().get(b"Outlines").is_ok()
}

fn has_forms(path: &Path) -> bool {
    let doc = lopdf::Document::load(path).unwrap();
    doc.catalog().unwrap().get(b"AcroForm").is_ok()
}

fn media_box(path: &Path, page: u32) -> (f32, f32) {
    let doc = lopdf::Document::load(path).unwrap();
    let id = doc.get_pages()[&page];
    let b = doc.get_dictionary(id).unwrap().get(b"MediaBox").unwrap().as_array().unwrap();
    let n = |o: &lopdf::Object| o.as_float().or_else(|_| o.as_i64().map(|v| v as f32)).unwrap();
    (n(&b[2]) - n(&b[0]), n(&b[3]) - n(&b[1]))
}

/// The most colourful pixel on a rendered page (0 = grey everywhere).
fn max_chroma(path: &Path, page: usize) -> u8 {
    let request = pdftools_render::pages::PreviewRequest { path: path.into(), page, width: 400, rotation: 0, gray: false };
    let rendered = pdftools_render::pages::render(&request, &mut Default::default()).unwrap();
    rendered.pixels.chunks_exact(4).map(|p| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])).max().unwrap_or(0)
}

/// Mean absolute difference between two pages rendered in grey (0–255).
fn luma_difference(a: &Path, b: &Path, page: usize) -> f64 {
    let gray = |path: &Path| {
        let request = pdftools_render::pages::PreviewRequest { path: path.into(), page, width: 300, rotation: 0, gray: true };
        pdftools_render::pages::render(&request, &mut Default::default()).unwrap()
    };
    let (a, b) = (gray(a), gray(b));
    assert_eq!((a.width, a.height), (b.width, b.height));
    let sum: u64 = a.pixels.chunks_exact(4).zip(b.pixels.chunks_exact(4)).map(|(p, q)| p[0].abs_diff(q[0]) as u64).sum();
    sum as f64 / (a.pixels.len() / 4) as f64
}

fn ok(status: &str, expected: &str) {
    assert!(status.contains(expected), "status {status:?} should contain {expected:?}");
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files
}

// ── Merge ────────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn merge() {
    let mut s = Scenario::new("merge");
    let h = &mut s.h;
    h.click("Merge");

    // MRG-1: three files in order; a text file is turned away.
    let notes = s.dir.join("notes.txt");
    std::fs::write(&notes, "not a pdf").unwrap();
    h.dialogs.open(&[&fixture("tracemonkey.pdf"), &fixture("sizes.pdf"), &fixture("rotation.pdf"), &notes]);
    h.click("Add PDF files…");
    assert_eq!(h.app.merge.entries.len(), 3);
    assert!(h.shows("19 pages"));
    let out = s.dir.join("MRG-1 three files.pdf");
    ok(&h.run("Merge PDFs…", Some(&out)), "Saved 19 pages from 3 files");
    assert_eq!(page_count(&out), 19);
    s.expect(&out, "pages 19");
    s.expect(&out, "text 1 Trace-based Just-in-Time");
    s.expect(&out, "text 15 Normal");

    // MRG-2: move a file, rotate a page, leave one out.
    let h = &mut s.h;
    h.send(Message::Merge(merge::Msg::FileDown(0)));
    assert_eq!(h.app.merge.entries[0].name, "sizes.pdf");
    h.send(Message::Merge(merge::Msg::Tile(0, TileEvent::RotateRight)));
    h.send(Message::Merge(merge::Msg::Tile(1, TileEvent::ToggleInclude)));
    let out = s.dir.join("MRG-2 reordered.pdf");
    ok(&h.run("Merge PDFs…", Some(&out)), "Saved 18 pages from 3 files (1 page left out)");
    assert_eq!(rotations(&out)[0], 90);
    s.expect(&out, "pages 18");
    s.expect(&out, "text 1 Normal");
    s.expect(&out, "text 2 Landscape");
    s.expect(&out, "text 3 Trace-based Just-in-Time");

    // MRG-3: a file only openable with a password is flagged, offers Unlock,
    // and blocks the merge until it's dealt with.
    let h = &mut s.h;
    h.click("Clear");
    h.dialogs.open(&[&fixture("sizes.pdf"), &encrypted("aes256-password-test.pdf")]);
    h.send(Message::Merge(merge::Msg::Add));
    assert!(matches!(h.app.merge.entries[1].state, merge::EntryState::Failed { locked: true, .. }));
    assert!(h.shows("Unlock…"));
    let blocked = s.dir.join("MRG-3 blocked.pdf");
    let status = h.run("Merge PDFs…", Some(&blocked));
    assert!(!blocked.exists(), "merged despite a locked file: {status}");
    h.click("Unlock…");
    assert_eq!(h.app.screen, Screen::Tool(ToolId::Unlock));

    // MRG-4: a file that only restricts permissions merges, unrestricted.
    let h = &mut s.h;
    h.click("Merge");
    h.click("Clear");
    h.dialogs.open(&[&encrypted("aes256-restrictions-only.pdf"), &fixture("rotation.pdf")]);
    h.send(Message::Merge(merge::Msg::Add));
    let out = s.dir.join("MRG-4 restricted input.pdf");
    ok(&h.run("Merge PDFs…", Some(&out)), "Saved 5 pages from 2 files");
    assert!(!is_encrypted(&out));
    s.expect(&out, "pages 5");
    s.finish();
}

// ── Split ────────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn split() {
    let mut s = Scenario::new("split");
    let h = &mut s.h;
    h.choose(ToolId::Split, &fixture("tracemonkey.pdf"));

    // SPL-1: ranges, including an open-ended one.
    h.option(ToolId::Split, "ranges", "1-2, 3-5, 6-");
    assert!(h.shows("3 files: 2 + 3 + 9 pages"));
    let folder = s.dir.join("SPL-1 ranges");
    std::fs::create_dir_all(&folder).unwrap();
    h.dialogs.open(&[&folder]);
    ok(&h.run("Split PDF…", None), "Created 3 files");
    let parts = files_in(&folder);
    assert_eq!(parts.iter().map(|p| page_count(p)).collect::<Vec<_>>(), [2, 3, 9]);
    assert_eq!(parts[0].file_name().unwrap(), "tracemonkey-1.pdf");

    // SPL-2: every 5 pages, with a custom base name.
    let h = &mut s.h;
    h.option(ToolId::Split, "mode", "every");
    h.option(ToolId::Split, "every", "5");
    h.option(ToolId::Split, "base", "chunk");
    let folder = s.dir.join("SPL-2 every 5");
    std::fs::create_dir_all(&folder).unwrap();
    h.dialogs.open(&[&folder]);
    ok(&h.run("Split PDF…", None), "Created 3 files");
    let parts = files_in(&folder);
    assert_eq!(parts.iter().map(|p| page_count(p)).collect::<Vec<_>>(), [5, 5, 4]);
    assert!(parts[0].file_name().unwrap().to_string_lossy().starts_with("chunk"));

    // SPL-3: each page.
    let h = &mut s.h;
    h.option(ToolId::Split, "mode", "each");
    let folder = s.dir.join("SPL-3 each");
    std::fs::create_dir_all(&folder).unwrap();
    h.dialogs.open(&[&folder]);
    ok(&h.run("Split PDF…", None), "Created 14 files");
    assert_eq!(files_in(&folder).len(), 14);

    // SPL-4: extract pages 2 and 4 into one file.
    let h = &mut s.h;
    h.option(ToolId::Split, "mode", "extract");
    h.send(Message::Tile(ToolId::Split, 1, TileEvent::View));
    h.send(Message::Tile(ToolId::Split, 3, TileEvent::View));
    let out = s.dir.join("SPL-4 extract.pdf");
    ok(&h.run("Extract pages…", Some(&out)), "Extracted 2 pages");
    s.expect(&out, "pages 2");
    s.expect(&out, "notext 1 Trace-based Just-in-Time");

    // SPL-5: bad ranges are explained and nothing is written.
    let h = &mut s.h;
    h.option(ToolId::Split, "mode", "ranges");
    for bad in ["0", "5-2", "abc", "1-3, 40", ""] {
        h.option(ToolId::Split, "ranges", bad);
        let folder = s.dir.join("SPL-5 never");
        h.dialogs.open(&[&folder]);
        let status = h.run("Split PDF…", None);
        assert!(!folder.exists(), "split ran with ranges {bad:?}: {status}");
        h.dialogs.files.borrow_mut().clear();
    }
    s.finish();
}

// ── Compress ─────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn compress() {
    let mut s = Scenario::new("compress");
    for (file, text) in [("tracemonkey.pdf", "Trace-based Just-in-Time"), ("cmykjpeg.pdf", "")] {
        for level in ["lossless", "balanced", "strong"] {
            let h = &mut s.h;
            h.choose(ToolId::Compress, &fixture(file));
            h.option(ToolId::Compress, "level", level);
            let out = s.dir.join(format!("CMP-{level} {file}"));
            ok(&h.run("Compress PDF…", Some(&out)), "Saved as");
            let (before, after) = (pdftools_core::file_size(&fixture(file)), pdftools_core::file_size(&out));
            println!("CMP {file} {level}: {before} -> {after} bytes");
            assert!(after <= before, "{file} {level} grew from {before} to {after}");
            s.expect(&out, &format!("pages {}", page_count(&fixture(file))));
            if !text.is_empty() {
                s.expect(&out, &format!("text 1 {text}"));
            }
        }
    }

    // CMP-4: saving over the input file itself.
    let input = s.dir.join("CMP-4 overwrite me.pdf");
    std::fs::copy(fixture("tracemonkey.pdf"), &input).unwrap();
    let h = &mut s.h;
    h.choose(ToolId::Compress, &input);
    h.dialogs.answer(true);
    ok(&h.run("Compress PDF…", Some(&input)), "Saved as");
    assert_eq!(page_count(&input), 14);
    s.expect(&input, "text 1 Trace-based Just-in-Time");
    assert!(!files_in(&s.dir).iter().any(|p| p.extension().is_some_and(|e| e == "part")), "temporary file left behind");
    s.finish();
}

// ── Protect and Unlock ───────────────────────────────────────────────────

#[test]
#[ignore]
fn protect_and_unlock() {
    let mut s = Scenario::new("security");
    let h = &mut s.h;

    // PRT-1: open password, everything allowed.
    h.choose(ToolId::Protect, &fixture("tracemonkey.pdf"));
    h.option(ToolId::Protect, "password", "Correct-Horse-9");
    h.option(ToolId::Protect, "confirm", "Correct-Horse-9");
    let out = s.dir.join("PRT-1 password.pdf");
    ok(&h.run("Protect PDF…", Some(&out)), "Password-protected copy saved as");
    assert!(pdftools_core::inspect(&out).unwrap().needs_password);
    s.expect(&out, "password Correct-Horse-9");
    s.expect(&out, "text 1 Trace-based Just-in-Time");

    // PRT-2: restrictions with an owner password.
    let h = &mut s.h;
    h.choose(ToolId::Protect, &fixture("tracemonkey.pdf"));
    h.option(ToolId::Protect, "password", "Correct-Horse-9");
    h.option(ToolId::Protect, "confirm", "Correct-Horse-9");
    h.option(ToolId::Protect, "owner", "Owner-Pass-77");
    h.option(ToolId::Protect, "print", "0");
    h.option(ToolId::Protect, "copy", "0");
    let out = s.dir.join("PRT-2 restricted.pdf");
    ok(&h.run("Protect PDF…", Some(&out)), "Password-protected copy saved as");
    s.expect(&out, "ownerpassword Owner-Pass-77");

    // PRT-3: a non-ASCII password round-trips through Unlock.
    let h = &mut s.h;
    h.choose(ToolId::Protect, &fixture("sizes.pdf"));
    h.option(ToolId::Protect, "password", "пароль-密码-9");
    h.option(ToolId::Protect, "confirm", "пароль-密码-9");
    let unicode = s.dir.join("PRT-3 unicode password.pdf");
    ok(&h.run("Protect PDF…", Some(&unicode)), "Password-protected copy saved as");
    s.expect(&unicode, "password пароль-密码-9");
    let h = &mut s.h;
    h.choose(ToolId::Unlock, &unicode);
    h.option(ToolId::Unlock, "password", "пароль-密码-9");
    let out = s.dir.join("PRT-3 unlocked.pdf");
    ok(&h.run("Unlock PDF…", Some(&out)), "Unlocked copy");
    s.expect(&out, "pages 3");

    // PRT-4: mismatched passwords block the run.
    let h = &mut s.h;
    h.choose(ToolId::Protect, &fixture("sizes.pdf"));
    h.option(ToolId::Protect, "password", "Correct-Horse-9");
    h.option(ToolId::Protect, "confirm", "Correct-Horse-8");
    let never = s.dir.join("PRT-4 never.pdf");
    h.run("Protect PDF…", Some(&never));
    assert!(!never.exists());
    h.dialogs.saves.borrow_mut().clear();

    // UNL-1: a file encrypted by qpdf, with its password.
    h.choose(ToolId::Unlock, &encrypted("aes256-password-test.pdf"));
    assert!(h.shows("This file needs its password to open. Enter it on the left and choose “Unlock PDF…” to save an unlocked copy."));
    // UNL-2: a wrong password fails clearly and writes nothing.
    h.option(ToolId::Unlock, "password", "wrong");
    let never = s.dir.join("UNL-2 never.pdf");
    let status = h.run("Unlock PDF…", Some(&never));
    println!("UNL-2 wrong password: {status}");
    assert!(!never.exists());
    h.dialogs.saves.borrow_mut().clear();
    h.option(ToolId::Unlock, "password", "test");
    let out = s.dir.join("UNL-1 unlocked.pdf");
    ok(&h.run("Unlock PDF…", Some(&out)), "Unlocked copy");
    assert!(!is_encrypted(&out));
    s.expect(&out, "pages 3");
    s.expect(&out, "text 1 Normal");

    // UNL-3: restrictions only, no password needed.
    let h = &mut s.h;
    h.choose(ToolId::Unlock, &encrypted("aes256-restrictions-only.pdf"));
    assert!(h.shows("This file restricts printing, copying or editing. Unlocking removes those restrictions."));
    let out = s.dir.join("UNL-3 unrestricted.pdf");
    ok(&h.run("Unlock PDF…", Some(&out)), "Unlocked copy");
    assert!(!is_encrypted(&out));
    s.expect(&out, "pages 3");
    s.finish();
}

// ── Rotate, Rearrange ────────────────────────────────────────────────────

#[test]
#[ignore]
fn rotate_and_rearrange() {
    let mut s = Scenario::new("pages");
    let h = &mut s.h;

    // ROT-1: all pages right, one left out; existing /Rotate respected.
    h.choose(ToolId::Rotate, &fixture("rotation.pdf"));
    let before = rotations(&fixture("rotation.pdf"));
    h.send(Message::GridAction(ToolId::Rotate, "rotate-all-right"));
    let out = s.dir.join("ROT-1 all right.pdf");
    ok(&h.run("Save rotated PDF…", Some(&out)), "Rotated 2 pages");
    assert_eq!(rotations(&out), before.iter().map(|r| (r + 90) % 360).collect::<Vec<_>>());

    // ROT-2: forms survive rotation.
    let h = &mut s.h;
    h.choose(ToolId::Rotate, &fixture("annotation-text-widget.pdf"));
    h.send(Message::Tile(ToolId::Rotate, 0, TileEvent::RotateLeft));
    let out = s.dir.join("ROT-2 forms.pdf");
    ok(&h.run("Save rotated PDF…", Some(&out)), "Rotated 1 page");
    assert!(has_forms(&out), "the form was dropped");

    // REA-1: reverse, delete, duplicate; bookmarks kept.
    let h = &mut s.h;
    h.choose(ToolId::Rearrange, &fixture("TAMReview.pdf"));
    h.send(Message::GridAction(ToolId::Rearrange, "reverse"));
    h.send(Message::Tile(ToolId::Rearrange, 0, TileEvent::Delete));
    h.send(Message::Tile(ToolId::Rearrange, 0, TileEvent::Duplicate));
    let out = s.dir.join("REA-1 rearranged.pdf");
    ok(&h.run("Save rearranged PDF…", Some(&out)), "Saved 23 pages");
    assert!(has_outlines(&out), "bookmarks were dropped");
    s.expect(&out, "pages 23");
    s.expect(&out, "samepage 1 2");
    s.finish();
}

// ── Page numbers, Watermark, Signature ───────────────────────────────────

#[test]
#[ignore]
fn stamps() {
    let mut s = Scenario::new("stamps");
    let h = &mut s.h;

    // PGN-1: custom format, pages 2–4, starting at 10.
    h.choose(ToolId::PageNumbers, &fixture("tracemonkey.pdf"));
    h.option(ToolId::PageNumbers, "format", "Page {n} of {total}");
    h.option(ToolId::PageNumbers, "which", "custom");
    h.option(ToolId::PageNumbers, "pages", "2-4");
    h.option(ToolId::PageNumbers, "start", "10");
    h.option(ToolId::PageNumbers, "anchor", "bottom center");
    let out = s.dir.join("PGN-1 numbers.pdf");
    ok(&h.run("Add page numbers…", Some(&out)), "Numbered 3 pages");
    s.expect(&out, "text 2 Page 10 of 12");
    s.expect(&out, "text 4 Page 12 of 12");
    s.expect(&out, "notext 1 Page ");
    s.expect(&out, "notext 5 Page ");

    // PGN-2: every position renders on rotated and odd-sized pages.
    for anchor in ["top left", "top center", "top right", "bottom left", "bottom right"] {
        let h = &mut s.h;
        h.choose(ToolId::PageNumbers, &fixture("rotation.pdf"));
        h.option(ToolId::PageNumbers, "which", "all");
        h.option(ToolId::PageNumbers, "anchor", anchor);
        let out = s.dir.join(format!("PGN-2 {anchor}.pdf"));
        ok(&h.run("Add page numbers…", Some(&out)), "Numbered 2 pages");
        s.expect(&out, "text 2 Page 11 of 11");
    }

    // WMK-1: text in three scripts, on pages 1 and 3 only.
    let h = &mut s.h;
    h.choose(ToolId::Watermark, &fixture("tracemonkey.pdf"));
    h.option(ToolId::Watermark, "text", "Черновик 草稿 مسودة");
    h.option(ToolId::Watermark, "which", "custom");
    h.option(ToolId::Watermark, "pages", "1,3");
    let out = s.dir.join("WMK-1 scripts.pdf");
    ok(&h.run("Add watermark…", Some(&out)), "Watermarked 2 pages");
    s.expect(&out, "text 1 Trace-based Just-in-Time");
    s.expect(&out, "pages 14");

    // WMK-2: an image, behind the content.
    let logo = s.dir.join("logo.png");
    image::RgbaImage::from_fn(200, 80, |x, _| image::Rgba([200, 30, 30, if x % 40 < 20 { 255 } else { 0 }])).save(&logo).unwrap();
    let h = &mut s.h;
    h.choose(ToolId::Watermark, &fixture("tracemonkey.pdf"));
    h.option(ToolId::Watermark, "which", "all");
    h.option(ToolId::Watermark, "kind", "image");
    h.dialogs.open(&[&logo]);
    h.option(ToolId::Watermark, "choose-image", "");
    h.option(ToolId::Watermark, "under", "1");
    h.option(ToolId::Watermark, "opacity", "60");
    let out = s.dir.join("WMK-2 image under.pdf");
    ok(&h.run("Add watermark…", Some(&out)), "Watermarked 14 pages");
    s.expect(&out, "text 1 Trace-based Just-in-Time");

    // SIG-1: typed, on every page.
    let h = &mut s.h;
    h.choose(ToolId::Signature, &fixture("sizes.pdf"));
    h.option(ToolId::Signature, "source", "type");
    h.option(ToolId::Signature, "name", "Zoë Ångström");
    h.click("Add to all 3 pages");
    let out = s.dir.join("SIG-1 typed.pdf");
    ok(&h.run("Sign PDF…", Some(&out)), "");
    println!("SIG-1: {}", h.status());
    s.expect(&out, "pages 3");
    s.expect(&out, "text 1 Normal");

    // SIG-2: drawn, on one page, moved.
    let h = &mut s.h;
    h.choose(ToolId::Signature, &fixture("tracemonkey.pdf"));
    for (key, at) in [("pad-down", "20,90"), ("pad-move", "60,40"), ("pad-move", "100,95"), ("pad-move", "160,50"), ("pad-up", "")] {
        h.option(ToolId::Signature, key, at);
    }
    h.click("Add to this page");
    h.option(ToolId::Signature, "box", "1,0.6,0.85,0.3");
    let out = s.dir.join("SIG-2 drawn.pdf");
    ok(&h.run("Sign PDF…", Some(&out)), "");
    println!("SIG-2: {}", h.status());
    assert!(out.exists());

    // SIG-3: an uploaded image.
    let h = &mut s.h;
    h.choose(ToolId::Signature, &fixture("sizes.pdf"));
    h.option(ToolId::Signature, "source", "upload");
    h.dialogs.open(&[&logo]);
    h.option(ToolId::Signature, "choose-image", "");
    h.click("Add to this page");
    let out = s.dir.join("SIG-3 image.pdf");
    ok(&h.run("Sign PDF…", Some(&out)), "");
    assert!(out.exists());
    s.finish();
}

// ── Metadata, Grayscale ──────────────────────────────────────────────────

#[test]
#[ignore]
fn metadata_and_grayscale() {
    let mut s = Scenario::new("document");
    let h = &mut s.h;

    // MET-1: every field, with non-Latin text.
    h.choose(ToolId::Metadata, &fixture("tracemonkey.pdf"));
    let fields = [
        ("title", "Отчёт 2026 — 季度报告"),
        ("author", "Zoë Ångström"),
        ("subject", "Acceptance"),
        ("keywords", "qa, pdf, тест"),
        ("creator", "PDF Tools QA"),
        ("producer", "PDF Tools"),
    ];
    for (key, value) in fields {
        h.option(ToolId::Metadata, key, value);
    }
    let out = s.dir.join("MET-1 metadata.pdf");
    ok(&h.run("Save metadata…", Some(&out)), "Updated properties");
    let meta = pdftools_core::inspect(&out).unwrap().metadata;
    assert_eq!(meta.title.as_deref(), Some(fields[0].1));
    assert_eq!(meta.author.as_deref(), Some(fields[1].1));
    assert_eq!(meta.keywords.as_deref(), Some(fields[3].1));
    s.expect(&out, &format!("meta Title {}", fields[0].1));
    s.expect(&out, &format!("meta Author {}", fields[1].1));

    // MET-2: clearing a field removes it.
    let h = &mut s.h;
    h.choose(ToolId::Metadata, &out);
    h.option(ToolId::Metadata, "subject", "");
    let cleared = s.dir.join("MET-2 cleared.pdf");
    ok(&h.run("Save metadata…", Some(&cleared)), "Updated properties");
    assert_eq!(pdftools_core::inspect(&cleared).unwrap().metadata.subject.filter(|s| !s.is_empty()), None);

    // GRY-1: no colour left on colourful pages.
    let mut coloured = Vec::new();
    for file in ["canvas.pdf", "cmykjpeg.pdf", "calrgb.pdf", "devicen.pdf", "alphatrans.pdf", "annotation-highlight.pdf"] {
        let h = &mut s.h;
        h.choose(ToolId::Grayscale, &fixture(file));
        let out = s.dir.join(format!("GRY-1 {file}"));
        ok(&h.run("Convert to grayscale…", Some(&out)), "Grayscale copy");
        let before = max_chroma(&fixture(file), 1);
        let after = max_chroma(&out, 1);
        // The grey copy should look like the original seen in grey.
        let difference = luma_difference(&fixture(file), &out, 1);
        println!("GRY {file}: chroma {before} -> {after}, mean luma difference {difference:.1}");
        // calrgb.pdf is a test of exotic calibrated spaces, which are converted as plain RGB.
        if file != "calrgb.pdf" {
            assert!(difference < 12.0, "{file} looks different in grey (mean difference {difference:.1})");
        }
        // Documented: spot colours (devicen) and gradients (alphatrans) are kept.
        if after > 8 && !matches!(file, "devicen.pdf" | "alphatrans.pdf") {
            coloured.push(format!("{file} (chroma {after})"));
        }
    }
    assert!(coloured.is_empty(), "still in colour: {coloured:?}");
    s.finish();
}

// ── Images to PDF ────────────────────────────────────────────────────────

/// A JPEG whose EXIF says "rotate 90° clockwise to display".
fn exif_rotated_jpeg(path: &Path) {
    let mut jpeg = Vec::new();
    image::RgbImage::from_fn(300, 150, |x, _| if x < 150 { image::Rgb([220, 40, 40]) } else { image::Rgb([40, 40, 220]) })
        .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
        .unwrap();
    // APP1: "Exif\0\0", little-endian TIFF, one IFD entry: Orientation (0x0112) = 6.
    let tiff: [u8; 26] = [
        b'I', b'I', 42, 0, 8, 0, 0, 0, // header, IFD at 8
        1, 0, // one entry
        0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, // Orientation, SHORT, 1, value 6
        0, 0, 0, 0, // no next IFD
    ];
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&tiff);
    let len = (app1.len() + 2) as u16;
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[2..]);
    std::fs::write(path, out).unwrap();
}

#[test]
#[ignore]
fn images_to_pdf() {
    let mut s = Scenario::new("images");
    let wide = s.path("1 wide.jpg");
    image::RgbImage::from_fn(1600, 900, |x, y| image::Rgb([(x / 7) as u8, (y / 4) as u8, 120])).save(&wide).unwrap();
    let alpha = s.path("2 transparent.png");
    image::RgbaImage::from_fn(400, 400, |x, y| {
        let inside = (x as i32 - 200).pow(2) + (y as i32 - 200).pow(2) < 150 * 150;
        image::Rgba([20, 160, 140, if inside { 255 } else { 0 }])
    })
    .save(&alpha)
    .unwrap();
    let exif = s.path("3 camera.jpg");
    exif_rotated_jpeg(&exif);
    let bogus = s.path("4 not an image.jpg");
    std::fs::write(&bogus, "nope").unwrap();

    let h = &mut s.h;
    h.click("Images to PDF");
    h.dialogs.open(&[&wide, &alpha, &exif, &bogus]);
    h.click("Add images…");
    println!("IMG entries: {:?}", h.app.images.entries.iter().map(|e| (&e.name, format!("{:?}", e.state))).collect::<Vec<_>>());

    assert!(h.shows("Remove the image that can’t be read to continue."));
    let blocked = s.dir.join("IMG-0 blocked.pdf");
    assert_eq!(h.run("Create PDF…", Some(&blocked)), "idle");
    h.send(Message::Images(images::Msg::Tile(3, TileEvent::Delete)));

    // IMG-1: fit image — each page the shape of its (upright) image.
    h.send(Message::Images(images::Msg::Option("size", "fit")));
    let out = s.dir.join("IMG-1 fit.pdf");
    let status = h.run("Create PDF…", Some(&out));
    println!("IMG-1: {status}");
    let pages = page_count(&out);
    let boxes: Vec<_> = (1..=pages as u32).map(|p| media_box(&out, p)).collect();
    println!("IMG-1 page sizes: {boxes:?}");
    assert!(boxes[0].0 > boxes[0].1, "wide image on a portrait page");
    assert!(boxes[2].1 > boxes[2].0, "EXIF rotation ignored: camera photo isn't upright");

    // IMG-2: A4, auto orientation, small margin.
    let h = &mut s.h;
    h.send(Message::Images(images::Msg::Option("size", "a4")));
    h.send(Message::Images(images::Msg::Option("margin", "small")));
    let out = s.dir.join("IMG-2 a4.pdf");
    h.run("Create PDF…", Some(&out));
    let a4: Vec<_> = (1..=page_count(&out) as u32).map(|p| media_box(&out, p)).collect();
    println!("IMG-2 page sizes: {a4:?}");
    assert!((a4[0].0 - 842.0).abs() < 2.0 && (a4[0].1 - 595.0).abs() < 2.0, "wide image should get landscape A4");
    assert!((a4[1].0 - 595.0).abs() < 2.0, "square image should get portrait A4");

    // IMG-3: Letter, forced portrait; one image rotated by hand.
    let h = &mut s.h;
    h.send(Message::Images(images::Msg::Option("size", "letter")));
    h.send(Message::Images(images::Msg::Option("orientation", "portrait")));
    h.send(Message::Images(images::Msg::Tile(0, TileEvent::RotateRight)));
    let out = s.dir.join("IMG-3 letter.pdf");
    h.run("Create PDF…", Some(&out));
    let letter = media_box(&out, 1);
    assert!((letter.0 - 612.0).abs() < 2.0 && (letter.1 - 792.0).abs() < 2.0);
    s.expect(&out, "opens");
    s.finish();
}

// ── Awkward inputs and outputs ───────────────────────────────────────────

#[test]
#[ignore]
fn awkward_files() {
    let mut s = Scenario::new("awkward");

    // X-1: spaces, accents and non-Latin scripts in paths.
    let folder = s.dir.join("Ünïcödé папка 文件夹");
    std::fs::create_dir_all(&folder).unwrap();
    let input = folder.join("Résumé #1 & файл.pdf");
    std::fs::copy(fixture("sizes.pdf"), &input).unwrap();
    let h = &mut s.h;
    h.choose(ToolId::Compress, &input);
    assert!(h.shows("Résumé #1 & файл.pdf"));
    let out = folder.join("результат 結果.pdf");
    ok(&h.run("Compress PDF…", Some(&out)), "Saved as");
    assert_eq!(page_count(&out), 3);

    // X-2: damaged, fake and empty files: a clear message, no crash.
    let mut damaged = std::fs::read(fixture("tracemonkey.pdf")).unwrap();
    damaged.truncate(damaged.len() * 6 / 10);
    let cases = [("truncated.pdf", damaged), ("fake.pdf", b"Hello, I am not a PDF".to_vec()), ("empty.pdf", Vec::new())];
    for (name, bytes) in &cases {
        let path = s.dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        for tool in [ToolId::Compress, ToolId::Split, ToolId::Signature, ToolId::Metadata] {
            let h = &mut s.h;
            h.choose(tool, &path);
            h.render_pages(tool, 3);
            let shot = format!("X-2 {} {tool:?}", name.replace('.', "-"));
            h.screenshot_at("acceptance/awkward", &shot, Size::new(1180.0, 760.0));
        }
        let h = &mut s.h;
        h.click("Merge");
        h.send(Message::Merge(merge::Msg::Clear));
        h.dialogs.open(&[path.as_path()]);
        h.send(Message::Merge(merge::Msg::Add));
        println!("X-2 merge {name}: {:?}", h.app.merge.entries.last().map(|e| &e.state));
    }
    // X-2b: a truncated file opens with the pages that survive, says so,
    // and saves them as a sound file.
    let h = &mut s.h;
    h.choose(ToolId::Compress, &s.dir.join("truncated.pdf"));
    assert!(h.shows("This file is damaged. These are the pages that could be recovered; only they will be saved."));
    let out = s.dir.join("X-2 truncated saved.pdf");
    ok(&h.run("Compress PDF…", Some(&out)), "Saved as");
    let recovered = page_count(&out);
    println!("X-2 truncated: recovered {recovered} of 14 pages");
    assert!(recovered >= 8, "only {recovered} pages recovered");
    // (Text isn't checked: the fonts were in the part that was cut off.)
    s.expect(&out, &format!("pages {recovered}"));

    // X-3: cancelling the save dialog does nothing.
    let h = &mut s.h;
    h.choose(ToolId::Rotate, &fixture("sizes.pdf"));
    h.send(Message::Tile(ToolId::Rotate, 0, TileEvent::RotateRight));
    let before = files_in(&s.dir).len();
    assert_eq!(h.run("Save rotated PDF…", None), "idle");
    assert_eq!(files_in(&s.dir).len(), before);

    // X-4: saving into a folder that doesn't exist fails with a message.
    let h = &mut s.h;
    let nowhere = s.dir.join("missing folder").join("out.pdf");
    let status = h.run("Save rotated PDF…", Some(&nowhere));
    println!("X-4 missing folder: {status}");
    assert!(status.starts_with("failed"), "{status}");

    // X-5: the input disappears after it was chosen.
    let gone = s.dir.join("gone.pdf");
    std::fs::copy(fixture("sizes.pdf"), &gone).unwrap();
    let h = &mut s.h;
    h.choose(ToolId::Grayscale, &gone);
    std::fs::remove_file(&gone).unwrap();
    let out = s.dir.join("X-5 never.pdf");
    let status = h.run("Convert to grayscale…", Some(&out));
    println!("X-5 input deleted: {status}");
    assert!(status.starts_with("failed"), "{status}");
    s.finish();
}

// ── A large document ─────────────────────────────────────────────────────

#[test]
#[ignore]
fn large_document() {
    let mut s = Scenario::new("large");
    let big = s.dir.join("big 504 pages.pdf");
    let inputs = vec![fixture("tracemonkey.pdf"); 36];
    let t = Instant::now();
    pdftools_core::merge::merge_files(&inputs, None, &big, |_| {}).unwrap();
    println!("PERF build 504-page file: {:?}, {} MB", t.elapsed(), pdftools_core::file_size(&big) / 1_000_000);

    let mut timed = |label: &str, f: &mut dyn FnMut(&mut Harness)| {
        let t = Instant::now();
        f(&mut s.h);
        println!("PERF {label}: {:?}", t.elapsed());
    };
    timed("open in Split", &mut |h| h.choose(ToolId::Split, &big));
    timed("render 24 thumbnails", &mut |h| h.render_pages(ToolId::Split, 24));
    let folder = s.dir.join("parts");
    std::fs::create_dir_all(&folder).unwrap();
    timed("split every 100 pages", &mut |h| {
        h.option(ToolId::Split, "mode", "every");
        h.option(ToolId::Split, "every", "100");
        h.dialogs.open(&[&folder]);
        ok(&h.run("Split PDF…", None), "Created 6 files");
    });
    let out = s.dir.join("numbered.pdf");
    timed("page numbers on 504 pages", &mut |h| {
        h.choose(ToolId::PageNumbers, &big);
        ok(&h.run("Add page numbers…", Some(&out)), "Numbered 504 pages");
    });
    let out2 = s.dir.join("compressed.pdf");
    timed("compress (balanced)", &mut |h| {
        h.choose(ToolId::Compress, &big);
        ok(&h.run("Compress PDF…", Some(&out2)), "Saved as");
    });
    let out3 = s.dir.join("gray.pdf");
    timed("grayscale", &mut |h| {
        h.choose(ToolId::Grayscale, &big);
        ok(&h.run("Convert to grayscale…", Some(&out3)), "Grayscale copy");
    });
    let out4 = s.dir.join("merged.pdf");
    timed("merge 504 + 14 pages", &mut |h| {
        h.click("Merge");
        h.dialogs.open(&[&big, &fixture("tracemonkey.pdf")]);
        h.send(Message::Merge(merge::Msg::Add));
        ok(&h.run("Merge PDFs…", Some(&out4)), "Saved 518 pages");
    });
    s.expect(&out, "text 504 504");
    s.expect(&out4, "pages 518");
    s.finish();
}

// ── Every screen, both themes, smallest and large windows ───────────────

#[test]
#[ignore]
fn screens() {
    let mut s = Scenario::new("screens");
    for (theme, choice) in [("light", ThemeChoice::Light), ("dark", ThemeChoice::Dark)] {
        let h = &mut s.h;
        h.send(Message::Theme(choice));
        let shots: Vec<(String, Box<dyn Fn(&mut Harness)>)> = vec![
            ("home".into(), Box::new(|h| h.send(Message::Navigate(Screen::Home)))),
            ("about".into(), Box::new(|h| h.send(Message::Navigate(Screen::About)))),
            (
                "merge".into(),
                Box::new(|h| {
                    h.send(Message::Navigate(Screen::Tool(ToolId::Merge)));
                    h.dialogs.open(&[&fixture("tracemonkey.pdf"), &fixture("sizes.pdf"), &encrypted("aes256-password-test.pdf")]);
                    h.send(Message::Merge(merge::Msg::Clear));
                    h.send(Message::Merge(merge::Msg::Add));
                    for i in 0..17 {
                        h.send(Message::Merge(merge::Msg::Tile(i, TileEvent::Shown)));
                    }
                }),
            ),
            ("images-empty".into(), Box::new(|h| h.send(Message::Navigate(Screen::Tool(ToolId::Images))))),
        ];
        let tools = [
            ToolId::Split,
            ToolId::Compress,
            ToolId::Protect,
            ToolId::Unlock,
            ToolId::Rotate,
            ToolId::Rearrange,
            ToolId::PageNumbers,
            ToolId::Watermark,
            ToolId::Metadata,
            ToolId::Signature,
            ToolId::Grayscale,
        ];
        let mut all = shots;
        for tool in tools {
            all.push((
                format!("{tool:?}-empty"),
                Box::new(move |h: &mut Harness| {
                    h.send(Message::Navigate(Screen::Tool(tool)));
                }),
            ));
            all.push((
                format!("{tool:?}"),
                Box::new(move |h: &mut Harness| {
                    h.choose(
                        tool,
                        &fixture(if tool == ToolId::Unlock { "../encrypted/aes256-password-test.pdf" } else { "tracemonkey.pdf" }),
                    );
                    h.render_pages(tool, 14);
                }),
            ));
        }
        for (name, setup) in &all {
            setup(&mut s.h);
            for (label, size) in
                [("small", Size::new(760.0, 500.0)), ("normal", Size::new(1180.0, 760.0)), ("large", Size::new(1920.0, 1080.0))]
            {
                s.h.screenshot_at("acceptance/screens", &format!("{theme}-{label}-{name}"), size);
            }
        }
    }
}

// ── Settings ─────────────────────────────────────────────────────────────

#[test]
#[ignore]
fn settings_persist() {
    let dir = std::env::temp_dir().join(format!("pdftools-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for var in ["APPDATA", "HOME", "XDG_CONFIG_HOME"] {
        std::env::set_var(var, &dir);
    }
    assert_eq!(Settings::load(), Settings::default(), "no file: defaults");
    Settings { theme: ThemeChoice::Dark }.save();
    assert_eq!(Settings::load().theme, ThemeChoice::Dark);
    let file = walk(&dir).into_iter().find(|p| p.ends_with("settings.txt")).expect("settings.txt written");
    println!("SET saved to {}: {:?}", file.display(), std::fs::read_to_string(&file).unwrap());
    std::fs::write(&file, "theme=\u{0}garbage\n\u{fffd}").unwrap();
    assert_eq!(Settings::load().theme, ThemeChoice::System, "a corrupt file falls back to defaults");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// Joins the `screens` screenshots into 3×3 contact sheets for review.
#[test]
#[ignore]
fn contact_sheets() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/acceptance/screens");
    let out = dir.join("sheets");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    for group in ["light-small", "dark-small", "light-normal", "dark-normal", "light-large", "dark-large"] {
        let shots: Vec<_> =
            files_in(&dir).into_iter().filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(&format!("{group}-"))).collect();
        for (n, chunk) in shots.chunks(9).enumerate() {
            let (tw, th) = (760, 500);
            let mut sheet = image::RgbaImage::from_pixel(tw * 3 + 8, th * 3 + 8, image::Rgba([255, 0, 255, 255]));
            for (i, path) in chunk.iter().enumerate() {
                let img = image::open(path).unwrap().resize_exact(tw - 4, th - 4, image::imageops::FilterType::Triangle);
                image::imageops::overlay(&mut sheet, &img, ((i as u32 % 3) * (tw + 2) + 2) as i64, ((i as u32 / 3) * (th + 2) + 2) as i64);
            }
            sheet.save(out.join(format!("{group}-{n}.png"))).unwrap();
            let names: Vec<_> = chunk
                .iter()
                .map(|p| p.file_name().unwrap().to_string_lossy().replace(&format!("{group}-"), "").replace("-tiny-skia.png", ""))
                .collect();
            println!("{group}-{n}: {}", names.join(", "));
        }
    }
}
