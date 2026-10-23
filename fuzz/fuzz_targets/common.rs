use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Writes `data` to a file of this process's own and returns its path: the
/// core and the renderer open PDFs by path, as the app does.
pub fn as_file(data: &[u8]) -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    let path = PATH.get_or_init(|| std::env::temp_dir().join(format!("pdftools-fuzz-{}.pdf", std::process::id())));
    std::fs::write(path, data).expect("write the input to a temporary file");
    path
}
