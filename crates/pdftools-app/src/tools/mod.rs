//! The single-file tools.

mod basic;
mod pages;
mod signature;
mod stamps;

pub use basic::{Compress, Grayscale, Protect, Rotate, Unlock};
pub use pages::{move_item, Metadata, Rearrange, Split};
pub use signature::{Signature, STAGE_WIDTH};
pub use stamps::{PageNumbers, Watermark};

use crate::app::Message;
use crate::catalog::ToolId;

/// A text option: sends the new value.
fn set(tool: ToolId, key: &'static str) -> impl Fn(String) -> Message {
    move |value| Message::Option(tool, key, value)
}

/// A checkbox option: sends "1" or "0".
fn toggle(tool: ToolId, key: &'static str) -> impl Fn(bool) -> Message {
    move |on| Message::Option(tool, key, if on { "1" } else { "0" }.into())
}
