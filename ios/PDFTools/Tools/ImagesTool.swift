// Images to PDF: photos and image files, one per page.

import PhotosUI
import SwiftUI

@Observable
final class ImagesOptions: ToolOptions {
    enum Size: String, CaseIterable { case a4, letter, fit }
    enum Orientation: String, CaseIterable { case auto, portrait, landscape }

    var size = Size.a4
    var orientation = Orientation.auto
    var margin = 0

    /// The page an image lands on, as the engine lays it out (in points), and the image's size on it.
    func layout(_ image: ImageItem) -> (page: CGSize, image: CGSize) {
        let quarter = image.rotation % 180 == 90
        let (iw, ih) = quarter ? (image.size.height, image.size.width) : (image.size.width, image.size.height)
        let m = CGFloat(margin)
        let page: CGSize
        if size == .fit {
            page = CGSize(width: iw * 0.75 + 2 * m, height: ih * 0.75 + 2 * m)
        } else {
            let (w, h): (CGFloat, CGFloat) = size == .letter ? (612, 792) : (595.28, 841.89)
            let landscape = orientation == .auto ? iw > ih : orientation == .landscape
            page = landscape ? CGSize(width: h, height: w) : CGSize(width: w, height: h)
        }
        let scale = min(max(1, page.width - 2 * m) / max(1, iw), max(1, page.height - 2 * m) / max(1, ih))
        return (page, CGSize(width: iw * scale, height: ih * scale))
    }

    func panel(_ session: ToolSession) -> AnyView { AnyView(ImagesPanel(options: self)) }

    func problem(_ session: ToolSession) -> String? {
        session.images.isEmpty ? "Add at least one image." : nil
    }

    func run(_ session: ToolSession) async throws -> ToolResult {
        let args = ["images", "OUT"] + session.images.map { "\($0.url.path):\($0.rotation)" }
            + ["--size=\(size.rawValue)", "--orientation=\(orientation.rawValue)", "--margin=\(margin)"]
        return try await runToFile(session, args, name: "images.pdf") { json in "Created a PDF with \(plural(json["pages"].int, "page"))." }
    }

    /// Saves picked image data into the workspace (PNG and JPEG as they are).
    static func item(_ data: Data, name: String) -> ImageItem? {
        let isPNG = data.starts(with: [0x89, 0x50, 0x4e, 0x47])
        let isJPEG = data.starts(with: [0xff, 0xd8])
        guard let image = UIImage(data: data) else { return nil }
        var bytes = data
        var ext = isPNG ? "png" : "jpg"
        if !isPNG && !isJPEG {
            // HEIC and others: re-encoded as JPEG, upright.
            guard let jpeg = image.upright?.jpegData(compressionQuality: 0.9) else { return nil }
            bytes = jpeg
            ext = "jpg"
        }
        let base = (name as NSString).deletingPathExtension
        guard let url = try? Workspace.write(bytes, named: "\(base.isEmpty ? "image" : base).\(ext)") else { return nil }
        let size = CGSize(width: image.size.width * image.scale, height: image.size.height * image.scale)
        let thumbnail = image.preparingThumbnail(of: CGSize(width: 400, height: 400 * size.height / max(1, size.width))) ?? image
        return ImageItem(url: url, name: name, size: size, thumbnail: thumbnail)
    }
}

private struct ImagesPanel: View {
    @Bindable var options: ImagesOptions

    var body: some View {
        Picker("Page size", selection: $options.size) {
            Text("A4").tag(ImagesOptions.Size.a4)
            Text("Letter").tag(ImagesOptions.Size.letter)
            Text("Image size").tag(ImagesOptions.Size.fit)
        }
        .pickerStyle(.segmented)
        if options.size != .fit {
            Picker("Orientation", selection: $options.orientation) {
                Text("Automatic").tag(ImagesOptions.Orientation.auto)
                Text("Portrait").tag(ImagesOptions.Orientation.portrait)
                Text("Landscape").tag(ImagesOptions.Orientation.landscape)
            }
            .pickerStyle(.segmented)
        }
        Picker("Margin", selection: $options.margin) {
            Text("No margin").tag(0)
            Text("Small margin").tag(18)
            Text("Large margin").tag(36)
        }
        .pickerStyle(.segmented)
        Note("JPEGs are embedded without re-compression; PNG transparency is kept.")
    }
}

/// One image on its page, as it will come out.
struct ImageCard: View {
    let session: ToolSession
    let options: ImagesOptions
    let item: ImageItem
    let index: Int

    var body: some View {
        let layout = options.layout(item)
        VStack(spacing: 6) {
            Color.clear
                .aspectRatio(5 / 6, contentMode: .fit)
                .overlay {
                    ZStack {
                        Rectangle().fill(.white).shadow(color: .black.opacity(0.15), radius: 2, y: 1)
                        GeometryReader { geometry in
                            let scale = geometry.size.width / layout.page.width
                            Image(uiImage: item.thumbnail)
                                .resizable()
                                .frame(
                                    width: (item.rotation % 180 == 90 ? layout.image.height : layout.image.width) * scale,
                                    height: (item.rotation % 180 == 90 ? layout.image.width : layout.image.height) * scale
                                )
                                .rotationEffect(.degrees(Double(item.rotation)))
                                .position(x: geometry.size.width / 2, y: geometry.size.height / 2)
                        }
                    }
                    .aspectRatio(layout.page.width / layout.page.height, contentMode: .fit)
                }
            HStack(spacing: 2) {
                Text(item.name).font(.caption).lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 0)
                ForEach(controls) { control in
                    Button(action: control.action) {
                        Image(systemName: control.symbol).frame(width: 26, height: 26)
                    }
                    .disabled(control.disabled)
                    .accessibilityLabel(control.label)
                }
            }
            .buttonStyle(.borderless)
        }
        .contextMenu {
            ForEach(controls) { control in
                Button(control.label, systemImage: control.symbol, action: control.action).disabled(control.disabled)
            }
        }
    }

    var controls: [CardControl] {
        [
            CardControl(symbol: "chevron.backward", label: "Move earlier", disabled: index == 0) {
                withAnimation { session.images.swapAt(index, index - 1) }
            },
            CardControl(symbol: "chevron.forward", label: "Move later", disabled: index == session.images.count - 1) {
                withAnimation { session.images.swapAt(index, index + 1) }
            },
            CardControl(symbol: "rotate.right", label: "Rotate") { session.images[index].rotation = (session.images[index].rotation + 90) % 360 },
            CardControl(symbol: "trash", label: "Remove") { withAnimation { _ = session.images.remove(at: index) } },
        ]
    }
}
