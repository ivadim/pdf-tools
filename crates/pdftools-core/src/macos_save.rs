//! Safe save for sandboxed macOS apps whose Save panel grants only the output.

use std::ffi::{CStr, CString, OsStr};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use lopdf::Document;
use objc2::rc::Retained;
use objc2_foundation::{NSFileManager, NSFileManagerItemReplacementOptions, NSSearchPathDirectory, NSSearchPathDomainMask, NSURL};

fn url(path: &Path) -> Result<Retained<NSURL>, String> {
    let bytes = CString::new(path.as_os_str().as_bytes()).map_err(|_| "Invalid output path".to_string())?;
    let ptr = NonNull::new(bytes.as_ptr().cast_mut()).ok_or("Invalid output path")?;
    // SAFETY: `ptr` points to the valid, NUL-terminated CString for this call.
    Ok(unsafe { NSURL::fileURLWithFileSystemRepresentation_isDirectory_relativeToURL(ptr, false, None) })
}

fn path(url: &NSURL) -> PathBuf {
    // SAFETY: Foundation returns a NUL-terminated representation valid while
    // the URL is alive; we copy it immediately.
    let bytes = unsafe { CStr::from_ptr(url.fileSystemRepresentation().as_ptr()) }.to_bytes();
    PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec()))
}

pub fn save(doc: &mut Document, output: &Path) -> Result<(), String> {
    objc2::rc::autoreleasepool(|_| {
        let manager = NSFileManager::defaultManager();
        if output.is_dir() {
            return Err("The output is a folder".into());
        }
        let destination = url(output)?;
        let directory = manager
            .URLForDirectory_inDomain_appropriateForURL_create_error(
                NSSearchPathDirectory::ItemReplacementDirectory,
                NSSearchPathDomainMask::UserDomainMask,
                Some(&destination),
                true,
            )
            .map_err(|e| e.to_string())?;
        let staged_path = path(&directory).join(OsStr::new("output.pdf"));
        let staged_url = url(&staged_path)?;
        let mut replacing = false;
        let result = (|| {
            let file = std::fs::File::create(&staged_path).map_err(|e| e.to_string())?;
            let mut writer = std::io::BufWriter::with_capacity(1 << 20, file);
            doc.save_to(&mut writer).map_err(|e| e.to_string())?;
            writer.into_inner().map_err(|e| e.into_error().to_string())?;
            if output.exists() {
                replacing = true;
                manager
                    .replaceItemAtURL_withItemAtURL_backupItemName_options_resultingItemURL_error(
                        &destination,
                        &staged_url,
                        None,
                        NSFileManagerItemReplacementOptions(0),
                        None,
                    )
                    .map_err(|e| format!("{e}; recovery files may be in {}", path(&directory).display()))?;
            } else {
                manager.moveItemAtURL_toURL_error(&staged_url, &destination).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        // On replacement failure, retain the directory: Foundation can report
        // that the original moved there and may need recovery.
        if result.is_ok() || !replacing {
            let _ = manager.removeItemAtURL_error(&directory);
        }
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::sample;

    #[test]
    fn creates_and_replaces_pdf() {
        let folder = std::env::temp_dir().join(format!("pdftools-macos-save-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let output = folder.join("result.pdf");
        let _ = std::fs::remove_file(&output);
        save(&mut sample("one", 1), &output).unwrap();
        assert_eq!(crate::page_count(&output).unwrap(), 1);
        save(&mut sample("two", 2), &output).unwrap();
        assert_eq!(crate::page_count(&output).unwrap(), 2);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn refuses_to_replace_a_folder() {
        let folder = std::env::temp_dir().join(format!("pdftools-macos-save-folder-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let marker = folder.join("keep.txt");
        std::fs::write(&marker, b"keep").unwrap();
        assert!(save(&mut sample("one", 1), &folder).is_err());
        assert_eq!(std::fs::read(&marker).unwrap(), b"keep");
        std::fs::remove_dir_all(folder).unwrap();
    }
}
