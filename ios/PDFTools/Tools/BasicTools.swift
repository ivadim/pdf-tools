// Compress, Protect, Unlock, Metadata and Grayscale: one PDF in, one out.

import SwiftUI

@Observable
final class CompressOptions: ToolOptions {
    enum Level: String, CaseIterable { case lossless, balanced, strong }
    var level = Level.balanced
    /// The file's images and their stored size: counting them reads the whole
    /// file, so only Compress does it, in the background after opening.
    var imageStats: (count: Int, bytes: Int)?

    func reset(for session: ToolSession) {
        imageStats = nil
        guard let pdf = session.pdf, !pdf.locked else { return }
        Task { await loadImageStats(pdf) }
    }

    func loadImageStats(_ pdf: PDFFile) async {
        guard let info = try? await Engine.run(["info", pdf.url.path, "--images"]) else { return }
        imageStats = (info["images"].int, info["image_bytes"].int)
    }

    func panel(_ session: ToolSession) -> AnyView { AnyView(CompressPanel(options: self, pdf: session.pdf)) }


    func problem(_ session: ToolSession) -> String? { session.keepProblem }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf else { throw EngineError(message: "Choose a PDF.") }
        let note = session.removedNote
        let args = ["compress", pdf.url.path, "OUT", "--level=\(level.rawValue)"] + session.keepArgs
        return try await runToFile(session, args, name: "\(pdf.baseName)-compressed.pdf") { json in
            let (before, after) = (json["before"].int, json["after"].int)
            let saved = before > 0 ? Int(((1 - Double(after) / Double(before)) * 100).rounded()) : 0
            return "\(formatBytes(before)) → \(formatBytes(after)) (\(saved >= 0 ? "−" : "+")\(abs(saved))%)\(note)."
        }
    }
}

private struct CompressPanel: View {
    @Bindable var options: CompressOptions
    let pdf: PDFFile?

    var body: some View {
        Picker("Level", selection: $options.level) {
            ForEach(CompressOptions.Level.allCases, id: \.self) { Text($0.rawValue.capitalized) }
        }
        .pickerStyle(.segmented)
        switch options.level {
        case .lossless: Note("Drops unused data. Page content is untouched.")
        case .balanced: Note("Also downscales images to 2000 px and re-encodes them as JPEG (quality 75).")
        case .strong: Note("Also downscales images to 1200 px and re-encodes them as JPEG (quality 50).")
        }
        if let pdf {
            if let stats = options.imageStats {
                Note("\(formatBytes(pdf.size)) · \(plural(stats.count, "image")) (\(formatBytes(stats.bytes)))")
            } else {
                Note("\(formatBytes(pdf.size)) · counting images…")
            }
        }
    }
}

@Observable
final class ProtectOptions: ToolOptions {
    var password = ""
    var confirm = ""
    var owner = ""
    var print = true
    var copy = true
    var modify = true
    var annotate = true

    func panel(_ session: ToolSession) -> AnyView { AnyView(ProtectPanel(options: self)) }

    func problem(_ session: ToolSession) -> String? {
        if password.isEmpty { return "Enter a password." }
        if password != confirm { return "The passwords don’t match." }
        return session.keepProblem
    }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf else { throw EngineError(message: "Choose a PDF.") }
        var args = ["protect", pdf.url.path, "OUT", "--password=\(password)"] + session.keepArgs
        if !owner.isEmpty { args.append("--owner=\(owner)") }
        for (allowed, name) in [(print, "print"), (copy, "copy"), (modify, "modify"), (annotate, "annotate")] where !allowed {
            args.append("--no-\(name)")
        }
        let note = session.removedNote
        return try await runToFile(session, args, name: "\(pdf.baseName)-protected.pdf") { _ in "Protected with a password\(note)." }
    }
}

private struct ProtectPanel: View {
    @Bindable var options: ProtectOptions

    var body: some View {
        LabeledField("Password to open") { SecureField("Password", text: $options.password).textContentType(.newPassword) }
        LabeledField("Repeat the password") { SecureField("Password", text: $options.confirm).textContentType(.newPassword) }
        Text("Allow").font(.headline)
        Toggle("Printing", isOn: $options.print)
        Toggle("Copying text and images", isOn: $options.copy)
        Toggle("Editing", isOn: $options.modify)
        Toggle("Comments and forms", isOn: $options.annotate)
        LabeledField("Owner password (optional)") {
            SecureField("Lifts the restrictions", text: $options.owner).textContentType(.newPassword)
        }
    }
}

@Observable
final class UnlockOptions: ToolOptions {
    var password = ""

    func panel(_ session: ToolSession) -> AnyView { AnyView(UnlockPanel(options: self, pdf: session.pdf)) }

    func reset(for session: ToolSession) { password = "" }

    func problem(_ session: ToolSession) -> String? {
        guard let pdf = session.pdf else { return "Choose a PDF." }
        if !pdf.encrypted { return "Nothing to unlock." }
        return pdf.locked && password.isEmpty ? "Enter the password." : nil
    }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf else { throw EngineError(message: "Choose a PDF.") }
        return try await runToFile(session, ["unlock", pdf.url.path, "OUT", "--password=\(password)"], name: "\(pdf.baseName)-unlocked.pdf") { _ in
            "Removed the protection."
        }
    }
}

private struct UnlockPanel: View {
    @Bindable var options: UnlockOptions
    let pdf: PDFFile?

    var body: some View {
        if let pdf {
            if !pdf.encrypted {
                Note("This file has no password or restrictions.")
            } else if !pdf.locked {
                Note("This file only restricts what can be done with it; no password is needed to remove that.")
            } else {
                Label("This file is protected with a password.", systemImage: "lock.fill").font(.subheadline)
                LabeledField("Password") { SecureField("Password", text: $options.password).textContentType(.password) }
            }
        }
    }
}

@Observable
final class MetadataOptions: ToolOptions {
    static let fields: [(key: String, label: String)] = [
        ("title", "Title"), ("author", "Author"), ("subject", "Subject"), ("keywords", "Keywords"),
        ("creator", "Creator (the original application)"), ("producer", "Producer (the PDF library)"),
    ]
    var values: [String: String] = [:]

    func reset(for session: ToolSession) {
        values = Dictionary(uniqueKeysWithValues: Self.fields.map { ($0.key, session.pdf?.info["metadata"][$0.key].string ?? "") })
    }

    func panel(_ session: ToolSession) -> AnyView { AnyView(MetadataPanel(options: self)) }

    func problem(_ session: ToolSession) -> String? { session.keepProblem }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf else { throw EngineError(message: "Choose a PDF.") }
        let args = ["metadata", pdf.url.path, "OUT"] + Self.fields.map { "--\($0.key)=\(values[$0.key] ?? "")" } + session.keepArgs
        let note = session.removedNote
        return try await runToFile(session, args, name: "\(pdf.baseName)-metadata.pdf") { _ in "Saved the details\(note)." }
    }
}

private struct MetadataPanel: View {
    @Bindable var options: MetadataOptions

    var body: some View {
        ForEach(MetadataOptions.fields, id: \.key) { field in
            LabeledField(field.label) {
                TextField(field.label, text: Binding(get: { options.values[field.key] ?? "" }, set: { options.values[field.key] = $0 }))
            }
        }
    }
}

@Observable
final class GrayscaleOptions: ToolOptions {
    var images = true

    func panel(_ session: ToolSession) -> AnyView { AnyView(GrayscalePanel(options: self)) }

    func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor { CardDecor(gray: true) }

    func problem(_ session: ToolSession) -> String? { session.keepProblem }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf else { throw EngineError(message: "Choose a PDF.") }
        var args = ["grayscale", pdf.url.path, "OUT"] + session.keepArgs
        if !images { args.append("--text-only") }
        let note = session.removedNote
        return try await runToFile(session, args, name: "\(pdf.baseName)-grayscale.pdf") { json in
            "Converted \(plural(json["streams"].int, "content stream")) and \(plural(json["images"].int, "image"))\(note)."
        }
    }
}

private struct GrayscalePanel: View {
    @Bindable var options: GrayscaleOptions

    var body: some View {
        Toggle("Convert images too", isOn: $options.images)
        Note("Text stays real text. Gradients and spot colours are left as they are.")
    }
}
