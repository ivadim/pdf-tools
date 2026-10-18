"""The version and repository every crate is built with (keep in step with Cargo.toml)."""

VERSION = "0.3.0"
REPOSITORY = "https://github.com/ivadim/pdf-tools"

# What env!("CARGO_PKG_...") reads in the crates.
RUSTC_ENV = {"CARGO_PKG_REPOSITORY": REPOSITORY}
