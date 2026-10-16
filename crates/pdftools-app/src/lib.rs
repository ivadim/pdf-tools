//! PDF Tools: the Slint desktop app.

slint::include_modules!();

pub mod app;
pub mod dialogs;
pub mod jobs;
pub mod previews;
pub mod tools;

pub use pdftools_render::{ranges, raster};

/// Turns rendered pixels into Slint images.
pub trait ToImage {
    fn to_image(&self) -> slint::Image;
}

impl ToImage for raster::Picture {
    fn to_image(&self) -> slint::Image {
        slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(&self.pixels, self.width, self.height))
    }
}

impl ToImage for previews::Rendered {
    fn to_image(&self) -> slint::Image {
        slint::Image::from_rgba8_premultiplied(slint::SharedPixelBuffer::clone_from_slice(
            &self.pixels,
            self.width,
            self.height,
        ))
    }
}
