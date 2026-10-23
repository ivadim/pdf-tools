// Stamps and signatures as PNG pictures: typed text drawn with the system's
// fonts (any installed font and script), drawings and chosen images.

import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct Picture: Equatable {
    /// PNG or JPEG bytes, as the engine reads them.
    let data: Data
    let image: UIImage
    /// In pixels.
    var size: CGSize { CGSize(width: image.size.width * image.scale, height: image.size.height * image.scale) }

    static func == (a: Picture, b: Picture) -> Bool { a.data == b.data }

    /// A chosen image: PNG and JPEG bytes are kept as they are, anything else becomes PNG.
    init?(imageData: Data) {
        guard let image = UIImage(data: imageData) else { return nil }
        if imageData.starts(with: [0x89, 0x50, 0x4e, 0x47]) {
            self.data = imageData
            self.image = image
        } else if imageData.starts(with: [0xff, 0xd8]), image.imageOrientation == .up {
            self.data = imageData
            self.image = image
        } else {
            guard let upright = image.upright, let png = upright.pngData() else { return nil }
            self.data = png
            self.image = upright
        }
    }

    init?(cgImage: CGImage) {
        let image = UIImage(cgImage: cgImage)
        guard let png = image.pngData() else { return nil }
        self.data = png
        self.image = image
    }

    /// Text in `family` (or the system font), cropped to what's drawn.
    static func text(_ text: String, family: String, bold: Bool, color: Color, size: CGFloat = 160) -> Picture? {
        let font = Self.font(family: family, bold: bold, size: size)
        let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: UIColor(color)]
        let lines = text.components(separatedBy: "\n")
        let pad = ceil(size * 0.15)
        let lineHeight = ceil(size * 1.2)
        let width = ceil(lines.map { ($0 as NSString).size(withAttributes: attributes).width }.max() ?? 1) + pad * 2
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.opaque = false
        let canvas = CGSize(width: max(1, width), height: lineHeight * CGFloat(lines.count) + pad * 2)
        let image = UIGraphicsImageRenderer(size: canvas, format: format).image { _ in
            for (i, line) in lines.enumerated() {
                let lineSize = (line as NSString).size(withAttributes: attributes)
                let y = pad + lineHeight * (CGFloat(i) + 0.5) - lineSize.height / 2
                (line as NSString).draw(at: CGPoint(x: pad, y: y), withAttributes: attributes)
            }
        }
        return image.cgImage.flatMap { trim($0, pad: Int(pad / 2)) }.flatMap(Picture.init(cgImage:))
    }

    static func font(family: String, bold: Bool, size: CGFloat) -> UIFont {
        var descriptor =
            family == systemFont
            ? UIFont.systemFont(ofSize: size).fontDescriptor : UIFontDescriptor(fontAttributes: [.family: family])
        if bold, let boldDescriptor = descriptor.withSymbolicTraits(.traitBold) { descriptor = boldDescriptor }
        return UIFont(descriptor: descriptor, size: size)
    }

    static let systemFont = "System"

    /// Font families to choose from.
    static let families: [String] = [systemFont] + UIFont.familyNames.sorted()
}

/// Crops an image to what is drawn on it (alpha above a few percent), keeping `pad` pixels around it.
func trim(_ image: CGImage, pad: Int) -> CGImage? {
    let (width, height) = (image.width, image.height)
    var pixels = [UInt8](repeating: 0, count: width * height * 4)
    let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
        guard let context = CGContext(
            data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return false }
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        return true
    }
    guard drawn else { return nil }
    var (left, top, right, bottom) = (width, height, -1, -1)
    for y in 0..<height {
        for x in 0..<width where pixels[(y * width + x) * 4 + 3] > 8 {
            left = min(left, x)
            right = max(right, x)
            top = min(top, y)
            bottom = max(bottom, y)
        }
    }
    guard right >= 0 else { return nil }
    let rect = CGRect(
        x: max(0, left - pad), y: max(0, top - pad), width: min(width - 1, right + pad) - max(0, left - pad) + 1,
        height: min(height - 1, bottom + pad) - max(0, top - pad) + 1)
    return image.cropping(to: rect)
}

extension UIImage {
    /// Drawn with its orientation applied.
    var upright: UIImage? {
        let format = UIGraphicsImageRendererFormat()
        format.scale = scale
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in draw(at: .zero) }
    }
}

/// Picks an image from Photos or Files.
struct ImageSourceButtons: View {
    let title: String
    let onPick: (Data) -> Void
    @State private var photo: PhotosPickerItem?
    @State private var importing = false

    var body: some View {
        HStack {
            PhotosPicker(selection: $photo, matching: .images) {
                Label("Photos", systemImage: "photo")
            }
            Button {
                importing = true
            } label: {
                Label("Files", systemImage: "folder")
            }
        }
        .buttonStyle(.bordered)
        .accessibilityLabel(title)
        .onChange(of: photo) { _, item in
            Task {
                if let data = try? await item?.loadTransferable(type: Data.self) { onPick(data) }
                photo = nil
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.png, .jpeg, .image]) { result in
            guard let url = try? result.get() else { return }
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            if let data = try? Data(contentsOf: url) { onPick(data) }
        }
    }
}
