//! Page images rendered natively with hayro, with a small cache of parsed
//! documents.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::peniko::ImageAlphaType;
use hayro::{PixmapSettings, RenderCache, RenderSettings};

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewRequest {
    pub path: PathBuf,
    /// 1-based.
    pub page: usize,
    /// Target width in pixels.
    pub width: u32,
    /// Extra clockwise rotation (multiple of 90).
    pub rotation: i32,
    pub gray: bool,
}

#[derive(Debug, Clone)]
pub struct Rendered {
    pub width: u32,
    pub height: u32,
    /// Premultiplied RGBA.
    pub pixels: Vec<u8>,
    /// Displayed page size in points, before any extra rotation.
    pub page_size: (f32, f32),
}

impl Rendered {
    /// Width / height as displayed.
    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height.max(1) as f32
    }
}

/// Parsed documents, most recently used last. Thread-safe: each file is
/// read and parsed once (threads that need it meanwhile wait for that load)
/// and shared by every thread rendering from it; pages render outside the lock.
pub struct DocCache {
    docs: Mutex<Vec<Entry>>,
    /// Bytes of PDF data kept; the newest document is kept even if larger.
    limit: usize,
}

struct Entry {
    path: PathBuf,
    doc: Arc<OnceLock<Result<Arc<Pdf>, String>>>,
}

impl Entry {
    /// The document's size, once it's loaded.
    fn bytes(&self) -> usize {
        match self.doc.get() {
            Some(Ok(pdf)) => pdf.data().as_ref().len(),
            _ => 0,
        }
    }
}

/// At most this many documents are kept…
const CACHED_DOCS: usize = 6;
/// …and at most this much of their data (on iOS mapped files' pages can be
/// dropped by the system, but desktops hold whole files in memory).
pub const CACHED_BYTES: usize = 512 << 20;

impl Default for DocCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DocCache {
    pub const fn new() -> Self {
        Self::with_limit(CACHED_BYTES)
    }

    /// A cache keeping at most `limit` bytes of PDF data.
    pub const fn with_limit(limit: usize) -> Self {
        Self { docs: Mutex::new(Vec::new()), limit }
    }

    fn docs(&self) -> MutexGuard<'_, Vec<Entry>> {
        self.docs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn get(&self, path: &Path) -> Result<Arc<Pdf>, String> {
        let slot = {
            let mut docs = self.docs();
            let entry = match docs.iter().position(|e| e.path == path) {
                Some(i) => docs.remove(i),
                None => Entry { path: path.to_path_buf(), doc: Arc::new(OnceLock::new()) },
            };
            let slot = Arc::clone(&entry.doc);
            docs.push(entry);
            slot
        };
        // Loaded without the lock, so other documents keep rendering; threads
        // that want this one meanwhile wait here for the same load.
        match slot.get_or_init(|| load(path).map(Arc::new)) {
            Ok(pdf) => {
                let pdf = Arc::clone(pdf);
                self.trim();
                Ok(pdf)
            }
            Err(message) => {
                // Not kept, so a later request tries again.
                self.docs().retain(|e| !Arc::ptr_eq(&e.doc, &slot));
                Err(message.clone())
            }
        }
    }

    /// Drops the least recently used documents beyond the limits.
    fn trim(&self) {
        let mut docs = self.docs();
        while docs.len() > 1 && (docs.len() > CACHED_DOCS || docs.iter().map(Entry::bytes).sum::<usize>() > self.limit) {
            docs.remove(0);
        }
    }

    /// Drops a document, e.g. after its file was overwritten. Pages still
    /// rendering from it finish first.
    pub fn forget(&self, path: &Path) {
        self.docs().retain(|e| e.path != path);
    }
}

fn load(path: &Path) -> Result<Pdf, String> {
    let bytes = pdftools_core::read_file(path).map_err(|e| e.to_string())?;
    // A damaged file is shown as the pages the tools can recover from it.
    match Pdf::new(Arc::new(bytes)) {
        Ok(pdf) => Ok(pdf),
        Err(e) => std::fs::read(path)
            .ok()
            .and_then(|data| pdftools_core::repaired_bytes(&data))
            .and_then(|repaired| Pdf::new(repaired).ok())
            .ok_or_else(|| format!("cannot read PDF: {e:?}")),
    }
}

/// Renders one page; the core of the preview system.
pub fn render(request: &PreviewRequest, docs: &DocCache) -> Result<Rendered, String> {
    let pdf = docs.get(&request.path)?;
    let pages = pdf.pages();
    let page = pages.get(request.page.wrapping_sub(1)).ok_or_else(|| format!("page {} does not exist", request.page))?;
    let (page_w, page_h) = page.render_dimensions();
    // Scale so the displayed width (after extra rotation) matches the request.
    let quarter_turn = request.rotation.rem_euclid(180) == 90;
    let displayed_w = if quarter_turn { page_h } else { page_w };
    let scale = (request.width as f32 / displayed_w.max(1.0)).clamp(0.01, 8.0);
    let pixmap = hayro::render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE },
    );
    let (mut width, mut height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    // The renderer's own buffer, without a copy (pages are opaque, so
    // premultiplied is the same as plain RGBA).
    let mut pixels = pixmap.take_rgba8(ImageAlphaType::AlphaPremultiplied);
    if request.rotation.rem_euclid(360) != 0 {
        (pixels, width, height) = rotate(&pixels, width, height, request.rotation);
    }
    if request.gray {
        to_gray(&mut pixels);
    }
    Ok(Rendered { width, height, pixels, page_size: (page_w, page_h) })
}

/// Rotates RGBA pixels clockwise by a multiple of 90 degrees.
pub fn rotate(pixels: &[u8], width: u32, height: u32, degrees: i32) -> (Vec<u8>, u32, u32) {
    let (w, h) = (width as usize, height as usize);
    let turns = (degrees.rem_euclid(360) / 90) as usize;
    if turns == 0 {
        return (pixels.to_vec(), width, height);
    }
    let (out_w, out_h) = if turns % 2 == 1 { (h, w) } else { (w, h) };
    let mut out = vec![0u8; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match turns {
                1 => (h - 1 - y, x),
                2 => (w - 1 - x, h - 1 - y),
                _ => (y, w - 1 - x),
            };
            let src = (y * w + x) * 4;
            let dst = (ny * out_w + nx) * 4;
            out[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
        }
    }
    (out, out_w as u32, out_h as u32)
}

pub fn to_gray(pixels: &mut [u8]) {
    for px in pixels.as_chunks_mut::<4>().0 {
        let luma = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32).round() as u8;
        px[0] = luma;
        px[1] = luma;
        px[2] = luma;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        // Bazel runs tests from the workspace's runfiles, Cargo from the crate.
        let repo =
            if std::env::var_os("TEST_SRCDIR").is_some() { PathBuf::new() } else { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") };
        repo.join("tests/fixtures/pdfjs").join(name)
    }

    #[test]
    fn renders_pages_at_the_requested_width() {
        let docs = DocCache::default();
        let request = PreviewRequest { path: fixture("sizes.pdf"), page: 1, width: 200, rotation: 0, gray: false };
        let page = render(&request, &docs).unwrap();
        assert!((page.width as i32 - 200).abs() <= 1, "{}", page.width);
        assert_eq!(page.pixels.len(), (page.width * page.height * 4) as usize);
        assert!(page.page_size.0 > 0.0);
    }

    #[test]
    fn honours_page_rotation_and_extra_rotation() {
        let docs = DocCache::default();
        // rotation.pdf: page 2 has /Rotate 90.
        let base = PreviewRequest { path: fixture("rotation.pdf"), page: 2, width: 100, rotation: 0, gray: false };
        let page = render(&base, &docs).unwrap();
        let turned = render(&PreviewRequest { rotation: 90, ..base.clone() }, &docs).unwrap();
        assert!((page.aspect() - 1.0 / turned.aspect()).abs() < 0.05);
        assert!(render(&PreviewRequest { page: 9, ..base.clone() }, &docs).is_err());
        assert!(render(&PreviewRequest { path: fixture("missing.pdf"), ..base }, &docs).is_err());
    }

    #[test]
    fn makes_grayscale_previews() {
        let docs = DocCache::default();
        let request = PreviewRequest { path: fixture("calrgb.pdf"), page: 1, width: 80, rotation: 0, gray: true };
        let page = render(&request, &docs).unwrap();
        assert!(page.pixels.as_chunks::<4>().0.iter().all(|p| p[0] == p[1] && p[1] == p[2]));
    }

    #[test]
    fn rotates_pixels() {
        // 2×1: red, blue.
        let pixels = [255, 0, 0, 255, 0, 0, 255, 255];
        let (out, w, h) = rotate(&pixels, 2, 1, 90);
        assert_eq!((w, h), (1, 2));
        assert_eq!(&out[..4], &[255, 0, 0, 255]);
        let (out, _, _) = rotate(&pixels, 2, 1, 180);
        assert_eq!(&out[..4], &[0, 0, 255, 255]);
        let (out, w, _) = rotate(&pixels, 2, 1, -90);
        assert_eq!(w, 1);
        assert_eq!(&out[..4], &[0, 0, 255, 255]);
        assert_eq!(rotate(&pixels, 2, 1, 360).0, pixels);
    }

    #[test]
    fn evicts_old_documents() {
        let docs = DocCache::default();
        for name in ["sizes.pdf", "rotation.pdf", "canvas.pdf", "issue5010.pdf", "devicen.pdf", "alphatrans.pdf", "calrgb.pdf"] {
            docs.get(&fixture(name)).unwrap();
        }
        assert_eq!(docs.docs().len(), CACHED_DOCS);
        docs.forget(&fixture("calrgb.pdf"));
        assert_eq!(docs.docs().len(), CACHED_DOCS - 1);
    }

    #[test]
    fn shares_one_copy_of_each_document_between_threads() {
        let docs = DocCache::new();
        let request = PreviewRequest { path: fixture("tracemonkey.pdf"), page: 1, width: 120, rotation: 0, gray: false };
        let pages: Vec<Rendered> = std::thread::scope(|scope| {
            let workers: Vec<_> = (1..=6)
                .map(|page| {
                    let (docs, request) = (&docs, PreviewRequest { page, ..request.clone() });
                    scope.spawn(move || render(&request, docs).unwrap())
                })
                .collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        assert!(pages.iter().all(|p| p.pixels.len() == (p.width * p.height * 4) as usize));
        assert_eq!(docs.docs().len(), 1);
    }

    #[test]
    fn loads_each_document_once_when_threads_ask_together() {
        let docs = DocCache::new();
        let path = fixture("tracemonkey.pdf");
        let copies: Vec<Arc<Pdf>> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..6).map(|_| scope.spawn(|| docs.get(&path).unwrap())).collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        assert!(copies.iter().all(|pdf| Arc::ptr_eq(pdf, &copies[0])), "one shared copy");
    }

    #[test]
    fn keeps_documents_within_the_size_limit() {
        let size = |name: &str| std::fs::metadata(fixture(name)).unwrap().len() as usize;
        let docs = DocCache::with_limit(size("sizes.pdf") + size("rotation.pdf"));
        for name in ["tracemonkey.pdf", "sizes.pdf", "rotation.pdf"] {
            docs.get(&fixture(name)).unwrap();
        }
        let kept: Vec<_> = docs.docs().iter().map(|e| e.path.file_name().unwrap().to_owned()).collect();
        assert_eq!(kept, ["sizes.pdf", "rotation.pdf"]);
        // The newest is kept even when it alone is over the limit.
        let tiny = DocCache::with_limit(1);
        tiny.get(&fixture("tracemonkey.pdf")).unwrap();
        assert_eq!(tiny.docs().len(), 1);
        // Files that fail to load aren't kept.
        assert!(tiny.get(&fixture("missing.pdf")).is_err());
        assert_eq!(tiny.docs().len(), 1);
    }
}
