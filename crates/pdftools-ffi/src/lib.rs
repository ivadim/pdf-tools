//! A C interface to the PDF Tools engine for the iOS app (see
//! `include/pdftools.h`), and JNI functions for the Android app
//! ([`android`]). Tools run the same commands as `pdftools-cli` and the web
//! version, in-process; pages are rendered straight into memory.

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use pdftools_render::pages::{render, DocCache, PreviewRequest};

#[cfg(target_os = "android")]
pub mod android;

/// A rendered page, owned by the caller until `pdftools_free_page`.
#[repr(C)]
pub struct PdftoolsPage {
    pub width: u32,
    pub height: u32,
    pub pixels: *mut u8,
    pub len: usize,
    pub points_width: f32,
    pub points_height: f32,
}

/// Parsed documents, one copy of each shared by all the threads rendering pages.
static DOCS: DocCache = DocCache::new();

/// Runs `f`, turning a panic into an error.
fn guarded<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| Err("the PDF engine stopped unexpectedly".into()))
}

fn into_c(text: String) -> *mut c_char {
    CString::new(text.replace('\0', " ")).expect("no NUL bytes").into_raw()
}

/// Writes `result` to `out` (the JSON or the error message); 0 on success.
///
/// # Safety
/// `out` is null or valid for writes.
unsafe fn report(result: Result<String, String>, out: *mut *mut c_char) -> i32 {
    let (status, text) = match result {
        Ok(text) => (0, text),
        Err(text) => (1, text),
    };
    if !out.is_null() {
        *out = into_c(text);
    }
    status
}

/// # Safety
/// `text` is null or a NUL-terminated string.
unsafe fn string(text: *const c_char) -> Result<String, String> {
    if text.is_null() {
        return Err("missing argument".into());
    }
    CStr::from_ptr(text).to_str().map(str::to_owned).map_err(|_| "an argument isn’t UTF-8".into())
}

fn parse_groups(text: &str, total: usize) -> Result<String, String> {
    let groups = pdftools_render::ranges::parse_groups(text, total)?;
    let json: Vec<String> = groups.iter().map(|g| format!("[{}]", g.iter().map(usize::to_string).collect::<Vec<_>>().join(","))).collect();
    Ok(format!("[{}]", json.join(",")))
}

/// Runs a `pdftools-cli` command.
///
/// # Safety
/// `argv` points to `argc` NUL-terminated strings; `out` is valid for writes.
#[no_mangle]
pub unsafe extern "C" fn pdftools_run(argv: *const *const c_char, argc: usize, out: *mut *mut c_char) -> i32 {
    let result = guarded(|| {
        if argv.is_null() || argc == 0 {
            return pdftools_cli::run(&[]);
        }
        let args = std::slice::from_raw_parts(argv, argc).iter().map(|&arg| string(arg)).collect::<Result<Vec<_>, _>>()?;
        pdftools_cli::run(&args)
    });
    report(result, out)
}

/// Renders one page into memory.
///
/// # Safety
/// `path` is a NUL-terminated string; `out` and `error` are valid for writes.
#[no_mangle]
pub unsafe extern "C" fn pdftools_render(
    path: *const c_char,
    page: u32,
    width: u32,
    rotation: i32,
    gray: bool,
    out: *mut PdftoolsPage,
    error: *mut *mut c_char,
) -> i32 {
    let result = guarded(|| {
        let request = PreviewRequest { path: PathBuf::from(string(path)?), page: page as usize, width, rotation, gray };
        render(&request, &DOCS)
    });
    match result {
        Ok(rendered) if !out.is_null() => {
            let mut pixels = rendered.pixels.into_boxed_slice();
            *out = PdftoolsPage {
                width: rendered.width,
                height: rendered.height,
                len: pixels.len(),
                pixels: pixels.as_mut_ptr(),
                points_width: rendered.page_size.0,
                points_height: rendered.page_size.1,
            };
            std::mem::forget(pixels);
            0
        }
        Ok(_) => report(Err("missing page".into()), error),
        Err(message) => report(Err(message), error),
    }
}

/// Caps the threads tools use for images and streams (see
/// `pdftools_core::set_max_threads`); fewer saves memory.
#[no_mangle]
pub extern "C" fn pdftools_set_threads(threads: usize) {
    pdftools_core::set_max_threads(threads);
}

/// Drops a cached document.
///
/// # Safety
/// `path` is null or a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn pdftools_forget(path: *const c_char) {
    if let Ok(path) = string(path) {
        DOCS.forget(Path::new(&path));
    }
}

/// Parses page ranges into JSON groups.
///
/// # Safety
/// `text` is a NUL-terminated string; `out` is valid for writes.
#[no_mangle]
pub unsafe extern "C" fn pdftools_parse_groups(text: *const c_char, total: usize, out: *mut *mut c_char) -> i32 {
    let result = guarded(|| parse_groups(&string(text)?, total));
    report(result, out)
}

/// Formats pages compactly, e.g. [1, 2, 3, 5] → "1-3, 5".
///
/// # Safety
/// `pages` points to `count` values (or is null when `count` is 0).
#[no_mangle]
pub unsafe extern "C" fn pdftools_format_list(pages: *const usize, count: usize) -> *mut c_char {
    let pages = if pages.is_null() || count == 0 { &[][..] } else { std::slice::from_raw_parts(pages, count) };
    into_c(pdftools_render::ranges::format_list(pages))
}

/// # Safety
/// `string` is null or came from this library and wasn't freed yet.
#[no_mangle]
pub unsafe extern "C" fn pdftools_free_string(string: *mut c_char) {
    if !string.is_null() {
        drop(CString::from_raw(string));
    }
}

/// # Safety
/// `page` is null or was filled by `pdftools_render` and wasn't freed yet.
#[no_mangle]
pub unsafe extern "C" fn pdftools_free_page(page: *mut PdftoolsPage) {
    if let Some(page) = page.as_mut() {
        if !page.pixels.is_null() {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(page.pixels, page.len)));
        }
        page.pixels = std::ptr::null_mut();
        page.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> CString {
        let repo =
            if std::env::var_os("TEST_SRCDIR").is_some() { PathBuf::new() } else { Path::new(env!("CARGO_MANIFEST_DIR")).join("../..") };
        let path = std::fs::canonicalize(repo.join("tests/fixtures/pdfjs").join(name)).unwrap();
        CString::new(path.to_str().unwrap()).unwrap()
    }

    unsafe fn take(text: *mut c_char) -> String {
        let owned = CStr::from_ptr(text).to_str().unwrap().to_owned();
        pdftools_free_string(text);
        owned
    }

    #[test]
    fn runs_commands() {
        let path = fixture("tracemonkey.pdf");
        let info = CString::new("info").unwrap();
        let argv = [info.as_ptr(), path.as_ptr()];
        let mut out = std::ptr::null_mut();
        unsafe {
            assert_eq!(pdftools_run(argv.as_ptr(), argv.len(), &mut out), 0);
            assert!(take(out).contains(r#""pages":14"#));
            let bad = CString::new("frobnicate").unwrap();
            assert_eq!(pdftools_run([bad.as_ptr()].as_ptr(), 1, &mut out), 1);
            assert!(take(out).contains("unknown command"));
        }
    }

    #[test]
    fn renders_pages() {
        let path = fixture("sizes.pdf");
        let mut page = PdftoolsPage { width: 0, height: 0, pixels: std::ptr::null_mut(), len: 0, points_width: 0.0, points_height: 0.0 };
        let mut error = std::ptr::null_mut();
        unsafe {
            assert_eq!(pdftools_render(path.as_ptr(), 1, 120, 90, false, &mut page, &mut error), 0);
            assert_eq!(page.len, (page.width * page.height * 4) as usize);
            assert!(page.points_width > 0.0);
            pdftools_free_page(&mut page);
            assert!(page.pixels.is_null());
            assert_eq!(pdftools_render(path.as_ptr(), 999, 120, 0, false, &mut page, &mut error), 1);
            assert!(take(error).contains("does not exist"));
            pdftools_forget(path.as_ptr());
        }
    }

    #[test]
    fn formats_page_lists() {
        unsafe {
            let pages = [5usize, 1, 2, 3, 3];
            assert_eq!(take(pdftools_format_list(pages.as_ptr(), pages.len())), "1-3, 5");
            assert_eq!(take(pdftools_format_list(std::ptr::null(), 0)), "");
        }
    }

    #[test]
    fn parses_ranges() {
        let mut out = std::ptr::null_mut();
        unsafe {
            let text = CString::new("1-3; 5, 8-").unwrap();
            assert_eq!(pdftools_parse_groups(text.as_ptr(), 9, &mut out), 0);
            assert_eq!(take(out), "[[1,2,3],[5],[8,9]]");
            let text = CString::new("4-2").unwrap();
            assert_eq!(pdftools_parse_groups(text.as_ptr(), 9, &mut out), 1);
            assert!(take(out).contains("backwards"));
        }
    }
}
