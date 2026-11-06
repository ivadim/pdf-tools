// Mark areas on an enlarged page and create an image-only redacted copy.
import SwiftUI

@Observable
final class RedactOptions: ToolOptions {
    struct Mark: Identifiable {
        let id = UUID()
        let page: Int
        let rect: CGRect // fractions from the page's top left
    }
    var marks: [Mark] = []
    var editing: PageItem?
    var smaller = false

    func reset(for session: ToolSession) { marks = []; editing = nil }
    func panel(_ session: ToolSession) -> AnyView {
        AnyView(VStack(alignment: .leading, spacing: 10) {
            Text("Choose a page, then drag over each area to remove. Review the marks before creating the copy.")
            Text("The exported PDF converts every page to an image. Text will not be selectable; links, forms and document details are removed.")
            Toggle("Smaller file", isOn: Binding(get: { self.smaller }, set: { self.smaller = $0 })).disabled(session.running)
            Text("Smaller file lowers resolution and may use JPEG; text may be less sharp.").font(.footnote).foregroundStyle(.secondary)
            Text("\(plural(self.marks.count, "area")) marked.")
            Button("Clear all marks") { self.marks = [] }.disabled(self.marks.isEmpty || session.running)
        }.sheet(item: Binding(get: { self.editing }, set: { self.editing = $0 })) { item in
            RedactEditor(options: self, session: session, item: item)
        })
    }
    func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor {
        CardDecor(overlay: { _ in AnyView(RedactMarks(marks: self.marks.filter { $0.page == item.page })) },
                  onTap: { _ in if !session.running { self.editing = item } })
    }
    func problem(_ session: ToolSession) -> String? { marks.isEmpty ? "Choose a page and mark an area to remove." : nil }
    func run(_ session: ToolSession) async throws -> ToolResult {
        guard let pdf = session.pdf else { throw EngineError(message: "Choose a PDF.") }
        let areas = marks.map { mark in
            let r = mark.rect
            return "\(mark.page):\(r.minX):\(r.minY):\(r.width):\(r.height)"
        }.joined(separator: ",")
        return try await runToFile(session, ["redact", pdf.url.path, "OUT", "--areas=\(areas)", "--quality=\(smaller ? "smaller" : "higher")"], name: "\(pdf.baseName)-redacted.pdf") { json in
            "Created a redacted copy of \(plural(json["pages"].int, "page"))."
        }
    }
}

private struct RedactMarks: View {
    let marks: [RedactOptions.Mark]
    var body: some View {
        GeometryReader { geometry in
            ForEach(marks) { mark in
                Rectangle().fill(.black)
                    .frame(width: mark.rect.width * geometry.size.width, height: mark.rect.height * geometry.size.height)
                    .position(x: mark.rect.midX * geometry.size.width, y: mark.rect.midY * geometry.size.height)
            }
        }.allowsHitTesting(false)
    }
}

private struct RedactEditor: View {
    let options: RedactOptions
    let session: ToolSession
    let item: PageItem
    @Environment(\.dismiss) private var dismiss
    @State private var draft: CGRect?
    @State private var coordinates = ["0", "0", "10", "10"]

    var body: some View {
        NavigationStack {
            VStack {
                Text("Drag across an area to mark it.").font(.subheadline).foregroundStyle(.secondary)
                PageImage(session: session, item: item, rotation: 0, gray: false) { _ in
                    GeometryReader { geometry in
                        let size = geometry.size
                        RedactMarks(marks: options.marks.filter { $0.page == item.page })
                        if let draft {
                            Rectangle().fill(.black.opacity(0.65)).frame(width: draft.width, height: draft.height)
                                .position(x: draft.midX, y: draft.midY)
                        }
                        Color.clear.contentShape(Rectangle()).accessibilityElement().accessibilityLabel("Page marking area")
                            .accessibilityIdentifier("redact.stage").gesture(DragGesture(minimumDistance: 1)
                            .onChanged { value in draft = box(value.startLocation, value.location, size) }
                            .onEnded { value in
                                let r = box(value.startLocation, value.location, size)
                                if r.width > 1 && r.height > 1 {
                                    options.marks.append(.init(page: item.page, rect: CGRect(x: r.minX / size.width, y: r.minY / size.height, width: r.width / size.width, height: r.height / size.height)))
                                }
                                draft = nil
                            })
                    }
                }
                DisclosureGroup("Enter an area") {
                    HStack {
                        ForEach(0..<4) { index in
                            TextField(["Left %", "Top %", "Width %", "Height %"][index], text: $coordinates[index])
                                .keyboardType(.decimalPad)
                                .textFieldStyle(.roundedBorder)
                                .accessibilityLabel(["Left percent", "Top percent", "Width percent", "Height percent"][index])
                        }
                    }
                    Button("Add area") {
                        if let rect = enteredArea { options.marks.append(.init(page: item.page, rect: rect)) }
                    }.disabled(enteredArea == nil)
                }
            }.padding()
                .navigationTitle("Page \(item.page)")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) {
                        Button("Undo") {
                            if let i = options.marks.lastIndex(where: { $0.page == item.page }) { options.marks.remove(at: i) }
                        }
                    }
                    ToolbarItem(placement: .topBarTrailing) { Button("Done") { dismiss() } }
                }
        }
    }
    private var enteredArea: CGRect? {
        let values = coordinates.compactMap(Double.init).map { $0 / 100 }
        guard values.count == 4, values.allSatisfy(\.isFinite), values[0] >= 0, values[1] >= 0,
              values[2] > 0, values[3] > 0, values[0] + values[2] <= 1, values[1] + values[3] <= 1 else { return nil }
        return CGRect(x: values[0], y: values[1], width: values[2], height: values[3])
    }
    private func box(_ a: CGPoint, _ b: CGPoint, _ size: CGSize) -> CGRect {
        let x1 = min(max(a.x, 0), size.width), x2 = min(max(b.x, 0), size.width)
        let y1 = min(max(a.y, 0), size.height), y2 = min(max(b.y, 0), size.height)
        return CGRect(x: min(x1, x2), y: min(y1, y2), width: abs(x2-x1), height: abs(y2-y1))
    }
}
