//! Page thumbnails: rendered in the background, kept once ready.
//!
//! Renders share one cache of parsed documents behind a lock, so they run
//! one at a time (in the order they were asked for, visible pages first)
//! without parsing a file twice.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use iced::widget::image::Handle;
use pdftools_render::pages::{render, DocCache, PreviewRequest};

/// A rendered page, ready to show.
#[derive(Debug, Clone)]
pub struct Thumb {
    pub handle: Handle,
    /// Width / height as displayed.
    pub aspect: f32,
    /// Page size in points, before any extra rotation.
    pub page_size: (f32, f32),
}

pub fn key(request: &PreviewRequest) -> String {
    format!("{}|{}|{}|{}|{}", request.path.display(), request.page, request.width, request.rotation, request.gray)
}

#[derive(Default)]
pub struct Previews {
    docs: Arc<Mutex<DocCache>>,
    ready: HashMap<String, Thumb>,
    pending: HashSet<String>,
    failed: HashSet<String>,
}

impl Previews {
    pub fn get(&self, request: &PreviewRequest) -> Option<&Thumb> {
        self.ready.get(&key(request))
    }

    pub fn failed(&self, request: &PreviewRequest) -> bool {
        self.failed.contains(&key(request))
    }

    /// The work to render `request`, or `None` if it's done or under way.
    pub fn start(&mut self, request: PreviewRequest) -> Option<impl FnOnce() -> (String, Result<Thumb, String>) + Send> {
        let key = key(&request);
        if self.ready.contains_key(&key) || self.failed.contains(&key) || !self.pending.insert(key.clone()) {
            return None;
        }
        let docs = self.docs.clone();
        Some(move || {
            let mut docs = docs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let result = render(&request, &mut docs).map(|page| Thumb {
                aspect: page.aspect(),
                page_size: page.page_size,
                // Pages are opaque, so premultiplied and straight alpha agree.
                handle: Handle::from_rgba(page.width, page.height, page.pixels),
            });
            (key, result)
        })
    }

    pub fn finished(&mut self, key: String, result: Result<Thumb, String>) {
        self.pending.remove(&key);
        match result {
            Ok(thumb) => {
                self.ready.insert(key, thumb);
            }
            Err(_) => {
                self.failed.insert(key);
            }
        }
    }

    /// Forgets everything about `path` (its file changed).
    pub fn forget(&mut self, path: &Path) {
        let prefix = format!("{}|", path.display());
        self.ready.retain(|k, _| !k.starts_with(&prefix));
        self.failed.retain(|k| !k.starts_with(&prefix));
        self.docs.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).forget(path);
    }
}
