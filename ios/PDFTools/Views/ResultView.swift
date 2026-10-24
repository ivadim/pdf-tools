// A finished run: preview, rename, share or save the files.

import QuickLook
import SwiftUI

struct ResultView: View {
    let result: ToolResult
    /// The outputs, under their current names.
    @State private var files: [URL]
    @State private var preview: URL?
    @State private var saving = false
    @State private var copies: [URL] = []
    @State private var saved: String?
    @State private var renaming: URL?
    @State private var newName = ""
    @State private var renameError: String?
    @Environment(\.dismiss) private var dismiss

    init(result: ToolResult) {
        self.result = result
        _files = State(initialValue: result.files)
    }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Label(result.message, systemImage: "checkmark.circle.fill").foregroundStyle(.green)
                        .accessibilityIdentifier("result.message")
                }
                Section(files.count == 1 ? "File" : "Files") {
                    ForEach(files, id: \.self) { url in
                        row(url)
                    }
                }
                if let saved {
                    Section { Label(saved, systemImage: "folder").foregroundStyle(.secondary) }
                }
            }
            .navigationTitle("Done")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Close") { dismiss() } }
                ToolbarItemGroup(placement: .bottomBar) {
                    Button("Save to Files", systemImage: "folder") { saveToFiles() }
                    Spacer()
                    ShareLink(items: files) { Label("Share", systemImage: "square.and.arrow.up") }
                }
            }
            .quickLookPreview($preview, in: files)
            .fileMover(isPresented: $saving, files: copies) { outcome in
                if case .success(let urls) = outcome, !urls.isEmpty {
                    saved = "Saved \(plural(urls.count, "file")) to Files."
                }
            }
            .alert("Rename", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } }), presenting: renaming) { url in
                TextField("Name", text: $newName)
                    .autocorrectionDisabled()
                    .submitLabel(.done)
                    .onSubmit {
                        rename(url)
                        renaming = nil
                    }
                    .accessibilityIdentifier("rename.name")
                Button("Rename") { rename(url) }
                Button("Cancel", role: .cancel) {}
            } message: { url in
                Text("“\(url.lastPathComponent)” will keep its .\(url.pathExtension) extension.")
            }
            .alert("Couldn’t rename", isPresented: Binding(get: { renameError != nil }, set: { if !$0 { renameError = nil } })) {
                Button("OK") {}
            } message: {
                Text(renameError ?? "")
            }
        }
    }

    private func row(_ url: URL) -> some View {
        HStack {
            Button {
                preview = url
            } label: {
                HStack {
                    Image(systemName: "doc.richtext").foregroundStyle(.tint)
                    VStack(alignment: .leading) {
                        Text(url.lastPathComponent).foregroundStyle(.primary).lineLimit(1).truncationMode(.middle)
                        Text(formatBytes(size(url))).font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            Button {
                startRenaming(url)
            } label: {
                Image(systemName: "pencil").frame(width: 32, height: 32)
            }
            .accessibilityLabel("Rename \(url.lastPathComponent)")
        }
        .buttonStyle(.borderless)
        .contextMenu {
            Button("Rename", systemImage: "pencil") { startRenaming(url) }
        }
        .swipeActions(edge: .trailing) {
            Button("Rename", systemImage: "pencil") { startRenaming(url) }.tint(.accentColor)
        }
    }

    private func startRenaming(_ url: URL) {
        newName = url.deletingPathExtension().lastPathComponent
        renaming = url
    }

    private func rename(_ url: URL) {
        do {
            let renamed = try OutputFiles.rename(url, to: newName)
            if let index = files.firstIndex(of: url) { files[index] = renamed }
        } catch {
            renameError = error.localizedDescription
        }
    }

    private func size(_ url: URL) -> Int {
        (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
    }

    /// Moves copies, so the files stay here for sharing and previews.
    private func saveToFiles() {
        guard let folder = try? Workspace.newFolder("Saving") else { return }
        copies = files.compactMap { url in
            let copy = folder.appendingPathComponent(url.lastPathComponent)
            return (try? FileManager.default.copyItem(at: url, to: copy)) != nil ? copy : nil
        }
        saving = !copies.isEmpty
    }
}

enum OutputFiles {
    /// Renames an output in its folder, keeping its extension (typing it again
    /// is fine). Names can't contain “/” or “:” or start with a dot.
    static func rename(_ url: URL, to name: String) throws -> URL {
        let ext = url.pathExtension
        var base = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if !ext.isEmpty, base.lowercased().hasSuffix("." + ext.lowercased()) {
            base = String(base.dropLast(ext.count + 1)).trimmingCharacters(in: .whitespaces)
        }
        guard !base.isEmpty, !base.hasPrefix("."), !base.contains("/"), !base.contains(":") else {
            throw EngineError(message: "Enter a name without “/” or “:”.")
        }
        var target = url.deletingLastPathComponent().appendingPathComponent(base)
        if !ext.isEmpty { target.appendPathExtension(ext) }
        if target.lastPathComponent == url.lastPathComponent { return url }
        if FileManager.default.fileExists(atPath: target.path), target.lastPathComponent.lowercased() != url.lastPathComponent.lowercased() {
            throw EngineError(message: "There’s already a file called “\(target.lastPathComponent)”.")
        }
        try FileManager.default.moveItem(at: url, to: target)
        return target
    }
}
