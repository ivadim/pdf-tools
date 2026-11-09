import AppKit
import ImageIO

// The captures are genuine screenshots exported from testCaptureMarketingScreenshots.
let root = URL(fileURLWithPath: CommandLine.arguments[1])
let items: [(String, String, String, String, String)] = [
    ("home", "01-on-device", "Your PDFs.", "Your device.", "Useful tools, right where your files are."),
    ("merge", "02-merge", "Bring pages", "together.", "Merge PDFs and arrange pages your way."),
    ("compress", "03-compress", "Make room", "to share.", "Choose the compression that fits."),
    ("redact", "04-redact", "Keep private", "details private.", "Mark areas and create a redacted copy."),
    ("images", "05-images", "Photos in.", "PDF out.", "Choose your page size and margins.")
]
func color(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> NSColor { NSColor(srgbRed: r/255, green: g/255, blue: b/255, alpha: 1) }
func draw(_ s: String, _ x: CGFloat, _ y: CGFloat, _ size: CGFloat, _ weight: NSFont.Weight, _ color: NSColor) {
    (s as NSString).draw(at: NSPoint(x: x, y: y), withAttributes: [.font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: color])
}
for device in ["duo", "ipad"] {
    for mode in ["light", "dark"] {
        for (index, shot) in items.enumerated() {
            let url = root.appendingPathComponent("captures/\(device)/\(mode)/\(shot.0).png")
            guard let source = CGImageSourceCreateWithURL(url as CFURL, nil), let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { fatalError("Missing \(url.path)") }
            let width = image.width, height = image.height
            // iPad's real status bar contains the simulator date. Crop that bar
            // from the inset capture while keeping the original source on disk.
            let displayed = device == "ipad" ? image.cropping(to: CGRect(x: 0, y: 55, width: width, height: height - 55))! : image
            let w = CGFloat(width), h = CGFloat(height)
            let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
            let context = NSGraphicsContext(bitmapImageRep: bitmap)!
            NSGraphicsContext.saveGraphicsState(); NSGraphicsContext.current = context; context.imageInterpolation = .high
            let dark = mode == "dark"
            (dark ? color(17, 23, 37) : color(239, 243, 249)).setFill(); NSRect(x: 0, y: 0, width: w, height: h).fill()
            let ink = dark ? NSColor.white : color(25, 33, 48)
            let margin = w * 0.065
            let title = min(w * 0.075, h * 0.048)
            color(68, 111, 224).setFill(); NSRect(x: margin, y: h * 0.953, width: w * 0.048, height: max(5, w * 0.006)).fill()
            draw("PDF TOOLS  /  \(String(format: "%02d", index + 1))", margin + w * 0.065, h * 0.942, min(w * 0.023, h * 0.014), .semibold, dark ? color(161, 187, 255) : color(49, 91, 190))
            draw(shot.2, margin, h * 0.882, title, .bold, ink)
            draw(shot.3, margin, h * 0.835, title, .bold, ink)
            draw(shot.4, margin, h * 0.797, min(w * 0.026, h * 0.016), .medium, dark ? color(188, 198, 218) : color(76, 88, 107))
            let sourceW = CGFloat(displayed.width), sourceH = CGFloat(displayed.height)
            let scale = min(w * 0.86 / sourceW, h * 0.735 / sourceH)
            let rect = NSRect(x: (w - sourceW * scale)/2, y: h * 0.025, width: sourceW * scale, height: sourceH * scale)
            let border = NSBezierPath(roundedRect: rect.insetBy(dx: -5, dy: -5), xRadius: 38, yRadius: 38)
            (dark ? color(67, 79, 101) : color(207, 216, 230)).setFill(); border.fill()
            NSGraphicsContext.saveGraphicsState(); NSBezierPath(roundedRect: rect, xRadius: 33, yRadius: 33).addClip()
            NSImage(cgImage: displayed, size: NSSize(width: displayed.width, height: displayed.height)).draw(in: rect)
            NSGraphicsContext.restoreGraphicsState(); NSGraphicsContext.restoreGraphicsState()
            let directory = root.appendingPathComponent("listing/\(device)/\(mode)")
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try bitmap.representation(using: .jpeg, properties: [.compressionFactor: 0.94])!.write(to: directory.appendingPathComponent("\(shot.1).jpg"))
            print("\(device)/\(mode)/\(shot.1): \(width) × \(height)")
        }
    }
}
