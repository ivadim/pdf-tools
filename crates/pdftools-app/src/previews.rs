//! Page previews rendered natively with hayro.
//!
//! A single worker thread keeps recently used documents parsed and serves
//! requests in priority order (visible pages first). Results are delivered
//! to the UI thread through [`jobs`](crate::jobs).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};

use crate::app::App;
use crate::jobs;

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
    pub fn to_image(&self) -> slint::Image {
        slint::Image::from_rgba8_premultiplied(slint::SharedPixelBuffer::clone_from_slice(
            &self.pixels,
            self.width,
            self.height,
        ))
    }

    /// Width / height as displayed.
    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height.max(1) as f32
    }
}

/// Parsed documents, most recently used last.
#[derive(Default)]
pub struct DocCache {
    docs: Vec<(PathBuf, Pdf)>,
}

const CACHED_DOCS: usize = 6;

impl DocCache {
    fn get(&mut self, path: &Path) -> Result<&Pdf, String> {
        if let Some(i) = self.docs.iter().position(|(p, _)| p == path) {
            let entry = self.docs.remove(i);
            self.docs.push(entry);
        } else {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let pdf = Pdf::new(data).map_err(|e| format!("cannot read PDF: {e:?}"))?;
            if self.docs.len() >= CACHED_DOCS {
                self.docs.remove(0);
            }
            self.docs.push((path.to_path_buf(), pdf));
        }
        Ok(&self.docs.last().expect("just inserted").1)
    }

    /// Drops a document, e.g. after its file was overwritten.
    pub fn forget(&mut self, path: &Path) {
        self.docs.retain(|(p, _)| p != path);
    }
}

/// Renders one page; the core of the preview system.
pub fn render(request: &PreviewRequest, docs: &mut DocCache) -> Result<Rendered, String> {
    let pdf = docs.get(&request.path)?;
    let pages = pdf.pages();
    let page = pages
        .get(request.page.wrapping_sub(1))
        .ok_or_else(|| format!("page {} does not exist", request.page))?;
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
    let mut pixels = pixmap.data_as_u8_slice().to_vec();
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

fn to_gray(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        let luma = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32).round() as u8;
        px[0] = luma;
        px[1] = luma;
        px[2] = luma;
    }
}

type Job = (PreviewRequest, u64);

struct Queue {
    jobs: Mutex<(VecDeque<Job>, Vec<PathBuf>)>,
    ready: Condvar,
}

/// The preview service owned by the app.
pub struct Previews {
    queue: Option<Arc<Queue>>,
    /// Used in inline (test) mode.
    local: DocCache,
    /// Rendered images by request, so revisiting a page is instant.
    cache: HashMap<String, (slint::Image, f32, (f32, f32))>,
}

impl Default for Previews {
    fn default() -> Self {
        Self::new()
    }
}

impl Previews {
    pub fn new() -> Self {
        let queue = (!jobs::is_inline()).then(|| {
            let queue = Arc::new(Queue { jobs: Mutex::new((VecDeque::new(), Vec::new())), ready: Condvar::new() });
            let worker = queue.clone();
            std::thread::Builder::new()
                .name("previews".into())
                .spawn(move || run_worker(&worker))
                .expect("start preview thread");
            queue
        });
        Self { queue, local: DocCache::default(), cache: HashMap::new() }
    }

    fn key(request: &PreviewRequest) -> String {
        format!("{}|{}|{}|{}|{}", request.path.display(), request.page, request.width, request.rotation, request.gray)
    }

    /// A previously rendered image for `request`, if any.
    pub fn cached(&self, request: &PreviewRequest) -> Option<(slint::Image, f32, (f32, f32))> {
        self.cache.get(&Self::key(request)).cloned()
    }

    /// Renders in the background; `then` receives the image, its aspect
    /// ratio and the page size in points. `urgent` requests jump the queue.
    pub fn request(
        &mut self,
        request: PreviewRequest,
        urgent: bool,
        then: impl FnOnce(&mut App, Result<(slint::Image, f32, (f32, f32)), String>) + 'static,
    ) {
        if let Some(hit) = self.cached(&request) {
            // Delivered later like any result: the caller holds the app borrowed.
            let id = jobs::continuation(move |app: &mut App, (): ()| then(app, Ok(hit)));
            jobs::post(id, Box::new(()));
            return;
        }
        let key = Self::key(&request);
        let id = jobs::continuation(move |app: &mut App, result: Result<Rendered, String>| {
            let result = result.map(|r| {
                let image = r.to_image();
                app.s.previews.cache.insert(key, (image.clone(), r.aspect(), r.page_size));
                (image, r.aspect(), r.page_size)
            });
            then(app, result);
        });
        match &self.queue {
            Some(queue) => {
                let mut jobs = queue.jobs.lock().expect("preview queue");
                if urgent {
                    jobs.0.push_front((request, id));
                } else {
                    jobs.0.push_back((request, id));
                }
                queue.ready.notify_one();
            }
            None => {
                let result = render(&request, &mut self.local);
                jobs::post(id, Box::new(result));
            }
        }
    }

    /// Forgets everything about `path` (its file changed).
    pub fn forget(&mut self, path: &Path) {
        let prefix = format!("{}|", path.display());
        self.cache.retain(|k, _| !k.starts_with(&prefix));
        self.local.forget(path);
        if let Some(queue) = &self.queue {
            queue.jobs.lock().expect("preview queue").1.push(path.to_path_buf());
        }
    }
}

fn run_worker(queue: &Queue) {
    let mut docs = DocCache::default();
    loop {
        let (request, id) = {
            let mut jobs = queue.jobs.lock().expect("preview queue");
            loop {
                for path in std::mem::take(&mut jobs.1) {
                    docs.forget(&path);
                }
                if let Some(job) = jobs.0.pop_front() {
                    break job;
                }
                jobs = queue.ready.wait(jobs).expect("preview queue");
            }
        };
        let result = render(&request, &mut docs);
        jobs::post(id, Box::new(result));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdfjs").join(name)
    }

    #[test]
    fn renders_pages_at_the_requested_width() {
        let mut docs = DocCache::default();
        let request = PreviewRequest { path: fixture("sizes.pdf"), page: 1, width: 200, rotation: 0, gray: false };
        let page = render(&request, &mut docs).unwrap();
        assert!((page.width as i32 - 200).abs() <= 1, "{}", page.width);
        assert_eq!(page.pixels.len(), (page.width * page.height * 4) as usize);
        assert!(page.page_size.0 > 0.0);
    }

    #[test]
    fn honours_page_rotation_and_extra_rotation() {
        let mut docs = DocCache::default();
        // rotation.pdf: page 2 has /Rotate 90.
        let base = PreviewRequest { path: fixture("rotation.pdf"), page: 2, width: 100, rotation: 0, gray: false };
        let page = render(&base, &mut docs).unwrap();
        let turned = render(&PreviewRequest { rotation: 90, ..base.clone() }, &mut docs).unwrap();
        assert!((page.aspect() - 1.0 / turned.aspect()).abs() < 0.05);
        assert!(render(&PreviewRequest { page: 9, ..base.clone() }, &mut docs).is_err());
        assert!(render(&PreviewRequest { path: fixture("missing.pdf"), ..base }, &mut docs).is_err());
    }

    #[test]
    fn makes_grayscale_previews() {
        let mut docs = DocCache::default();
        let request = PreviewRequest { path: fixture("calrgb.pdf"), page: 1, width: 80, rotation: 0, gray: true };
        let page = render(&request, &mut docs).unwrap();
        assert!(page.pixels.chunks_exact(4).all(|p| p[0] == p[1] && p[1] == p[2]));
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
        let mut docs = DocCache::default();
        for name in ["sizes.pdf", "rotation.pdf", "canvas.pdf", "issue5010.pdf", "devicen.pdf", "alphatrans.pdf", "calrgb.pdf"] {
            docs.get(&fixture(name)).unwrap();
        }
        assert_eq!(docs.docs.len(), CACHED_DOCS);
        docs.forget(&fixture("calrgb.pdf"));
        assert_eq!(docs.docs.len(), CACHED_DOCS - 1);
    }
}

