use std::path::PathBuf;

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

    #[error("could not read {}: {source}", path.display())]
    Load {
        path: PathBuf,
        #[source]
        source: lopdf::Error,
    },

    #[error("{} is password-protected", .0.display())]
    Encrypted(PathBuf),

    #[error("the password is incorrect")]
    WrongPassword,

    #[error("the file is not password-protected")]
    NotEncrypted,

    #[error("could not write {}: {message}", path.display())]
    Save { path: PathBuf, message: String },

    #[error("invalid option: {0}")]
    InvalidOption(String),

    #[error("could not read the image: {0}")]
    Image(String),

    #[error(transparent)]
    Pdf(#[from] lopdf::Error),
}
