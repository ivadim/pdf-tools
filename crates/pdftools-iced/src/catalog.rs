//! The tools offered, in sidebar order.

use iced::widget::svg::Handle;
use iced::{color, Color};

use crate::icons;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolId {
    Merge,
    Split,
    Compress,
    Protect,
    Unlock,
    Rotate,
    Rearrange,
    PageNumbers,
    Watermark,
    Metadata,
    Signature,
    Grayscale,
    Images,
}

pub struct ToolInfo {
    pub id: ToolId,
    pub name: &'static str,
    pub description: &'static str,
    pub icon: fn() -> Handle,
    pub tint: Color,
    /// Shown in the top row of the home screen.
    pub featured: bool,
}

pub const TOOLS: [ToolInfo; 13] = [
    ToolInfo { id: ToolId::Merge, name: "Merge", description: "Combine PDFs into one, and reorder or rotate their pages on the way", icon: icons::merge, tint: color!(0xf43f5e), featured: true },
    ToolInfo { id: ToolId::Split, name: "Split", description: "Divide a PDF into several files, or pull out the pages you need", icon: icons::split, tint: color!(0x3b82f6), featured: true },
    ToolInfo { id: ToolId::Compress, name: "Compress", description: "Reduce file size by cleaning up and re-encoding large images", icon: icons::compress, tint: color!(0xf59e0b), featured: true },
    ToolInfo { id: ToolId::Protect, name: "Protect", description: "Encrypt a PDF with a password (AES-256) and limit what others can do", icon: icons::protect, tint: color!(0x6366f1), featured: true },
    ToolInfo { id: ToolId::Unlock, name: "Unlock", description: "Remove the password and restrictions from a PDF you have access to", icon: icons::unlock, tint: color!(0xa855f7), featured: false },
    ToolInfo { id: ToolId::Rotate, name: "Rotate", description: "Turn pages upright — individually or all at once", icon: icons::rotate, tint: color!(0xf97316), featured: false },
    ToolInfo { id: ToolId::Rearrange, name: "Rearrange", description: "Reorder, duplicate or delete pages — bookmarks and forms are kept", icon: icons::rearrange, tint: color!(0x22c55e), featured: false },
    ToolInfo { id: ToolId::PageNumbers, name: "Page numbers", description: "Add page numbers in the position and style you choose", icon: icons::page_numbers, tint: color!(0x38bdf8), featured: false },
    ToolInfo { id: ToolId::Watermark, name: "Watermark", description: "Stamp text or a logo across your pages", icon: icons::watermark, tint: color!(0xc084fc), featured: false },
    ToolInfo { id: ToolId::Metadata, name: "Metadata", description: "View and edit the title, author, keywords and other document properties", icon: icons::metadata, tint: color!(0x22d3ee), featured: false },
    ToolInfo { id: ToolId::Signature, name: "Signature", description: "Draw, type or upload your signature and place it on the page", icon: icons::signature, tint: color!(0xec4899), featured: false },
    ToolInfo { id: ToolId::Grayscale, name: "Grayscale", description: "Convert colours to shades of grey for printing — text stays sharp and selectable", icon: icons::grayscale, tint: color!(0xa1a1aa), featured: false },
    ToolInfo { id: ToolId::Images, name: "Images to PDF", description: "Turn JPEG and PNG images into a PDF, one image per page", icon: icons::images, tint: color!(0x14b8a6), featured: false },
];

pub fn info(id: ToolId) -> &'static ToolInfo {
    TOOLS.iter().find(|t| t.id == id).expect("every tool is in the catalog")
}
