//! JNI functions behind the Android app's `io.github.ivadim.pdftools.Engine`
//! (android/java/io/github/ivadim/pdftools/Engine.kt). Errors are
//! thrown as `EngineException`.

use std::path::{Path, PathBuf};

use jni::errors::ThrowRuntimeExAndDefault;
use jni::objects::JIntArray;
use jni::objects::{JClass, JObject, JObjectArray, JString};
use jni::strings::JNIString;
use jni::sys::{jboolean, jint, jobject, jstring, JNI_FALSE};
use jni::{jni_sig, jni_str, Env, EnvUnowned};
use pdftools_render::pages::{render, PreviewRequest};

use super::{guarded, parse_groups, DOCS};

fn text(env: &mut Env, string: &JString) -> Result<String, String> {
    string.try_to_string(env).map_err(|e| e.to_string())
}

fn throw(env: &mut Env, message: String) {
    let _ = env.throw_new(jni_str!("io/github/ivadim/pdftools/EngineException"), JNIString::new(message));
}

/// Returns `result` as a Java string, or throws its error and returns null.
fn finish(env: &mut Env, result: Result<String, String>) -> jstring {
    match result.and_then(|text| env.new_string(text).map_err(|e| e.to_string())) {
        Ok(string) => string.into_raw(),
        Err(message) => {
            throw(env, message);
            std::ptr::null_mut()
        }
    }
}

/// `Engine.nativeRun(args)`: a `pdftools-cli` command's JSON result.
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeRun<'l>(
    mut unowned_env: EnvUnowned<'l>,
    _: JClass<'l>,
    args: JObjectArray<'l, JString<'l>>,
) -> jstring {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = (|| {
                let count = args.len(env).map_err(|e| e.to_string())?;
                let mut strings = Vec::with_capacity(count);
                for i in 0..count {
                    let item = args.get_element(env, i).map_err(|e| e.to_string())?;
                    strings.push(text(env, &item)?);
                }
                guarded(|| pdftools_cli::run(&strings))
            })();
            Ok(finish(env, result))
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

/// `Engine.nativeRender(path, page, width, rotation, gray)`: a `NativePage`.
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeRender<'l>(
    mut unowned_env: EnvUnowned<'l>,
    _: JClass<'l>,
    path: JString<'l>,
    page: jint,
    width: jint,
    rotation: jint,
    gray: jboolean,
) -> jobject {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = text(env, &path)
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
                    env.new_object(jni_str!("io/github/ivadim/pdftools/NativePage"), jni_sig!("(IIFF[B)V"), &args)
                        .map_err(|e| e.to_string())
                });
            Ok(match result {
                Ok(page) => page.into_raw(),
                Err(message) => {
                    throw(env, message);
                    std::ptr::null_mut()
                }
            })
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

/// `Engine.nativeForget(path)`: drops a cached document, e.g. after its file changed.
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeForget<'l>(
    mut unowned_env: EnvUnowned<'l>,
    _: JClass<'l>,
    path: JString<'l>,
) {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            if let Ok(path) = text(env, &path) {
                DOCS.forget(Path::new(&path));
            }
            Ok(())
        })
        .resolve::<ThrowRuntimeExAndDefault>();
}

/// `Engine.nativeParseGroups(text, total)`: page ranges as JSON groups.
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeParseGroups<'l>(
    mut unowned_env: EnvUnowned<'l>,
    _: JClass<'l>,
    ranges: JString<'l>,
    total: jint,
) -> jstring {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = text(env, &ranges).and_then(|ranges| guarded(|| parse_groups(&ranges, total.max(0) as usize)));
            Ok(finish(env, result))
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}

/// `Engine.nativeSetThreads(threads)`: caps the threads tools use for images
/// and streams; fewer saves memory.
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeSetThreads<'l>(_: EnvUnowned<'l>, _: JClass<'l>, threads: jint) {
    pdftools_core::set_max_threads(threads.max(1) as usize);
}

/// `Engine.nativeFormatList(pages)`: [1, 2, 3, 5] → "1-3, 5".
#[no_mangle]
pub extern "system" fn Java_io_github_ivadim_pdftools_Engine_nativeFormatList<'l>(
    mut unowned_env: EnvUnowned<'l>,
    _: JClass<'l>,
    pages: JIntArray<'l>,
) -> jstring {
    unowned_env
        .with_env(|env| -> jni::errors::Result<_> {
            let result = (|| {
                let count = pages.len(env).map_err(|e| e.to_string())?;
                let mut values = vec![0; count];
                pages.get_region(env, 0, &mut values).map_err(|e| e.to_string())?;
                let values: Vec<usize> = values.into_iter().map(|v| v.max(0) as usize).collect();
                Ok(pdftools_render::ranges::format_list(&values))
            })();
            Ok(finish(env, result))
        })
        .resolve::<ThrowRuntimeExAndDefault>()
}
