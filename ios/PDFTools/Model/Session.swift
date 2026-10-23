// One tool's screen: its files, its page cards and its options.

import SwiftUI

/// An opened PDF, copied into the app's workspace.
final class PDFFile: Identifiable {
    let id = UUID()
    let url: URL
    let name: String
    let info: JSON

    init(url: URL, name: String, info: JSON) {
        self.url = url
        self.name = name
        self.info = info
    }

    var pageCount: Int { info["pages"].int }
    var locked: Bool { info["needs_password"].bool }
    var encrypted: Bool { info["encrypted"].bool }
    var repaired: Bool { info["repaired"].bool }
    var size: Int { info["file_size"].int }
    var baseName: String { (name as NSString).deletingPathExtension }

    /// Copies a picked file into the workspace and inspects it.
    static func open(_ source: URL) async throws -> PDFFile {
        let local = try Workspace.importFile(source)
        let info = try await Engine.run(["info", local.path])
        return PDFFile(url: local, name: source.lastPathComponent, info: info)
    }
}

/// One page card.
struct PageItem: Identifiable, Equatable {
    let id = UUID()
    let file: PDFFile
    /// 1-based page in `file`.
    let page: Int
    var rotation = 0
    var keep = true

    static func == (a: PageItem, b: PageItem) -> Bool {
        a.id == b.id && a.rotation == b.rotation && a.keep == b.keep
    }
}

/// An image for Images to PDF.
struct ImageItem: Identifiable, Equatable {
    let id = UUID()
    let url: URL
    let name: String
    /// As displayed (camera rotation applied), in pixels.
    let size: CGSize
    let thumbnail: UIImage
    var rotation = 0
}

/// What a finished run produced.
struct ToolResult: Identifiable {
    let id = UUID()
    let message: String
    let files: [URL]
}

@Observable
final class ToolSession {
    var tool: Tool
    var options: any ToolOptions
    var files: [PDFFile] = []
    var items: [PageItem] = []
    var images: [ImageItem] = []
    /// Each page's size in points, once its thumbnail is drawn (key: file id + page).
    var pagePoints: [String: CGSize] = [:]
    var loading = false
    var running = false
    var result: ToolResult?
    var error: String?
    /// A locked file opened in a single-file tool other than Unlock.
    var lockedFile: PDFFile?

    init(tool: Tool) {
        self.tool = tool
        self.options = tool.makeOptions()
    }

    var pdf: PDFFile? { files.first }
    var kept: [PageItem] { items.filter(\.keep) }

    func points(of item: PageItem) -> CGSize? {
        pagePoints[Self.key(item.file, item.page)]
    }

    static func key(_ file: PDFFile, _ page: Int) -> String { "\(file.id)-\(page)" }

    // MARK: Files

    func open(_ urls: [URL]) async {
        loading = true
        defer { loading = false }
        for url in urls {
            do {
                let file = try await PDFFile.open(url)
                if file.locked && tool != .unlock {
                    if tool.kind == .merge {
                        // Switching to Unlock would drop the other files chosen here.
                        error = "\(file.name) has a password, so it was left out. Remove the password with Unlock first."
                    } else {
                        lockedFile = file
                    }
                    continue
                }
                if tool.kind == .merge {
                    files.append(file)
                    items += newItems(file)
                } else {
                    files = [file]
                    items = newItems(file)
                    options.reset(for: self)
                }
            } catch {
                self.error = "\(url.lastPathComponent): \(error.localizedDescription)"
            }
        }
    }

    func newItems(_ file: PDFFile) -> [PageItem] {
        file.locked ? [] : (0..<file.pageCount).map { PageItem(file: file, page: $0 + 1) }
    }

    func resetItems() {
        items = files.flatMap(newItems)
    }

    func removeFile(_ file: PDFFile) {
        files.removeAll { $0 === file }
        items.removeAll { $0.file === file }
        Engine.forget(file.url)
    }

    /// Opens the locked file in Unlock instead.
    func switchToUnlock(_ file: PDFFile) {
        tool = .unlock
        options = Tool.unlock.makeOptions()
        files = [file]
        items = []
        lockedFile = nil
    }

    func move(_ index: Int, by offset: Int) {
        let target = index + offset
        guard items.indices.contains(index), items.indices.contains(target) else { return }
        items.swapAt(index, target)
    }

    /// Moves the dragged card to where another one is.
    func move(_ id: UUID, to target: UUID) {
        guard let from = items.firstIndex(where: { $0.id == id }), let to = items.firstIndex(where: { $0.id == target }), from != to
        else { return }
        let item = items.remove(at: from)
        items.insert(item, at: to)
    }

    // MARK: Pages left out

    /// `--keep` when some pages are left out.
    var keepArgs: [String] {
        kept.count == items.count ? [] : ["--keep=\(formatList(kept.map(\.page)))"]
    }

    var keepProblem: String? { kept.isEmpty ? "Keep at least one page." : nil }

    var removedNote: String {
        let removed = items.count - kept.count
        return removed > 0 ? " (\(plural(removed, "page")) left out)" : ""
    }

    // MARK: Running

    var problem: String? {
        switch tool.kind {
        case .single where pdf == nil: "Choose a PDF."
        default: options.problem(self)
        }
    }

    func run() async {
        guard problem == nil else { return }
        running = true
        defer { running = false }
        do {
            result = try await options.run(self)
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// A new folder for one run's outputs.
    func outputFolder() throws -> URL {
        try Workspace.newFolder("Outputs")
    }
}

/// Imported files and outputs, kept in the app's temporary folder.
enum Workspace {
    nonisolated static let root = FileManager.default.temporaryDirectory.appendingPathComponent("Workspace", isDirectory: true)

    nonisolated static func newFolder(_ kind: String) throws -> URL {
        let url = root.appendingPathComponent(kind).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    /// Copies a file (from the Files app, or another app) into the workspace.
    nonisolated static func importFile(_ source: URL) throws -> URL {
        let scoped = source.startAccessingSecurityScopedResource()
        defer { if scoped { source.stopAccessingSecurityScopedResource() } }
        let local = try newFolder("Inputs").appendingPathComponent(source.lastPathComponent)
        try FileManager.default.copyItem(at: source, to: local)
        return local
    }

    nonisolated static func write(_ data: Data, named name: String) throws -> URL {
        let url = try newFolder("Inputs").appendingPathComponent(name)
        try data.write(to: url)
        return url
    }

    /// Clears what earlier launches left behind.
    nonisolated static func clean() {
        try? FileManager.default.removeItem(at: root)
    }
}
