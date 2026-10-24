//! JNI functions behind the Android app's `io.github.ivadim.pdftools.Engine`
//! (android/java/io/github/ivadim/pdftools/Engine.kt). Errors are
//! thrown as `EngineException`.

use std::path::{Path, PathBuf};

use jni::objects::JIntArray;
use jni::objects::{JClass, JObject, JObjectArray, JString};
use jni::sys::{jboolean, jint, jobject, jstring, JNI_FALSE};
use jni::JNIEnv;
use pdftools_render::pages::{render, PreviewRequest};

use super::{guarded, parse_groups, DOCS};

const EXCEPTION: &str = "io/github/ivadim/pdftools/EngineException";
const PAGE: &str = "io/github/ivadim/pdftools/NativePage";

fn text(env: &mut JNIEnv, string: &JString) -> Result<String, String> {
    env.get_string(string).map(Into::into).map_err(|e| e.to_string())
}

/// Returns `result` as a Java string, or throws its error and returns null.
fn finish(env: &mut JNIEnv, result: Result<String, String>) -> jstring {
    match result.and_then(|text| env.new_string(text).map_err(|e| e.to_string())) {
        Ok(string) => string.into_raw(),
        Err(message) => {
            let _ = env.throw_new(EXCEPTION, message);
            std::ptr::null_mut()
        }
    }
}

/// `Engine.nativeRun(args)`: a `pdftools-cli` command's JSON result.
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeRun<'l>(
    mut env: JNIEnv<'l>,
    _: JClass<'l>,
    args: JObjectArray<'l>,
) -> jstring {
    let result = (|| {
        let count = env.get_array_length(&args).map_err(|e| e.to_string())?;
        let mut strings = Vec::with_capacity(count as usize);
        for i in 0..count {
            let item = JString::from(env.get_object_array_element(&args, i).map_err(|e| e.to_string())?);
            strings.push(text(&mut env, &item)?);
        }
        guarded(|| pdftools_cli::run(&strings))
    })();
    finish(&mut env, result)
}

/// `Engine.nativeRender(path, page, width, rotation, gray)`: a `NativePage`.
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeRender<'l>(
    mut env: JNIEnv<'l>,
    _: JClass<'l>,
    path: JString<'l>,
    page: jint,
    width: jint,
    rotation: jint,
    gray: jboolean,
) -> jobject {
    let result = text(&mut env, &path)
        .and_then(|path| {
            let request = PreviewRequest {
                path: PathBuf::from(path),
                page: page.max(0) as usize,
                width: width.max(1) as u32,
                rotation,
                gray: gray != JNI_FALSE,
            };
            guarded(|| render(&request, &DOCS))
        })
        .and_then(|rendered| {
            let pixels = JObject::from(env.byte_array_from_slice(&rendered.pixels).map_err(|e| e.to_string())?);
            let args = [
                (rendered.width as jint).into(),
                (rendered.height as jint).into(),
                rendered.page_size.0.into(),
                rendered.page_size.1.into(),
                (&pixels).into(),
            ];
            env.new_object(PAGE, "(IIFF[B)V", &args).map_err(|e| e.to_string())
        });
    match result {
        Ok(page) => page.into_raw(),
        Err(message) => {
            let _ = env.throw_new(EXCEPTION, message);
            std::ptr::null_mut()
        }
    }
}

/// `Engine.nativeForget(path)`: drops a cached document, e.g. after its file changed.
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeForget<'l>(mut env: JNIEnv<'l>, _: JClass<'l>, path: JString<'l>) {
    if let Ok(path) = text(&mut env, &path) {
        DOCS.forget(Path::new(&path));
    }
}

/// `Engine.nativeParseGroups(text, total)`: page ranges as JSON groups.
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeParseGroups<'l>(
    mut env: JNIEnv<'l>,
    _: JClass<'l>,
    ranges: JString<'l>,
    total: jint,
) -> jstring {
    let result = text(&mut env, &ranges).and_then(|ranges| guarded(|| parse_groups(&ranges, total.max(0) as usize)));
    finish(&mut env, result)
}

/// `Engine.nativeSetThreads(threads)`: caps the threads tools use for images
/// and streams; fewer saves memory.
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeSetThreads<'l>(_: JNIEnv<'l>, _: JClass<'l>, threads: jint) {
    pdftools_core::set_max_threads(threads.max(1) as usize);
}

/// `Engine.nativeFormatList(pages)`: [1, 2, 3, 5] → "1-3, 5".
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeFormatList<'l>(
    mut env: JNIEnv<'l>,
    _: JClass<'l>,
    pages: JIntArray<'l>,
) -> jstring {
    let result = (|| {
        let count = env.get_array_length(&pages).map_err(|e| e.to_string())? as usize;
        let mut values = vec![0; count];
        env.get_int_array_region(&pages, 0, &mut values).map_err(|e| e.to_string())?;
        let values: Vec<usize> = values.into_iter().map(|v| v.max(0) as usize).collect();
        Ok(pdftools_render::ranges::format_list(&values))
    })();
    finish(&mut env, result)
}
