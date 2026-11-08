import AppKit
import ImageIO
import UniformTypeIdentifiers

let repository = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
let outputDirectory = repository.appendingPathComponent("docs/app-store/ios/listing", isDirectory: true)
try FileManager.default.createDirectory(at: outputDirectory, withIntermediateDirectories: true)

struct ListingShot {
    let source: String
    let output: String
    let title: [String]
    let subtitle: String
}

let shots = [
    ListingShot(
        source: "home.png",
        output: "01-private-by-design.jpg",
        title: ["PDF tools that keep", "files on your device."],
        subtitle: "Free to use. No account, ads, or tracking."
    ),
    ListingShot(
        source: "tools/merge-configured.png",
        output: "02-merge-and-arrange.jpg",
        title: ["Put every page", "in the right order."],
        subtitle: "Combine PDFs and rearrange pages before saving."
    ),
    ListingShot(
        source: "tools/compress-configured.png",
        output: "03-compress-pdfs.jpg",
        title: ["Make big PDFs", "easier to share."],
        subtitle: "Choose lossless, balanced, or stronger compression."
    ),
    ListingShot(
        source: "tools/redact-configured.png",
        output: "04-redact-private-details.jpg",
        title: ["Redact private", "details safely."],
        subtitle: "Mark areas before creating a separate redacted copy."
    ),
    ListingShot(
        source: "tools/images-configured.png",
        output: "05-images-to-pdf.jpg",
        title: ["Turn photos into", "a ready-to-share PDF."],
        subtitle: "Choose page size, orientation, and margins."
    ),
]

let width = 1206
let height = 2622
let colorSpace = CGColorSpaceCreateDeviceRGB()
let font = NSFont.systemFont(ofSize: 79, weight: .bold)
let subtitleFont = NSFont.systemFont(ofSize: 31, weight: .regular)
let labelFont = NSFont.systemFont(ofSize: 21, weight: .semibold)
let ink = NSColor(srgbRed: 0.075, green: 0.09, blue: 0.13, alpha: 1)
let secondaryInk = NSColor(srgbRed: 0.34, green: 0.37, blue: 0.43, alpha: 1)
let accent = NSColor(srgbRed: 0.12, green: 0.42, blue: 0.91, alpha: 1)
let canvas = NSColor(srgbRed: 0.965, green: 0.969, blue: 0.984, alpha: 1)

func cgImage(at url: URL) throws -> CGImage {
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
        throw NSError(domain: "ListingScreenshots", code: 1, userInfo: [NSLocalizedDescriptionKey: "Could not read \(url.path)"])
    }
    return image
}

func draw(_ text: String, at point: NSPoint, font: NSFont, color: NSColor) {
    (text as NSString).draw(at: point, withAttributes: [.font: font, .foregroundColor: color])
}

func render(_ shot: ListingShot) throws {
    let sourceURL = repository
        .appendingPathComponent("docs/design-reviews/screenshots/ios/light", isDirectory: true)
        .appendingPathComponent(shot.source)
    let sourceImage = try cgImage(at: sourceURL)
    let bitmap = NSBitmapImageRep(
        bitmapDataPlanes: nil,
        pixelsWide: width,
        pixelsHigh: height,
        bitsPerSample: 8,
        samplesPerPixel: 4,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: .deviceRGB,
        bytesPerRow: 0,
        bitsPerPixel: 0
    )!
    guard let context = NSGraphicsContext(bitmapImageRep: bitmap) else {
        throw NSError(domain: "ListingScreenshots", code: 2, userInfo: [NSLocalizedDescriptionKey: "Could not create image context"])
    }

    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = context
    context.imageInterpolation = .high
    canvas.setFill()
    NSRect(x: 0, y: 0, width: width, height: height).fill()

    // A small category label gives the carousel a consistent editorial rhythm.
    draw("PDF TOOLS  /  \(String(format: "%02d", shots.firstIndex(where: { $0.output == shot.output })! + 1))", at: NSPoint(x: 84, y: 2518), font: labelFont, color: accent)

    let titleY: CGFloat = shot.title.count == 2 ? 2405 : 2370
    for (index, line) in shot.title.enumerated() {
        draw(line, at: NSPoint(x: 82, y: titleY - CGFloat(index) * 91), font: font, color: ink)
    }
    draw(shot.subtitle, at: NSPoint(x: 84, y: 2255), font: subtitleFont, color: secondaryInk)

    let cardWidth: CGFloat = 1000
    let cardHeight = cardWidth * CGFloat(sourceImage.height) / CGFloat(sourceImage.width)
    let cardRect = NSRect(x: (CGFloat(width) - cardWidth) / 2, y: 72, width: cardWidth, height: cardHeight)
    let cardPath = NSBezierPath(roundedRect: cardRect, xRadius: 40, yRadius: 40)
    let shadow = NSShadow()
    shadow.shadowColor = NSColor(srgbRed: 0.12, green: 0.16, blue: 0.25, alpha: 0.16)
    shadow.shadowBlurRadius = 30
    shadow.shadowOffset = NSSize(width: 0, height: 10)
    shadow.set()
    NSColor.white.setFill()
    cardPath.fill()
    NSShadow().set()
    NSGraphicsContext.saveGraphicsState()
    cardPath.addClip()
    NSImage(cgImage: sourceImage, size: NSSize(width: sourceImage.width, height: sourceImage.height))
        .draw(in: cardRect, from: .zero, operation: .sourceOver, fraction: 1)
    NSGraphicsContext.restoreGraphicsState()
    NSColor(srgbRed: 0.85, green: 0.87, blue: 0.91, alpha: 1).setStroke()
    cardPath.lineWidth = 2
    cardPath.stroke()

    NSGraphicsContext.restoreGraphicsState()
    guard let data = bitmap.representation(using: .jpeg, properties: [.compressionFactor: 0.94]) else {
        throw NSError(domain: "ListingScreenshots", code: 3, userInfo: [NSLocalizedDescriptionKey: "Could not encode \(shot.output)"])
    }
    try data.write(to: outputDirectory.appendingPathComponent(shot.output), options: .atomic)
    print("Wrote \(shot.output) (\(width)×\(height))")
}

for shot in shots {
    try render(shot)
}
