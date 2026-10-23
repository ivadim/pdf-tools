// Merge, Split, Rotate and Rearrange: tools about which pages go where.

import SwiftUI

@Observable
final class MergeOptions: ToolOptions {
    func panel(_ session: ToolSession) -> AnyView {
        AnyView(Note("\(plural(session.files.count, "file")), \(plural(session.kept.count, "page")) in the result. Drag a page by its ≡ handle to move it."))
    }

    func gridActions(_ session: ToolSession) -> [CardControl] {
        [
            CardControl(symbol: "arrow.uturn.backward", label: "Reset order") {
                withAnimation {
                    session.items.sort { a, b in
                        let (fa, fb) = (session.files.firstIndex { $0 === a.file } ?? 0, session.files.firstIndex { $0 === b.file } ?? 0)
                        return fa != fb ? fa < fb : a.page < b.page
                    }
                }
            }
        ]
    }

    func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor {
        let file = session.files.firstIndex { $0 === item.file } ?? 0
        return CardDecor(controls: moveControls(session, index) + rotateControls(session, index), partColor: fileColor(file))
    }

    func problem(_ session: ToolSession) -> String? {
        session.files.count < 2 ? "Add at least two PDFs." : session.keepProblem
    }

    func run(_ session: ToolSession) async throws -> ToolResult {
        let pages = session.kept.map { item in "\(session.files.firstIndex { $0 === item.file } ?? 0):\(item.page):\(item.rotation)" }
        let args = ["merge", "OUT"] + session.files.map(\.url.path) + ["--pages=\(pages.joined(separator: ","))"]
        return try await runToFile(session, args, name: "merged.pdf") { json in
            "Merged \(plural(json["files"].int, "file")) into \(plural(json["pages"].int, "page"))."
        }
    }
}

@Observable
final class SplitOptions: ToolOptions {
    enum Mode: String, CaseIterable { case ranges = "Ranges", every = "Every N", each = "Each page", extract = "Extract" }

    var mode = Mode.ranges
    var ranges = ""
    var every = 2
    var extract = ""
    var base = "part"

    func reset(for session: ToolSession) {
        base = session.pdf?.baseName ?? "part"
    }

    /// Each output file's pages.
    func parts(_ session: ToolSession) throws -> [[Int]] {
        let total = session.pdf?.pageCount ?? 0
        switch mode {
        case .ranges:
            return try Engine.parseGroups(ranges, total: total)
        case .every:
            let n = max(1, every)
            return stride(from: 1, through: total, by: n).map { Array($0...min(total, $0 + n - 1)) }
        case .each:
            return (1...max(1, total)).map { [$0] }
        case .extract:
            return [try Engine.parseList(extract, total: total)]
        }
    }

    func panel(_ session: ToolSession) -> AnyView {
        AnyView(SplitPanel(options: self))
    }

    func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor {
        guard let parts = try? parts(session) else { return CardDecor() }
        let part = parts.firstIndex { $0.contains(item.page) }
        return CardDecor(partColor: part.map(fileColor), dimmed: part == nil)
    }

    func problem(_ session: ToolSession) -> String? {
        do {
            let parts = try parts(session)
            if mode != .extract && base.trimmingCharacters(in: .whitespaces).isEmpty { return "Enter a name for the files." }
            return parts.isEmpty ? "Nothing to split." : nil
        } catch {
            return error.localizedDescription
        }
    }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf else { throw EngineError(message: "Choose a PDF.") }
        let parts = try parts(session)
        let name = mode == .extract ? "part" : base.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "/", with: "_")
        let folder = try session.outputFolder()
        let json = try await Engine.run(
            ["split", pdf.url.path, folder.path, "--parts=\(parts.map(formatList).joined(separator: ";"))", "--name=\(name)"])
        var files = json["files"].array.compactMap(\.string).map { URL(fileURLWithPath: $0) }
        if mode == .extract, let file = files.first {
            let renamed = folder.appendingPathComponent(
                "\(pdf.baseName)-pages-\(formatList(parts[0]).replacingOccurrences(of: ", ", with: "_")).pdf")
            try FileManager.default.moveItem(at: file, to: renamed)
            return ToolResult(message: "Extracted \(plural(parts[0].count, "page")).", files: [renamed])
        }
        files.sort { $0.path.localizedStandardCompare($1.path) == .orderedAscending }
        return ToolResult(message: "Created \(plural(json["count"].int, "file")).", files: files)
    }
}

private struct SplitPanel: View {
    @Bindable var options: SplitOptions

    var body: some View {
        Picker("Split", selection: $options.mode) {
            ForEach(SplitOptions.Mode.allCases, id: \.self) { Text($0.rawValue) }
        }
        .pickerStyle(.segmented)
        switch options.mode {
        case .ranges:
            LabeledField("One file per range") {
                TextField("1-3, 4-6, 7-", text: $options.ranges).keyboardType(.numbersAndPunctuation)
            }
        case .every:
            Stepper("Pages per file: \(options.every)", value: $options.every, in: 1...999)
        case .extract:
            LabeledField("Pages to extract into one file") {
                TextField("1, 3-5", text: $options.extract).keyboardType(.numbersAndPunctuation)
            }
        case .each:
            EmptyView()
        }
        if options.mode != .extract {
            LabeledField("File names") {
                TextField("part", text: $options.base).autocorrectionDisabled()
            }
        }
        Note("Pages are coloured by the file they’ll end up in.")
    }
}

@Observable
final class RotateOptions: ToolOptions {
    func panel(_ session: ToolSession) -> AnyView {
        AnyView(Note("Rotate pages with the buttons under each one, or all at once."))
    }

    func gridActions(_ session: ToolSession) -> [CardControl] {
        [
            CardControl(symbol: "rotate.left", label: "All left") {
                for i in session.items.indices { session.items[i].rotation = (session.items[i].rotation + 270) % 360 }
            },
            CardControl(symbol: "rotate.right", label: "All right") {
                for i in session.items.indices { session.items[i].rotation = (session.items[i].rotation + 90) % 360 }
            },
            CardControl(symbol: "arrow.uturn.backward", label: "Reset") {
                for i in session.items.indices { session.items[i].rotation = 0 }
            },
        ]
    }

    func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor {
        CardDecor(controls: rotateControls(session, index))
    }

    func problem(_ session: ToolSession) -> String? {
        if session.items.allSatisfy({ $0.keep && $0.rotation == 0 }) { return "Rotate a page first." }
        return session.keepProblem
    }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf else { throw EngineError(message: "Choose a PDF.") }
        let pages = session.kept.map { "\($0.page):\($0.rotation)" }.joined(separator: ",")
        let turned = session.kept.filter { $0.rotation != 0 }.count
        let note = session.removedNote
        return try await runToFile(session, ["organize", pdf.url.path, "OUT", "--pages=\(pages)"], name: "\(pdf.baseName)-rotated.pdf") { _ in
            "Rotated \(plural(turned, "page"))\(note)."
        }
    }
}

@Observable
final class RearrangeOptions: ToolOptions {
    func panel(_ session: ToolSession) -> AnyView {
        AnyView(Note("\(plural(session.items.count, "page")) in the result. Drag a page by its ≡ handle to move it."))
    }

    func gridActions(_ session: ToolSession) -> [CardControl] {
        [
            CardControl(symbol: "arrow.left.arrow.right", label: "Reverse") { withAnimation { session.items.reverse() } },
            CardControl(symbol: "arrow.uturn.backward", label: "Reset") { withAnimation { session.resetItems() } },
        ]
    }

    func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor {
        CardDecor(
            controls: moveControls(session, index) + [
                CardControl(symbol: "plus.square.on.square", label: "Duplicate") {
                    withAnimation { session.items.insert(PageItem(file: item.file, page: item.page), at: index + 1) }
                },
                CardControl(symbol: "trash", label: "Delete", disabled: session.items.count == 1) {
                    withAnimation { _ = session.items.remove(at: index) }
                },
            ])
    }

    func problem(_ session: ToolSession) -> String? {
        session.items.isEmpty ? "Keep at least one page." : nil
    }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf else { throw EngineError(message: "Choose a PDF.") }
        let pages = session.items.map { "\($0.page)" }.joined(separator: ",")
        let count = session.items.count
        return try await runToFile(session, ["organize", pdf.url.path, "OUT", "--pages=\(pages)"], name: "\(pdf.baseName)-rearranged.pdf") { _ in
            "Saved \(plural(count, "page"))."
        }
    }
}
