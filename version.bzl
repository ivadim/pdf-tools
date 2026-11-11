"""The version and repository every crate is built with (keep in step with Cargo.toml)."""

VERSION = "0.5.1"
# The Mac App Store version is independent of desktop/CLI releases.
MACOS_APP_STORE_VERSION = "1.0"
REPOSITORY = "https://github.com/ivadim/pdf-tools"

# What env!("CARGO_PKG_...") reads in the crates.
RUSTC_ENV = {"CARGO_PKG_REPOSITORY": REPOSITORY}
