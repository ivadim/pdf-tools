// Signature: draw (PencilKit), type or upload a signature, then tap pages to
// place it and drag it into position.

import PencilKit
import SwiftUI

@Observable
final class SignatureOptions: ToolOptions {
    enum Source: String, CaseIterable { case draw = "Draw", type = "Type", upload = "Upload" }

    struct Placement: Identifiable {
        let id = UUID()
        let page: Int
        /// The centre, as fractions of the page.
        var cx: Double
        var cy: Double
    }

    static let fonts = ["Snell Roundhand", "Bradley Hand", "Savoye LET", "Zapfino", "Noteworthy", "Marker Felt"].filter {
        UIFont.familyNames.contains($0)
    }

    var source = Source.draw
    var drawing = PKDrawing() {
        didSet { drawingVersion += 1 }
    }
    /// Changes with every stroke; keys the drawn picture without serialising the drawing.
    @ObservationIgnored private var drawingVersion = 0
    var ink = Color(hex: 0x1a237e)
    var name = ""
    var font = fonts.first ?? Picture.systemFont
    var upload: Picture?
    var width = 0.3
    var placed: [Placement] = []

    @ObservationIgnored private var cachedKey = ""
    @ObservationIgnored private var cachedPicture: Picture?

    var picture: Picture? {
        switch source {
        case .upload:
            return upload
        case .type:
            guard !name.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
            return cached("type|\(name)|\(font)|\(ink.hexString)") { Picture.text(name, family: font, bold: false, color: ink, size: 140) }
        case .draw:
            guard !drawing.strokes.isEmpty else { return nil }
            return cached("draw|\(drawingVersion)|\(ink.hexString)") {
                var image: UIImage?
                UITraitCollection(userInterfaceStyle: .light).performAsCurrent {
                    image = drawing.image(from: drawing.bounds.insetBy(dx: -6, dy: -6), scale: 2)
                }
                return image?.cgImage.flatMap(Picture.init(cgImage:))
            }
        }
    }

    /// Placements belong to the file they were made on.
    func reset(for session: ToolSession) {
        placed = []
    }

    private func cached(_ key: String, _ make: () -> Picture?) -> Picture? {
        if key != cachedKey {
            cachedKey = key
            cachedPicture = make()
        }
        return cachedPicture
    }

    /// A placement's box, as fractions of the page.
    func box(_ placement: Placement, points: CGSize) -> CGRect {
        let w = width
        let h = picture.map { w * $0.size.height * points.width / ($0.size.width * points.height) } ?? w / 3
        let x = min(max(0, placement.cx - w / 2), 1 - w)
        let y = min(max(0, placement.cy - h / 2), max(0, 1 - h))
        return CGRect(x: x, y: y, width: w, height: h)
    }

    func panel(_ session: ToolSession) -> AnyView { AnyView(SignaturePanel(options: self)) }

    func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor {
        guard let picture else { return CardDecor() }
        return CardDecor(
            overlay: { points in
                AnyView(PlacedSignatures(options: self, page: item.page, points: points, picture: picture))
            },
            onTap: { point in
                self.placed.append(Placement(page: item.page, cx: point.x, cy: point.y))
            })
    }

    func problem(_ session: ToolSession) -> String? {
        if picture == nil {
            switch source {
            case .draw: return "Draw your signature."
            case .type: return "Type your name."
            case .upload: return "Choose an image."
            }
        }
        return placed.isEmpty ? "Tap a page to place the signature." : nil
    }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf, let picture else { throw EngineError(message: "Choose a PDF.") }
        let image = try Workspace.write(picture.data, named: "signature.png")
        let places = placed.map { placement in
            let item = session.items.first { $0.page == placement.page }
            let box = box(placement, points: item.flatMap(session.points(of:)) ?? CGSize(width: 612, height: 792))
            return [Double(placement.page), box.minX, box.minY, box.width, box.height].enumerated()
                .map { $0 == 0 ? "\(Int($1))" : String(format: "%.5f", $1) }.joined(separator: ":")
        }
        let args = ["signature", pdf.url.path, "OUT", "--image=\(image.path)", "--place=\(places.joined(separator: ","))"]
        return try await runToFile(session, args, name: "\(pdf.baseName)-signed.pdf") { json in "Signed \(plural(json["pages"].int, "page"))." }
    }
}

/// The signatures on one page; drag to move, × to remove.
private struct PlacedSignatures: View {
    let options: SignatureOptions
    let page: Int
    let points: CGSize
    let picture: Picture
    @State private var dragStart: (id: UUID, cx: Double, cy: Double)?

    var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            ForEach(options.placed.filter { $0.page == page }) { placement in
                let box = options.box(placement, points: points)
                Image(uiImage: picture.image)
                    .resizable()
                    .frame(width: box.width * size.width, height: box.height * size.height)
                    .overlay { Rectangle().strokeBorder(Color.accentColor, style: StrokeStyle(lineWidth: 1, dash: [4, 3])) }
                    .overlay(alignment: .topTrailing) {
                        Button {
                            options.placed.removeAll { $0.id == placement.id }
                        } label: {
                            Image(systemName: "xmark.circle.fill").font(.title3).symbolRenderingMode(.palette)
                                .foregroundStyle(.white, Color.accentColor)
                        }
                        .offset(x: 10, y: -10)
                        .accessibilityLabel("Remove signature")
                    }
                    .position(x: box.midX * size.width, y: box.midY * size.height)
                    .gesture(
                        DragGesture(minimumDistance: 2)
                            .onChanged { drag in
                                guard let index = options.placed.firstIndex(where: { $0.id == placement.id }) else { return }
                                if dragStart?.id != placement.id { dragStart = (placement.id, placement.cx, placement.cy) }
                                guard let start = dragStart else { return }
                                options.placed[index].cx = start.cx + drag.translation.width / size.width
                                options.placed[index].cy = start.cy + drag.translation.height / size.height
                            }
                            .onEnded { _ in
                                // Keep the stored centre where the box was drawn.
                                if let index = options.placed.firstIndex(where: { $0.id == placement.id }) {
                                    let box = options.box(options.placed[index], points: points)
                                    options.placed[index].cx = box.midX
                                    options.placed[index].cy = box.midY
                                }
                                dragStart = nil
                            })
            }
        }
    }
}

private struct SignaturePanel: View {
    @Bindable var options: SignatureOptions

    var body: some View {
        Picker("Signature", selection: $options.source) {
            ForEach(SignatureOptions.Source.allCases, id: \.self) { Text($0.rawValue) }
        }
        .pickerStyle(.segmented)
        switch options.source {
        case .draw:
            SignaturePad(drawing: $options.drawing, ink: UIColor(options.ink))
                .frame(height: 160)
                .clipShape(RoundedRectangle(cornerRadius: 10))
                .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(.secondary.opacity(0.4)))
            Button("Clear", role: .destructive) { options.drawing = PKDrawing() }
                .buttonStyle(.bordered)
                .disabled(options.drawing.strokes.isEmpty)
        case .type:
            LabeledField("Your name") { TextField("Name", text: $options.name).textContentType(.name) }
            Picker("Style", selection: $options.font) {
                ForEach(SignatureOptions.fonts + [Picture.systemFont], id: \.self) { Text($0).font(.custom($0, size: 17)) }
            }
        case .upload:
            if let upload = options.upload {
                Image(uiImage: upload.image).resizable().scaledToFit().frame(maxHeight: 80)
            }
            ImageSourceButtons(title: "Choose an image") { data in options.upload = Picture(imageData: data) }
        }
        if options.source != .upload {
            ColorPicker("Ink", selection: $options.ink, supportsOpacity: false)
                .onChange(of: options.ink) { _, ink in
                    // Strokes already drawn take the new colour too.
                    options.drawing = PKDrawing(strokes: options.drawing.strokes.map { stroke in
                        var stroke = stroke
                        stroke.ink = PKInk(stroke.ink.inkType, color: UIColor(ink))
                        return stroke
                    })
                }
        }
        SliderField(label: "Size", value: $options.width, range: 0.08...0.8, step: 0.02, format: percent)
        Note("Tap a page to place the signature; drag it to move it.")
    }
}

/// A white pad to sign on with a finger or Apple Pencil.
struct SignaturePad: UIViewRepresentable {
    @Binding var drawing: PKDrawing
    let ink: UIColor

    func makeUIView(context: Context) -> PKCanvasView {
        let canvas = PKCanvasView()
        canvas.drawingPolicy = .anyInput
        canvas.backgroundColor = .white
        canvas.overrideUserInterfaceStyle = .light
        canvas.isScrollEnabled = false
        canvas.delegate = context.coordinator
        canvas.drawing = drawing
        canvas.tool = PKInkingTool(.pen, color: ink, width: 4)
        return canvas
    }

    func updateUIView(_ canvas: PKCanvasView, context: Context) {
        context.coordinator.parent = self
        if canvas.drawing != drawing { canvas.drawing = drawing }
        canvas.tool = PKInkingTool(.pen, color: ink, width: 4)
    }

    func makeCoordinator() -> Coordinator { Coordinator(parent: self) }

    final class Coordinator: NSObject, PKCanvasViewDelegate {
        var parent: SignaturePad
        init(parent: SignaturePad) { self.parent = parent }

        func canvasViewDrawingDidChange(_ canvas: PKCanvasView) {
            if parent.drawing != canvas.drawing { parent.drawing = canvas.drawing }
        }
    }
}
