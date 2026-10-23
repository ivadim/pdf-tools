use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no input files were given")]
    NoInput,

    #[error("the selected files contain no pages")]
    NoPages,

    #[error("page {page} of input {input} does not exist")]
    InvalidPage { input: usize, page: usize },

    #[error("rotation must be a multiple of 90 degrees, got {0}")]
    InvalidRotation(i64),

    #[error("couldn’t open {}: {}", file_name(path), load_reason(source))]
    Load {
        path: PathBuf,
        #[source]
        source: lopdf::Error,
    },

    #[error("{} is password-protected", file_name(.0))]
    Encrypted(PathBuf),

    #[error("the password is incorrect")]
    WrongPassword,

    #[error("the file is not password-protected")]
    NotEncrypted,

    #[error(
        "{} is too large to open on this device: it needs about {} of memory and {} is free",
        file_name(path),
        human_bytes(*needed),
        human_bytes(*available)
    )]
    TooLarge { path: PathBuf, needed: u64, available: u64 },

    #[error("couldn’t save {}: {message}", file_name(path))]
    Save { path: PathBuf, message: String },

    #[error("invalid option: {0}")]
    InvalidOption(String),

    #[error("could not read the image: {0}")]
    Image(String),

    #[error("couldn’t read {}: {message}", file_name(path))]
    ReadImage { path: PathBuf, message: String },

    #[error(transparent)]
    Pdf(#[from] lopdf::Error),
}

/// Messages name files, not full paths: the person knows where they are.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// "1.4 GB", "350 MB".
fn human_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / 1_000_000.0;
    if mb >= 1000.0 {
        format!("{:.1} GB", mb / 1000.0)
    } else {
        format!("{mb:.0} MB")
    }
}

/// Why a PDF couldn't be opened, in words a person can act on.
fn load_reason(error: &lopdf::Error) -> String {
    match error {
        lopdf::Error::IO(e) => match e.kind() {
            std::io::ErrorKind::NotFound => "the file no longer exists".into(),
            std::io::ErrorKind::PermissionDenied => "access was denied".into(),
            _ => format!("it couldn’t be read ({e})"),
        },
        _ => "it isn’t a PDF, or it’s damaged".into(),
    }
}

/// Why a file couldn't be written, in words a person can act on.
pub(crate) fn io_reason(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "the folder doesn’t exist".into(),
        std::io::ErrorKind::PermissionDenied => "access was denied — is the file open in another app?".into(),
        _ => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_name_the_file_and_say_why() {
        let missing = || std::io::Error::from(std::io::ErrorKind::NotFound);
        let load = Error::Load { path: "/some/where/report.pdf".into(), source: lopdf::Error::IO(missing()) };
        assert_eq!(load.to_string(), "couldn’t open report.pdf: the file no longer exists");
        let save = Error::Save { path: "/x/out.pdf".into(), message: io_reason(&missing()) };
        assert_eq!(save.to_string(), "couldn’t save out.pdf: the folder doesn’t exist");
        let large = Error::TooLarge { path: "/x/big.pdf".into(), needed: 2_300_000_000, available: 900_000_000 };
        assert_eq!(large.to_string(), "big.pdf is too large to open on this device: it needs about 2.3 GB of memory and 900 MB is free");
    }
}
