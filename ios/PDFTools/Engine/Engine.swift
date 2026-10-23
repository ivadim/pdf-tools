// The Rust engine (crates/pdftools-ffi): tools run the same commands as
// pdftools-cli and the web version; pages are rendered straight into memory.

import CoreGraphics
import Foundation
import PdftoolsFFI

nonisolated struct EngineError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

/// A command's result, as parsed JSON.
nonisolated struct JSON: @unchecked Sendable {
    let value: Any?

    subscript(key: String) -> JSON { JSON(value: (value as? [String: Any])?[key]) }
    var int: Int { (value as? NSNumber)?.intValue ?? 0 }
    var bool: Bool { (value as? NSNumber)?.boolValue ?? false }
    var string: String? { value as? String }
    var array: [JSON] { (value as? [Any])?.map { JSON(value: $0) } ?? [] }
}

nonisolated struct RenderedPage: @unchecked Sendable {
    let image: CGImage
    /// The page's displayed size in points, before any extra rotation.
    let points: CGSize
}

nonisolated enum Engine {
    /// Thumbnails render a few at a time (the engine keeps three document caches).
    private static let renderQueue: OperationQueue = {
        let queue = OperationQueue()
        queue.maxConcurrentOperationCount = 3
        queue.qualityOfService = .userInitiated
        return queue
    }()

    /// Images and streams the engine processes at once. Each image in flight
    /// holds its decoded pixels, so devices with about 3 GB of memory or less
    /// (iPhone XR, SE) use two.
    static func threads(memory: UInt64 = ProcessInfo.processInfo.physicalMemory) -> Int {
        memory < 3_500_000_000 ? 2 : 4
    }

    /// Sets the engine up for this device; called once at launch.
    static func configure() {
        pdftools_set_threads(threads())
    }

    /// Runs a command, e.g. `["merge", out, a, b]`, off the main thread.
    static func run(_ args: [String]) async throws -> JSON {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                continuation.resume(with: Result { try runNow(args) })
            }
        }
    }

    static func runNow(_ args: [String]) throws -> JSON {
        let strings = args.map { strdup($0) }
        defer { strings.forEach { free($0) } }
        let argv: [UnsafePointer<CChar>?] = strings.map { UnsafePointer($0) }
        var out: UnsafeMutablePointer<CChar>?
        let status = pdftools_run(argv, argv.count, &out)
        let text = take(out)
        guard status == 0 else { throw EngineError(message: text) }
        let value = try? JSONSerialization.jsonObject(with: Data(text.utf8), options: [.fragmentsAllowed])
        return JSON(value: value)
    }

    static func render(_ url: URL, page: Int, width: Int, rotation: Int = 0, gray: Bool = false) async throws -> RenderedPage {
        try await withCheckedThrowingContinuation { continuation in
            renderQueue.addOperation {
                continuation.resume(with: Result { try renderNow(url, page: page, width: width, rotation: rotation, gray: gray) })
            }
        }
    }

    static func renderNow(_ url: URL, page: Int, width: Int, rotation: Int, gray: Bool) throws -> RenderedPage {
        var result = PdftoolsPage()
        var error: UnsafeMutablePointer<CChar>?
        let status = pdftools_render(url.path, UInt32(page), UInt32(max(1, width)), Int32(rotation), gray, &result, &error)
        guard status == 0 else { throw EngineError(message: take(error)) }
        // The image uses the engine's pixel buffer as it is (no copy); the
        // buffer goes back to the engine when the image is released.
        let owner = UnsafeMutablePointer<PdftoolsPage>.allocate(capacity: 1)
        owner.initialize(to: result)
        let release: CGDataProviderReleaseDataCallback = { info, _, _ in
            guard let page = info?.assumingMemoryBound(to: PdftoolsPage.self) else { return }
            pdftools_free_page(page)
            page.deinitialize(count: 1)
            page.deallocate()
        }
        guard let provider = CGDataProvider(dataInfo: owner, data: result.pixels, size: result.len, releaseData: release) else {
            release(owner, result.pixels, result.len)
            throw EngineError(message: "couldn’t draw the page")
        }
        guard
            let image = CGImage(
                width: Int(result.width), height: Int(result.height), bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: Int(result.width) * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
        else { throw EngineError(message: "couldn’t draw the page") }
        return RenderedPage(image: image, points: CGSize(width: CGFloat(result.points_width), height: CGFloat(result.points_height)))
    }

    /// Drops a cached document whose file went away. In the background: it
    /// waits for any page of it still rendering.
    static func forget(_ url: URL) {
        let path = url.path
        DispatchQueue.global(qos: .utility).async { pdftools_forget(path) }
    }

    /// "1-3, 5, 8-" → [[1, 2, 3], [5], [8, …, total]]; throws a message for the person.
    static func parseGroups(_ text: String, total: Int) throws -> [[Int]] {
        var out: UnsafeMutablePointer<CChar>?
        let status = pdftools_parse_groups(text, total, &out)
        let result = take(out)
        guard status == 0 else { throw EngineError(message: result) }
        return JSON(value: try? JSONSerialization.jsonObject(with: Data(result.utf8))).array.map { $0.array.map(\.int) }
    }

    /// The distinct pages named, in order.
    static func parseList(_ text: String, total: Int) throws -> [Int] {
        Array(Set(try parseGroups(text, total: total).joined())).sorted()
    }

    private static func take(_ string: UnsafeMutablePointer<CChar>?) -> String {
        guard let string else { return "" }
        defer { pdftools_free_string(string) }
        return String(cString: string)
    }
}

/// [1, 2, 3, 5] → "1-3, 5", as the engine writes page lists.
nonisolated func formatList(_ pages: [Int]) -> String {
    let values = pages.map { max(0, $0) }
    let text = values.withUnsafeBufferPointer { pdftools_format_list($0.baseAddress, $0.count) }
    defer { pdftools_free_string(text) }
    return text.map { String(cString: $0) } ?? ""
}

func formatBytes(_ bytes: Int) -> String {
    ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)
}

func plural(_ n: Int, _ word: String) -> String {
    "\(n) \(word)\(n == 1 ? "" : "s")"
}
