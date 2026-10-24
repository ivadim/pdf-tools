// Page numbers and Watermark: something drawn on chosen pages, previewed on
// each page card.

import SwiftUI

@Observable
final class NumbersOptions: ToolOptions {
    static let formats = ["{n}", "Page {n}", "Page {n} of {total}", "{n} / {total}", "- {n} -"]

    var format = "{n}"
    var anchor = Anchor.bottomCenter
    var start = 1
    var size = 11.0
    var margin = 28.0
    var color = Color.black
    let pages = PageChoice()

    func panel(_ session: ToolSession) -> AnyView { AnyView(NumbersPanel(options: self)) }

    func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor {
        guard pages.includes(session, item) else { return CardDecor() }
        let stamped = pages.stamped(session)
        let position = stamped.firstIndex { $0.id == item.id } ?? 0
        let text = format.replacingOccurrences(of: "{n}", with: "\(start + position)")
            .replacingOccurrences(of: "{total}", with: "\(start + stamped.count - 1)")
        let (anchor, size, margin, color) = (anchor, size, margin, color)
        return CardDecor(overlay: { points in
            AnyView(
                GeometryReader { geometry in
                    let scale = geometry.size.width / max(1, points.width)
                    let alignment: Alignment =
                        switch anchor.horizontal {
                        case "left": anchor.vertical == "top" ? .topLeading : .bottomLeading
                        case "right": anchor.vertical == "top" ? .topTrailing : .bottomTrailing
                        default: anchor.vertical == "top" ? .top : .bottom
                        }
                    Text(text)
                        .font(.custom("Helvetica", fixedSize: max(1, size * scale)))
                        .foregroundStyle(color)
                        .fixedSize()
                        .padding(anchor.vertical == "top" ? .top : .bottom, margin * scale)
                        .padding(anchor.horizontal == "left" ? .leading : anchor.horizontal == "right" ? .trailing : [], margin * scale)
                        .frame(width: geometry.size.width, height: geometry.size.height, alignment: alignment)
                }
                .allowsHitTesting(false))
        })
    }

    func problem(_ session: ToolSession) -> String? {
        if format.trimmingCharacters(in: .whitespaces).isEmpty { return "Enter a format." }
        let latin = format.unicodeScalars.allSatisfy { s in
            (0x20...0x7e).contains(s.value) || (0xa0...0xff).contains(s.value) || "–—‘’“”•…€".unicodeScalars.contains(s)
        }
        if !latin { return "The format can only use Latin characters." }
        if size <= 0 { return "Enter a font size." }
        return session.keepProblem ?? pages.problem(session)
    }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf else { throw EngineError(message: "Choose a PDF.") }
        var args = ["numbers", pdf.url.path, "OUT", "--format=\(format)", "--position=\(anchor.rawValue)", "--start=\(start)"]
        args += ["--size=\(size)", "--margin=\(margin)", "--color=\(color.hexString)"] + session.keepArgs + (try pages.args(session))
        let note = session.removedNote
        return try await runToFile(session, args, name: "\(pdf.baseName)-numbered.pdf") { json in
            "Numbered \(plural(json["pages"].int, "page"))\(note)."
        }
    }
}

private struct NumbersPanel: View {
    @Bindable var options: NumbersOptions

    var body: some View {
        LabeledField("Format") {
            HStack {
                TextField("{n}", text: $options.format).autocorrectionDisabled().textInputAutocapitalization(.never)
                Menu {
                    ForEach(NumbersOptions.formats, id: \.self) { format in
                        Button(format) { options.format = format }
                    }
                } label: {
                    Image(systemName: "list.bullet").padding(6)
                }
                .accessibilityLabel("Formats")
            }
        }
        LabeledField("Position") { AnchorPicker(anchor: $options.anchor, middle: false) }
        Stepper("Start at \(options.start)", value: $options.start, in: -9999...99999)
        SliderField(label: "Font size", value: $options.size, range: 4...72, step: 1) { "\(Int($0)) pt" }
        SliderField(label: "Margin", value: $options.margin, range: 0...200, step: 2) { "\(Int($0)) pt" }
        ColorPicker("Colour", selection: $options.color, supportsOpacity: false)
        PageChoiceView(choice: options.pages)
        Note("Uses Helvetica, so Latin characters only.")
    }
}

@Observable
final class WatermarkOptions: ToolOptions {
    static let margin = 0.05

    enum Mode: String, CaseIterable { case text = "Text", image = "Image" }

    var mode = Mode.text
    var text = "CONFIDENTIAL"
    var family = Picture.systemFont
    var bold = true
    var color = Color(hex: 0xc62828)
    var image: Picture?
    var width = 0.7
    var angle = 45.0
    var opacity = 0.3
    var anchor = Anchor.middleCenter
    var under = false
    let pages = PageChoice()

    @ObservationIgnored private var cachedKey = ""
    @ObservationIgnored private var cachedText: Picture?

    /// The stamp as drawn: the image, or the text in its font.
    var picture: Picture? {
        if mode == .image { return image }
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let key = "\(text)|\(family)|\(bold)|\(color.hexString)"
        if key != cachedKey {
            cachedKey = key
            cachedText = Picture.text(text, family: family, bold: bold, color: color)
        }
        return cachedText
    }

    func panel(_ session: ToolSession) -> AnyView { AnyView(WatermarkPanel(options: self)) }

    func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor {
        guard let picture, pages.includes(session, item) else { return CardDecor() }
        let (width, angle, opacity, anchor, under) = (width, angle, opacity, anchor, under)
        return CardDecor(overlay: { points in
            AnyView(
                GeometryReader { geometry in
                    let scale = geometry.size.width / max(1, points.width)
                    let w = width * points.width
                    let h = w * picture.size.height / max(1, picture.size.width)
                    let rad = angle * .pi / 180
                    let box = CGSize(width: abs(w * cos(rad)) + abs(h * sin(rad)), height: abs(w * sin(rad)) + abs(h * cos(rad)))
                    let centre = anchor.place(page: points, box: box, margin: Self.margin * min(points.width, points.height))
                    Image(uiImage: picture.image)
                        .resizable()
                        .frame(width: w * scale, height: h * scale)
                        .rotationEffect(.degrees(-angle))
                        .opacity(opacity)
                        .blendMode(under ? .multiply : .normal)
                        .position(x: centre.x * scale, y: centre.y * scale)
                }
                .allowsHitTesting(false))
        })
    }

    func problem(_ session: ToolSession) -> String? {
        if mode == .text && text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return "Enter the watermark text." }
        if mode == .image && image == nil { return "Choose an image." }
        if picture == nil { return "Couldn’t draw the watermark." }
        return session.keepProblem ?? pages.problem(session)
    }

    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf, let picture else { throw EngineError(message: "Choose a PDF.") }
        let stamp = try Workspace.write(picture.data, named: "watermark.png")
        var args = ["stamp", pdf.url.path, "OUT", "--image=\(stamp.path)", "--position=\(anchor.rawValue)", "--margin=\(Self.margin)"]
        args += ["--width=\(width)", "--angle=\(angle)", "--opacity=\(opacity)"] + session.keepArgs + (try pages.args(session))
        if under { args.append("--under") }
        let note = session.removedNote
        return try await runToFile(session, args, name: "\(pdf.baseName)-watermarked.pdf") { json in
            "Watermarked \(plural(json["pages"].int, "page"))\(note)."
        }
    }
}

private struct WatermarkPanel: View {
    @Bindable var options: WatermarkOptions

    var body: some View {
        Picker("Watermark", selection: $options.mode) {
            ForEach(WatermarkOptions.Mode.allCases, id: \.self) { Text($0.rawValue) }
        }
        .pickerStyle(.segmented)
        switch options.mode {
        case .text:
            LabeledField("Text") { TextField("CONFIDENTIAL", text: $options.text, axis: .vertical) }
            Picker("Font", selection: $options.family) {
                ForEach(Picture.families, id: \.self) { Text($0) }
            }
            .pickerStyle(.navigationLink)
            Toggle("Bold", isOn: $options.bold)
            ColorPicker("Colour", selection: $options.color, supportsOpacity: false)
        case .image:
            if let image = options.image {
                Image(uiImage: image.image).resizable().scaledToFit().frame(maxHeight: 80)
            }
            ImageSourceButtons(title: "Choose an image") { data in
                options.image = Picture(imageData: data)
            }
        }
        SliderField(label: "Size", value: $options.width, range: 0.1...1, step: 0.05, format: percent)
        SliderField(label: "Angle", value: $options.angle, range: -90...90, step: 5) { "\(Int($0))°" }
        SliderField(label: "Opacity", value: $options.opacity, range: 0.05...1, step: 0.05, format: percent)
        LabeledField("Position") { AnchorPicker(anchor: $options.anchor) }
        Toggle("Behind the page content", isOn: $options.under)
        PageChoiceView(choice: options.pages)
    }
}
