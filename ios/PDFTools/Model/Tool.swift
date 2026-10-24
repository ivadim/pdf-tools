// The thirteen tools, in the desktop app's order.

import SwiftUI

enum Tool: String, CaseIterable, Identifiable, Hashable {
    case merge, split, compress, protect, unlock, rotate, rearrange, numbers, watermark, metadata, signature, grayscale, images

    var id: String { rawValue }

    enum Kind { case single, merge, images }

    var kind: Kind {
        switch self {
        case .merge: .merge
        case .images: .images
        default: .single
        }
    }

    var name: String {
        switch self {
        case .merge: "Merge"
        case .split: "Split"
        case .compress: "Compress"
        case .protect: "Protect"
        case .unlock: "Unlock"
        case .rotate: "Rotate"
        case .rearrange: "Rearrange"
        case .numbers: "Page numbers"
        case .watermark: "Watermark"
        case .metadata: "Metadata"
        case .signature: "Signature"
        case .grayscale: "Grayscale"
        case .images: "Images to PDF"
        }
    }

    var summary: String {
        switch self {
        case .merge: "Combine PDFs into one, and reorder or rotate their pages on the way"
        case .split: "Divide a PDF into several files, or pull out the pages you need"
        case .compress: "Reduce file size by cleaning up and re-encoding large images"
        case .protect: "Encrypt a PDF with a password (AES-256) and limit what others can do"
        case .unlock: "Remove the password and restrictions from a PDF you have access to"
        case .rotate: "Turn pages upright, individually or all at once"
        case .rearrange: "Reorder, duplicate or delete pages; bookmarks and forms are kept"
        case .numbers: "Add page numbers in the position and style you choose"
        case .watermark: "Stamp text or a logo across your pages"
        case .metadata: "View and edit the title, author, keywords and other properties"
        case .signature: "Draw, type or upload your signature and place it on the page"
        case .grayscale: "Convert colours to shades of grey; text stays sharp and selectable"
        case .images: "Turn photos and images into a PDF, one image per page"
        }
    }

    var symbol: String {
        switch self {
        case .merge: "arrow.triangle.merge"
        case .split: "scissors"
        case .compress: "arrow.down.right.and.arrow.up.left"
        case .protect: "lock"
        case .unlock: "lock.open"
        case .rotate: "rotate.right"
        case .rearrange: "square.grid.3x3.square"
        case .numbers: "number"
        case .watermark: "drop"
        case .metadata: "info.circle"
        case .signature: "signature"
        case .grayscale: "circle.lefthalf.filled"
        case .images: "photo.on.rectangle"
        }
    }

    var tint: Color {
        switch self {
        case .merge: Color(hex: 0xf43f5e)
        case .split: Color(hex: 0x3b82f6)
        case .compress: Color(hex: 0xf59e0b)
        case .protect: Color(hex: 0x6366f1)
        case .unlock: Color(hex: 0xa855f7)
        case .rotate: Color(hex: 0xf97316)
        case .rearrange: Color(hex: 0x22c55e)
        case .numbers: Color(hex: 0x38bdf8)
        case .watermark: Color(hex: 0xc084fc)
        case .metadata: Color(hex: 0x22d3ee)
        case .signature: Color(hex: 0xec4899)
        case .grayscale: Color(hex: 0xa1a1aa)
        case .images: Color(hex: 0x14b8a6)
        }
    }

    /// Pages can be left out of the result (their checkbox).
    var leavesOutPages: Bool {
        switch self {
        case .merge, .compress, .protect, .rotate, .numbers, .watermark, .metadata, .grayscale: true
        default: false
        }
    }

    /// The run button's label.
    var action: String {
        switch self {
        case .numbers: "Add numbers"
        case .images: "Create PDF"
        case .metadata: "Save details"
        case .signature: "Sign"
        default: name
        }
    }

    @MainActor func makeOptions() -> any ToolOptions {
        switch self {
        case .merge: MergeOptions()
        case .split: SplitOptions()
        case .compress: CompressOptions()
        case .protect: ProtectOptions()
        case .unlock: UnlockOptions()
        case .rotate: RotateOptions()
        case .rearrange: RearrangeOptions()
        case .numbers: NumbersOptions()
        case .watermark: WatermarkOptions()
        case .metadata: MetadataOptions()
        case .signature: SignatureOptions()
        case .grayscale: GrayscaleOptions()
        case .images: ImagesOptions()
        }
    }
}

extension Color {
    init(hex: UInt32) {
        self.init(red: Double((hex >> 16) & 0xff) / 255, green: Double((hex >> 8) & 0xff) / 255, blue: Double(hex & 0xff) / 255)
    }

    /// "RRGGBB" for the engine.
    var hexString: String {
        let c = UIColor(self).resolvedColor(with: UITraitCollection(userInterfaceStyle: .light)).cgColor.converted(
            to: CGColorSpace(name: CGColorSpace.sRGB)!, intent: .defaultIntent, options: nil)?.components ?? [0, 0, 0]
        let channel = { (i: Int) in String(format: "%02x", Int((min(1, max(0, c[min(i, c.count - 1)])) * 255).rounded())) }
        return channel(0) + channel(1) + channel(2)
    }
}
