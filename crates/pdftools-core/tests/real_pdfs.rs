//! Every operation against real-world PDFs (tests/fixtures/pdfjs).
//!
//! Each test runs over all fixtures and reports every failure at once, so a
//! single run shows which files trip which operation.

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};
use pdftools_core::compress::{compress_file, Level};
use pdftools_core::grayscale::grayscale_file;
use pdftools_core::organize::{organize_file, split_file, SplitPart};
use pdftools_core::security::{protect_file, unlock_file, Protection};
use pdftools_core::stamps::{
    add_image_stamp_file, add_page_numbers_file, add_signature_file, ImageStamp, PageNumbers, Placement,
};
use pdftools_core::{metadata, merge_files, Anchor, Error, HAlign, Metadata, PageSpec, VAlign};

// ── Harness ──────────────────────────────────────────────────────────────

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdfjs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("fixtures missing at {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
        .collect();
    files.sort();
    assert!(files.len() >= 10, "expected the pdf.js fixtures, found {}", files.len());
    files
}

/// A fresh output directory per test.
fn out_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdftools-it-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

/// Runs `check` for every fixture, collecting failures (including panics).
fn for_each_fixture(test: &str, mut check: impl FnMut(&Path, &Path) -> Result<(), String>) {
    let out = out_dir(test);
    let mut failures = Vec::new();
    for fixture in fixtures() {
        let file_out = out.join(name(&fixture).trim_end_matches(".pdf"));
        std::fs::create_dir_all(&file_out).unwrap();
        match catch_unwind(AssertUnwindSafe(|| check(&fixture, &file_out))) {
            Ok(Ok(())) => {}
            Ok(Err(message)) => failures.push(format!("{}: {message}", name(&fixture))),
            Err(panic) => failures.push(format!("{}: PANIC {}", name(&fixture), panic_message(&panic))),
        }
    }
    let _ = std::fs::remove_dir_all(&out);
    assert!(failures.is_empty(), "{} fixture(s) failed:\n  {}", failures.len(), failures.join("\n  "));
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

trait Context<T> {
    fn ctx(self, what: &str) -> Result<T, String>;
}

impl<T, E: std::fmt::Display> Context<T> for Result<T, E> {
    fn ctx(self, what: &str) -> Result<T, String> {
        self.map_err(|e| format!("{what}: {e}"))
    }
}

fn ensure(condition: bool, message: impl FnOnce() -> String) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message())
    }
}

fn load(path: &Path) -> Result<Document, String> {
    pdftools_core::load(path).ctx("reload output")
}

/// Extracted text per page, whitespace-normalised. Pages whose text can't
/// be extracted (unusual fonts) come back empty, consistently.
fn page_texts(doc: &Document) -> Vec<String> {
    let count = doc.get_pages().len() as u32;
    (1..=count)
        .map(|n| {
            doc.extract_text(&[n])
                .unwrap_or_default()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

fn rotations(doc: &Document) -> Vec<i64> {
    doc.get_pages()
        .into_values()
        .map(|id| inherited(doc, id, b"Rotate").and_then(|o| o.as_i64().ok()).unwrap_or(0).rem_euclid(360))
        .collect()
}

fn inherited(doc: &Document, mut id: ObjectId, key: &[u8]) -> Option<Object> {
    for _ in 0..32 {
        let dict = doc.get_dictionary(id).ok()?;
        if let Ok(value) = dict.get(key) {
            return Some(value.clone());
        }
        id = dict.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

fn page_count(doc: &Document) -> usize {
    doc.get_pages().len()
}

/// The decoded content of page `n` (1-based), all streams joined.
fn page_content(doc: &Document, n: u32) -> String {
    let id = doc.get_pages()[&n];
    String::from_utf8_lossy(&doc.get_page_content(id)).into_owned()
}

/// Compares extracted text page by page. Pages whose original text lopdf
/// can't extract (e.g. content with comments, which its parser stops at)
/// are skipped; other checks cover those.
fn same_text(before: &[String], after: &[String], what: &str) -> Result<(), String> {
    ensure(before.len() == after.len(), || format!("{what}: {} pages became {}", before.len(), after.len()))?;
    for (i, (b, a)) in before.iter().zip(after).enumerate() {
        if b.is_empty() {
            continue;
        }
        ensure(b == a, || format!("{what}: text of page {} changed\n    before: {:.80}\n    after:  {:.80}", i + 1, b, a))?;
    }
    Ok(())
}

/// Text-showing operators on a page, counted with a plain token scan so it
/// works where lopdf's parser gives up.
fn text_operator_count(doc: &Document, n: u32) -> usize {
    let content = page_content(doc, n);
    content
        .lines()
        .map(|line| line.split('%').next().unwrap_or(""))
        .flat_map(|line| line.split_whitespace().map(str::to_owned).collect::<Vec<_>>())
        .filter(|token| matches!(token.as_str(), "Tj" | "TJ" | "'" | "\""))
        .count()
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(width, height, |x, _| image::Rgba([200, 30, 30, if x % 2 == 0 { 255 } else { 90 }]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

// ── Reading ──────────────────────────────────────────────────────────────

#[test]
fn every_fixture_opens() {
    for_each_fixture("open", |fixture, _| {
        let info = pdftools_core::inspect(fixture).ctx("inspect")?;
        ensure(!info.needs_password, || "unexpectedly password-protected".into())?;
        ensure(info.pages.unwrap_or(0) > 0, || "no pages".into())?;
        ensure(info.file_size > 0, || "no size".into())
    });
}

/// Guards the other tests: text comparisons only prove something if text
/// can actually be extracted from a good share of the fixtures.
#[test]
fn text_comparisons_are_meaningful() {
    let mut with_text = Vec::new();
    for fixture in fixtures() {
        let texts = page_texts(&pdftools_core::load(&fixture).unwrap());
        if texts.iter().any(|t| t.len() > 20) {
            with_text.push(name(&fixture));
        }
    }
    assert!(with_text.len() >= 5, "only {} fixtures have extractable text: {with_text:?}", with_text.len());
    assert!(with_text.contains(&"tracemonkey.pdf".to_string()), "{with_text:?}");
}

#[test]
fn corrupted_files_fail_cleanly() {
    for_each_fixture("corrupt", |fixture, out| {
        let bytes = std::fs::read(fixture).unwrap();
        for (label, data) in [("truncated", bytes[..bytes.len() / 2].to_vec()), ("garbage", b"%PDF-1.7\nnot a pdf".to_vec())] {
            let broken = out.join(format!("{label}.pdf"));
            std::fs::write(&broken, data).unwrap();
            // Either outcome is fine; panicking or hanging is not.
            let _ = pdftools_core::inspect(&broken);
            let _ = compress_file(&broken, Level::Lossless, &out.join(format!("{label}-out.pdf")));
        }
        Ok(())
    });
}

// ── Page tools ───────────────────────────────────────────────────────────

#[test]
fn reversing_pages_keeps_their_content() {
    for_each_fixture("reverse", |fixture, out| {
        let before = page_texts(&load(fixture)?);
        let n = before.len();
        let order: Vec<PageSpec> = (1..=n).rev().map(|p| PageSpec::new(0, p)).collect();
        let output = out.join("reversed.pdf");
        organize_file(fixture, &order, &output).ctx("organize")?;
        let mut after = page_texts(&load(&output)?);
        after.reverse();
        same_text(&before, &after, "reversed")
    });
}

#[test]
fn rotation_adds_to_existing_rotation() {
    for_each_fixture("rotate", |fixture, out| {
        let doc = load(fixture)?;
        let original = rotations(&doc);
        let specs: Vec<PageSpec> = (1..=original.len()).map(|p| PageSpec::new(0, p).rotated(if p % 2 == 1 { 90 } else { -90 })).collect();
        let output = out.join("rotated.pdf");
        organize_file(fixture, &specs, &output).ctx("organize")?;
        let rotated = rotations(&load(&output)?);
        for (i, (before, after)) in original.iter().zip(&rotated).enumerate() {
            let delta = if i % 2 == 0 { 90 } else { 270 };
            ensure(*after == (before + delta) % 360, || format!("page {}: {before}° became {after}°", i + 1))?;
        }
        same_text(&page_texts(&doc), &page_texts(&load(&output)?), "rotated")
    });
}

#[test]
fn keeping_some_pages_drops_the_rest() {
    for_each_fixture("keep", |fixture, out| {
        let doc = load(fixture)?;
        let before = page_texts(&doc);
        if before.len() < 2 {
            return Ok(());
        }
        let keep: Vec<usize> = (1..=before.len()).filter(|p| p % 2 == 1).collect();
        let mut trimmed = pdftools_core::load_pages(fixture, Some(&keep)).ctx("load_pages")?;
        let output = out.join("odd.pdf");
        pdftools_core::save(&mut trimmed, &output).ctx("save")?;
        let reloaded = load(&output)?;
        let expected: Vec<String> = keep.iter().map(|p| before[p - 1].clone()).collect();
        same_text(&expected, &page_texts(&reloaded), "odd pages")?;
        // Content of dropped pages must not linger in the file.
        ensure(std::fs::metadata(&output).unwrap().len() <= std::fs::metadata(fixture).unwrap().len() * 2, || {
            "trimmed file is far larger than the original".into()
        })
    });
}

#[test]
fn splitting_into_single_pages() {
    for_each_fixture("split", |fixture, out| {
        let before = page_texts(&load(fixture)?);
        let parts: Vec<SplitPart> = (1..=before.len().min(20))
            .map(|p| SplitPart { pages: vec![PageSpec::new(0, p)], output: out.join(format!("page-{p}.pdf")) })
            .collect();
        split_file(fixture, &parts, |_| {}).ctx("split")?;
        for (i, part) in parts.iter().enumerate() {
            let piece = load(&part.output)?;
            ensure(page_count(&piece) == 1, || format!("part {} has {} pages", i + 1, page_count(&piece)))?;
            same_text(&before[i..=i], &page_texts(&piece), &format!("part {}", i + 1))?;
        }
        Ok(())
    });
}

#[test]
fn merging_every_fixture_into_one() {
    let files = fixtures();
    let out = out_dir("merge");
    let output = out.join("all.pdf");
    let summary = merge_files(&files, None, &output, |_| {}).expect("merge all fixtures");

    let mut expected = Vec::new();
    for file in &files {
        expected.extend(page_texts(&pdftools_core::load(file).unwrap()));
    }
    let merged = pdftools_core::load(&output).unwrap();
    assert_eq!(summary.pages, expected.len());
    same_text(&expected, &page_texts(&merged), "merged").unwrap();

    // A custom order with repeats and a page left out.
    let order = [PageSpec::new(1, 1), PageSpec::new(0, 1).rotated(90), PageSpec::new(1, 1)];
    let custom = out.join("custom.pdf");
    merge_files(&files[..2], Some(&order), &custom, |_| {}).expect("custom merge");
    assert_eq!(page_count(&pdftools_core::load(&custom).unwrap()), 3);
    let _ = std::fs::remove_dir_all(&out);
}

// ── Optimise & secure ────────────────────────────────────────────────────

#[test]
fn compression_keeps_content_and_never_bloats() {
    for_each_fixture("compress", |fixture, out| {
        let before = page_texts(&load(fixture)?);
        for level in [Level::Lossless, Level::Balanced, Level::Strong] {
            let output = out.join(format!("{level:?}.pdf"));
            let summary = compress_file(fixture, level, &output).ctx(&format!("{level:?}"))?;
            same_text(&before, &page_texts(&load(&output)?), &format!("{level:?}"))?;
            // Tiny files may gain a few bytes of structure, nothing more.
            ensure(summary.after <= summary.before + summary.before / 20 + 2048, || {
                format!("{level:?}: {} → {} bytes", summary.before, summary.after)
            })?;
        }
        Ok(())
    });
}

#[test]
fn protect_and_unlock_round_trip() {
    for_each_fixture("security", |fixture, out| {
        let before = page_texts(&load(fixture)?);
        let locked = out.join("locked.pdf");
        let protection = Protection { allow_copy: false, ..Protection::with_password("pässwörd") };
        protect_file(fixture, &protection, &locked).ctx("protect")?;

        let info = pdftools_core::inspect(&locked).ctx("inspect locked")?;
        ensure(info.needs_password, || "locked file opens without a password".into())?;
        ensure(matches!(pdftools_core::load(&locked), Err(Error::Encrypted(_))), || "load accepted a locked file".into())?;
        ensure(matches!(unlock_file(&locked, "wrong", &out.join("x.pdf")), Err(Error::WrongPassword)), || {
            "wrong password not rejected".into()
        })?;

        let unlocked = out.join("unlocked.pdf");
        unlock_file(&locked, "pässwörd", &unlocked).ctx("unlock")?;
        let doc = load(&unlocked)?;
        ensure(!doc.is_encrypted() && doc.trailer.get(b"Encrypt").is_err(), || "still encrypted".into())?;
        same_text(&before, &page_texts(&doc), "after unlock")
    });
}

/// Files encrypted by qpdf (tests/fixtures/encrypted), so Unlock is checked
/// against another implementation and not only against our own output.
#[test]
fn unlocks_files_encrypted_elsewhere() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/encrypted");
    let out = out_dir("unlock-elsewhere");
    let original = load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdfjs/sizes.pdf")).unwrap();

    // AES-256 with an open password: "test".
    let locked = dir.join("aes256-password-test.pdf");
    let info = pdftools_core::inspect(&locked).unwrap();
    assert!(info.needs_password && info.pages.is_none());
    assert!(matches!(unlock_file(&locked, "wrong", &out.join("x.pdf")), Err(Error::WrongPassword)));
    unlock_file(&locked, "test", &out.join("unlocked.pdf")).unwrap();
    let unlocked = load(&out.join("unlocked.pdf")).unwrap();
    assert!(unlocked.trailer.get(b"Encrypt").is_err());
    same_text(&page_texts(&original), &page_texts(&unlocked), "after unlock").unwrap();

    // Restrictions only: opens without a password, and Unlock lifts them.
    let restricted = dir.join("aes256-restrictions-only.pdf");
    let info = pdftools_core::inspect(&restricted).unwrap();
    assert!(!info.needs_password && info.encrypted && info.pages == Some(3));
    unlock_file(&restricted, "", &out.join("free.pdf")).unwrap();
    assert!(!pdftools_core::inspect(&out.join("free.pdf")).unwrap().encrypted);
}

#[test]
fn metadata_round_trip() {
    for_each_fixture("metadata", |fixture, out| {
        let output = out.join("meta.pdf");
        let wanted = Metadata {
            title: Some("Тест — test ✓".into()),
            author: Some("PDF Tools".into()),
            keywords: Some("a, b".into()),
            ..Default::default()
        };
        metadata::write_file(fixture, &wanted, &output).ctx("write")?;
        let read = pdftools_core::inspect(&output).ctx("inspect")?.metadata;
        ensure(read.title == wanted.title && read.author == wanted.author && read.keywords == wanted.keywords, || {
            format!("read back {read:?}")
        })?;
        ensure(read.modified.is_some(), || "no ModDate".into())?;
        same_text(&page_texts(&load(fixture)?), &page_texts(&load(&output)?), "metadata")
    });
}

fn colour_operator_count(doc: &Document) -> usize {
    content_streams(doc)
        .into_iter()
        .filter_map(|(_, stream)| Content::decode(&stream.decompressed_content().unwrap_or(stream.content.clone())).ok())
        .map(|c| c.operations.iter().filter(|op| matches!(op.operator.as_str(), "rg" | "RG" | "k" | "K")).count())
        .sum()
}

#[test]
fn grayscale_leaves_no_device_colour_operators() {
    // The fixtures must actually contain colour for this test to prove anything.
    let coloured: usize = fixtures().iter().map(|f| colour_operator_count(&pdftools_core::load(f).unwrap())).sum();
    assert!(coloured > 50, "fixtures contain only {coloured} colour operators");

    for_each_fixture("grayscale", |fixture, out| {
        let before = page_texts(&load(fixture)?);
        let output = out.join("gray.pdf");
        let summary = grayscale_file(fixture, true, &output).ctx("grayscale")?;
        let doc = load(&output)?;
        same_text(&before, &page_texts(&doc), "grayscale")?;
        // Rewriting must never drop drawing operations (regression: content
        // with comments used to be truncated after the first operator).
        let original = load(fixture)?;
        for n in 1..=page_count(&doc) as u32 {
            let (was, now) = (text_operator_count(&original, n), text_operator_count(&doc, n));
            ensure(now >= was, || format!("page {n} lost text: {was} text operators became {now}"))?;
        }

        if summary.skipped == 0 {
            for (id, stream) in content_streams(&doc) {
                let Ok(data) = stream.decompressed_content().or_else(|_| Ok::<_, lopdf::Error>(stream.content.clone())) else {
                    continue;
                };
                let Ok(content) = Content::decode(&data) else { continue };
                if let Some(op) = content.operations.iter().find(|op| matches!(op.operator.as_str(), "rg" | "RG" | "k" | "K")) {
                    return Err(format!("stream {id:?} still uses `{}`", op.operator));
                }
            }
        }
        for (id, stream) in images(&doc) {
            let dict = &stream.dict;
            let rgb = dict.get(b"ColorSpace").and_then(Object::as_name).ok() == Some(b"DeviceRGB".as_slice());
            let eight_bit = dict.get(b"BitsPerComponent").and_then(Object::as_i64).ok() == Some(8);
            let plain = !dict.has(b"Decode") && !dict.has(b"ImageMask");
            ensure(!(rgb && eight_bit && plain), || format!("image {id:?} is still DeviceRGB"))?;
        }
        Ok(())
    });
}

fn content_streams(doc: &Document) -> Vec<(ObjectId, &lopdf::Stream)> {
    let mut ids: HashSet<ObjectId> = doc.get_pages().into_values().flat_map(|p| doc.get_page_contents(p)).collect();
    for (id, object) in &doc.objects {
        if let Ok(stream) = object.as_stream() {
            if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Form".as_slice()) {
                ids.insert(*id);
            }
        }
    }
    ids.into_iter().filter_map(|id| Some((id, doc.get_object(id).ok()?.as_stream().ok()?))).collect()
}

fn images(doc: &Document) -> Vec<(ObjectId, &lopdf::Stream)> {
    doc.objects
        .iter()
        .filter_map(|(id, o)| {
            let stream = o.as_stream().ok()?;
            (stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image".as_slice())).then_some((*id, stream))
        })
        .collect()
}

// ── Stamps ───────────────────────────────────────────────────────────────

#[test]
fn page_numbers_appear_on_every_page() {
    for_each_fixture("numbers", |fixture, out| {
        let before = page_texts(&load(fixture)?);
        let output = out.join("numbered.pdf");
        let options = PageNumbers {
            anchor: Anchor::new(HAlign::Right, VAlign::Bottom),
            margin: 20.0,
            font_size: 9.0,
            format: "Page {n} of {total}".into(),
            start_at: 1,
            pages: None,
            color: [0.0, 0.0, 0.0],
        };
        add_page_numbers_file(fixture, &options, &output).ctx("number")?;
        let doc = load(&output)?;
        let total = before.len();
        for n in 1..=total {
            let label = format!("(Page {n} of {total})");
            ensure(page_content(&doc, n as u32).contains(&label), || format!("page {n} lacks {label}"))?;
        }
        ensure(page_count(&doc) == total, || "page count changed".into())
    });
}

#[test]
fn watermark_and_signature_are_drawn_on_the_right_pages() {
    for_each_fixture("stamps", |fixture, out| {
        let total = page_count(&load(fixture)?);

        let watermarked = out.join("watermark.pdf");
        let stamp = ImageStamp {
            image: png(120, 40),
            anchor: Anchor::new(HAlign::Center, VAlign::Middle),
            margin: 0.05,
            width: 0.5,
            rotation: 45.0,
            opacity: 0.3,
            under: false,
            pages: Some(vec![1]),
        };
        add_image_stamp_file(fixture, &stamp, &watermarked).ctx("watermark")?;
        let doc = load(&watermarked)?;
        ensure(page_content(&doc, 1).contains("/PdfToolsWm0 Do"), || "page 1 has no watermark".into())?;
        if total > 1 {
            ensure(!page_content(&doc, 2).contains("PdfToolsWm"), || "page 2 was watermarked".into())?;
        }

        let signed = out.join("signed.pdf");
        let placement = Placement { page: total, x: 0.6, y: 0.85, width: 0.3, height: 0.1 };
        add_signature_file(fixture, &png(90, 30), &[placement], &signed).ctx("sign")?;
        let doc = load(&signed)?;
        ensure(page_content(&doc, total as u32).contains("/PdfToolsSig0 Do"), || "last page not signed".into())?;

        // Stamping twice must not reuse (and so overwrite) resource names.
        let twice = out.join("twice.pdf");
        add_image_stamp_file(&watermarked, &stamp, &twice).ctx("second watermark")?;
        let content = page_content(&load(&twice)?, 1);
        ensure(content.contains("/PdfToolsWm0 Do") && content.contains("/PdfToolsWm1 Do"), || {
            "second watermark reused a resource name".into()
        })
    });
}
