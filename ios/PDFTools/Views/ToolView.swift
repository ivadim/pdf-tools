// The shared tool screen: choose files, set options, check the pages, run.

import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct ToolView: View {
    @State private var session: ToolSession
    @State private var importing = false
    @State private var photos: [PhotosPickerItem] = []
    @State private var viewing: PageItem?
    @Environment(\.horizontalSizeClass) private var sizeClass

    init(tool: Tool) {
        _session = State(initialValue: ToolSession(tool: tool))
    }

    var body: some View {
        Group {
            if isEmpty {
                EmptyToolView(session: session, chooseFiles: chooseFiles, photos: $photos)
            } else if sizeClass == .regular {
                HStack(spacing: 0) {
                    ScrollView { optionsPanel.padding() }
                        .frame(width: 380)
                        .background(Color(.systemGroupedBackground))
                    Divider()
                    ScrollView { pages.padding() }
                }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        optionsPanel
                        pages
                    }
                    .padding()
                }
                .scrollDismissesKeyboard(.interactively)
                .background(Color(.systemGroupedBackground))
            }
        }
        .navigationTitle(session.tool.name)
        .navigationBarTitleDisplayMode(.inline)
        .safeAreaInset(edge: .bottom) {
            if !isEmpty { RunBar(session: session) }
        }
        .toolbar {
            if !isEmpty { ToolbarItem(placement: .topBarTrailing) { addButton } }
        }
        .overlay {
            if session.loading { ProgressView("Opening…").padding().background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12)) }
        }
        .fileImporter(
            isPresented: $importing,
            allowedContentTypes: session.tool.kind == .images ? [.jpeg, .png, .image] : [.pdf],
            allowsMultipleSelection: session.tool.kind != .single
        ) { result in
            guard let urls = try? result.get() else { return }
            Task { await add(urls) }
        }
        .onChange(of: photos) { _, items in
            guard !items.isEmpty else { return }
            Task { await addPhotos(items) }
        }
        .sheet(item: $session.result) { result in ResultView(result: result) }
        .sheet(item: $viewing) { item in PageViewer(item: item, gray: session.tool == .grayscale) }
        .alert("Couldn’t do that", isPresented: Binding(get: { session.error != nil }, set: { if !$0 { session.error = nil } })) {
            Button("OK") {}
        } message: {
            Text(session.error ?? "")
        }
        .alert(
            "This PDF has a password",
            isPresented: Binding(get: { session.lockedFile != nil }, set: { if !$0 { session.lockedFile = nil } })
        ) {
            Button("Unlock it") { if let file = session.lockedFile { session.switchToUnlock(file) } }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Remove the password with Unlock first; you need to know it.")
        }
    }

    private var isEmpty: Bool {
        switch session.tool.kind {
        case .single: session.pdf == nil
        case .merge: session.files.isEmpty
        case .images: session.images.isEmpty
        }
    }

    @ViewBuilder private var addButton: some View {
        switch session.tool.kind {
        case .single:
            Button("Other PDF", systemImage: "doc.badge.arrow.up", action: chooseFiles)
        case .merge:
            Button("Add PDFs", systemImage: "plus", action: chooseFiles)
        case .images:
            Menu {
                PhotosPicker(selection: $photos, matching: .images) { Label("Photo Library", systemImage: "photo.on.rectangle") }
                Button("Files", systemImage: "folder", action: chooseFiles)
            } label: {
                Label("Add images", systemImage: "plus")
            }
        }
    }

    private var optionsPanel: some View {
        VStack(alignment: .leading, spacing: 14) {
            FilesHeader(session: session)
            session.options.panel(session)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 14))
    }

    @ViewBuilder private var pages: some View {
        let actions = session.options.gridActions(session)
        VStack(alignment: .leading, spacing: 12) {
            if !actions.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack {
                        ForEach(actions) { action in
                            Button(action.label, systemImage: action.symbol, action: action.action).buttonStyle(.bordered)
                        }
                    }
                }
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150, maximum: 260), spacing: 14)], spacing: 18) {
                if session.tool.kind == .images, let options = session.options as? ImagesOptions {
                    ForEach(Array(session.images.enumerated()), id: \.element.id) { index, item in
                        ImageCard(session: session, options: options, item: item, index: index)
                    }
                } else {
                    ForEach(Array(session.items.enumerated()), id: \.element.id) { index, item in
                        PageCard(session: session, item: item, index: index, decor: session.options.card(session, item: item, index: index)) {
                            viewing = item
                        }
                    }
                }
            }
        }
    }

    /// Shows the Files picker (or, in UI tests, opens the files they pass).
    private func chooseFiles() {
        if let files = TestHooks.files {
            Task { await add(session.tool.kind == .single ? Array(files.prefix(1)) : files) }
        } else {
            importing = true
        }
    }

    private func add(_ urls: [URL]) async {
        if session.tool.kind == .images {
            for url in urls {
                let scoped = url.startAccessingSecurityScopedResource()
                defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                if let data = try? Data(contentsOf: url), let item = ImagesOptions.item(data, name: url.lastPathComponent) {
                    session.images.append(item)
                } else {
                    session.error = "\(url.lastPathComponent) isn’t an image that can be read."
                }
            }
        } else {
            await session.open(urls)
        }
    }

    private func addPhotos(_ items: [PhotosPickerItem]) async {
        session.loading = true
        defer {
            session.loading = false
            photos = []
        }
        for (i, item) in items.enumerated() {
            guard let data = try? await item.loadTransferable(type: Data.self),
                let image = ImagesOptions.item(data, name: "photo-\(session.images.count + 1)")
            else {
                session.error = "Photo \(i + 1) couldn’t be read."
                continue
            }
            session.images.append(image)
        }
    }
}

/// The open file(s), with page counts and notes.
private struct FilesHeader: View {
    let session: ToolSession

    var body: some View {
        switch session.tool.kind {
        case .single:
            if let pdf = session.pdf {
                HStack(spacing: 10) {
                    Image(systemName: pdf.locked ? "lock.doc" : "doc.richtext").font(.title2).foregroundStyle(session.tool.tint)
                    VStack(alignment: .leading) {
                        Text(pdf.name).font(.headline).lineLimit(1).truncationMode(.middle)
                        Text(pdf.locked ? "Locked · \(formatBytes(pdf.size))" : "\(plural(pdf.pageCount, "page")) · \(formatBytes(pdf.size))")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                if pdf.repaired {
                    Label("This file is damaged; only the pages that could be recovered are shown.", systemImage: "exclamationmark.triangle")
                        .font(.footnote).foregroundStyle(.orange)
                }
            }
        case .merge:
            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array(session.files.enumerated()), id: \.element.id) { index, file in
                    HStack {
                        Circle().fill(fileColor(index)).frame(width: 10, height: 10)
                        Text(file.name).lineLimit(1).truncationMode(.middle)
                        Spacer()
                        Text(plural(file.pageCount, "page")).font(.caption).foregroundStyle(.secondary)
                        Button {
                            withAnimation { session.removeFile(file) }
                        } label: {
                            Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                        }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("Remove \(file.name)")
                    }
                    .font(.subheadline)
                }
            }
        case .images:
            Text(plural(session.images.count, "image")).font(.headline)
        }
    }
}

private struct EmptyToolView: View {
    let session: ToolSession
    let chooseFiles: () -> Void
    @Binding var photos: [PhotosPickerItem]

    var body: some View {
        ContentUnavailableView {
            Label(session.tool.name, systemImage: session.tool.symbol)
        } description: {
            Text(session.tool.summary)
        } actions: {
            switch session.tool.kind {
            case .single:
                Button("Choose a PDF", action: chooseFiles).buttonStyle(.borderedProminent)
            case .merge:
                Button("Choose PDFs", action: chooseFiles).buttonStyle(.borderedProminent)
            case .images:
                PhotosPicker(selection: $photos, matching: .images) { Text("Choose photos") }.buttonStyle(.borderedProminent)
                Button("Choose image files", action: chooseFiles).buttonStyle(.bordered)
            }
        }
    }
}

/// What's missing, or the button that runs the tool.
private struct RunBar: View {
    let session: ToolSession

    var body: some View {
        let problem = session.problem
        HStack(spacing: 12) {
            if let problem {
                Text(problem).font(.footnote).foregroundStyle(.secondary).lineLimit(2).accessibilityIdentifier("problem")
            }
            Spacer(minLength: 0)
            Button {
                Task { await session.run() }
            } label: {
                if session.running {
                    ProgressView().frame(minWidth: 80)
                } else {
                    Text(session.tool.action).frame(minWidth: 80)
                }
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .accessibilityIdentifier("run")
            .disabled(problem != nil || session.running)
        }
        .padding(.horizontal)
        .padding(.vertical, 10)
        .background(.bar)
    }
}
