//! Page previews for the Slint app.
//!
//! A single worker thread keeps recently used documents parsed and serves
//! requests in priority order (visible pages first). Results are delivered
//! to the UI thread through [`jobs`](crate::jobs).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

pub use pdftools_render::pages::{render, rotate, DocCache, PreviewRequest, Rendered};

use crate::app::App;
use crate::jobs;
use crate::ToImage;

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

