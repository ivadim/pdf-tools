// What each tool adds to the shared screen: its options panel, what it shows
// on the page cards, what's missing before it can run, and the command.

import SwiftUI

protocol ToolOptions: AnyObject {
    /// The options panel.
    @MainActor func panel(_ session: ToolSession) -> AnyView
    /// Buttons above the page grid.
    @MainActor func gridActions(_ session: ToolSession) -> [CardControl]
    /// How a page card looks and what it can do.
    @MainActor func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor
    /// What's missing before the tool can run.
    @MainActor func problem(_ session: ToolSession) -> String?
    @MainActor func run(_ session: ToolSession) async throws -> ToolResult
    /// Called when a new file is opened.
    @MainActor func reset(for session: ToolSession)
}

extension ToolOptions {
    func gridActions(_ session: ToolSession) -> [CardControl] { [] }
    func card(_ session: ToolSession, item: PageItem, index: Int) -> CardDecor { CardDecor() }
    func reset(for session: ToolSession) {}
}

struct CardControl: Identifiable {
    var id: String { label }
    let symbol: String
    let label: String
    var disabled = false
    let action: () -> Void
}

struct CardDecor {
    var controls: [CardControl] = []
    /// Split: the colour of the file the page goes to.
    var partColor: Color?
    /// Shown dimmed, as not in the result.
    var dimmed = false
    var gray = false
    /// Drawn over the page; gets the page's size in points.
    var overlay: ((CGSize) -> AnyView)?
    /// A tap on the page, at a fraction of its width and height (instead of opening it).
    var onTap: ((CGPoint) -> Void)?
}

// MARK: Shared pieces

let partColors: [Color] = [0x2f6fed, 0xe0752d, 0x1f9d6a, 0xb8459e, 0xc9a227, 0x3aa7c9, 0xd24b4b, 0x7a5cd6].map { Color(hex: $0) }

func fileColor(_ index: Int) -> Color { partColors[index % partColors.count] }

func moveControls(_ session: ToolSession, _ index: Int) -> [CardControl] {
    [
        CardControl(symbol: "chevron.backward", label: "Move earlier", disabled: index == 0) { withAnimation { session.move(index, by: -1) } },
        CardControl(symbol: "chevron.forward", label: "Move later", disabled: index == session.items.count - 1) {
            withAnimation { session.move(index, by: 1) }
        },
    ]
}

func rotateControls(_ session: ToolSession, _ index: Int) -> [CardControl] {
    [
        CardControl(symbol: "rotate.left", label: "Rotate left") { session.items[index].rotation = (session.items[index].rotation + 270) % 360 },
        CardControl(symbol: "rotate.right", label: "Rotate right") { session.items[index].rotation = (session.items[index].rotation + 90) % 360 },
    ]
}

/// The output file `name` in a new folder.
@MainActor func runToFile(_ session: ToolSession, _ args: [String], name: String, message: (JSON) -> String) async throws -> ToolResult {
    let out = try session.outputFolder().appendingPathComponent(name)
    let json = try await Engine.run(args.map { $0 == "OUT" ? out.path : $0 })
    return ToolResult(message: message(json), files: [out])
}

/// "All pages" or the pages typed; used by the stamping tools.
@Observable
final class PageChoice {
    var custom = false
    var text = ""

    /// The kept pages the stamp goes on, worked out once per change rather
    /// than once per page card.
    @ObservationIgnored private var memoKey: [AnyHashable] = []
    @ObservationIgnored private var memo: (stamped: [PageItem], ids: Set<UUID>, error: Error?) = ([], [], nil)

    @MainActor private func evaluate(_ session: ToolSession) -> (stamped: [PageItem], ids: Set<UUID>, error: Error?) {
        let kept = session.kept
        let key: [AnyHashable] = [custom, text, session.pdf?.pageCount ?? 0, kept.map(\.id)]
        if key != memoKey {
            memoKey = key
            if !custom {
                memo = (kept, Set(kept.map(\.id)), nil)
            } else {
                do {
                    let pages = Set(try Engine.parseList(text, total: session.pdf?.pageCount ?? 0))
                    let stamped = kept.filter { pages.contains($0.page) }
                    memo = (stamped, Set(stamped.map(\.id)), nil)
                } catch {
                    memo = ([], [], error)
                }
            }
        }
        return memo
    }

    /// The chosen pages as positions in the result (after left-out pages are gone); nil = all.
    @MainActor func positions(_ session: ToolSession) throws -> [Int]? {
        guard custom else { return nil }
        let result = evaluate(session)
        if let error = result.error { throw error }
        return session.kept.enumerated().filter { result.ids.contains($1.id) }.map { $0.offset + 1 }
    }

    @MainActor func problem(_ session: ToolSession) -> String? {
        guard custom else { return nil }
        let result = evaluate(session)
        if let error = result.error { return error.localizedDescription }
        return result.stamped.isEmpty ? "None of the chosen pages are kept." : nil
    }

    @MainActor func includes(_ session: ToolSession, _ item: PageItem) -> Bool {
        item.keep && evaluate(session).ids.contains(item.id)
    }

    /// The pages the stamp goes on, in order.
    @MainActor func stamped(_ session: ToolSession) -> [PageItem] {
        evaluate(session).stamped
    }

    @MainActor func args(_ session: ToolSession) throws -> [String] {
        guard let pages = try positions(session) else { return [] }
        return ["--pages=\(formatList(pages))"]
    }
}

struct PageChoiceView: View {
    @Bindable var choice: PageChoice

    var body: some View {
        Picker("Pages", selection: $choice.custom) {
            Text("All pages").tag(false)
            Text("Some pages").tag(true)
        }
        .pickerStyle(.segmented)
        if choice.custom {
            LabeledField("Pages") {
                TextField("1-3, 5", text: $choice.text).keyboardType(.numbersAndPunctuation)
            }
        }
    }
}

enum Anchor: String, CaseIterable {
    case topLeft = "top-left", topCenter = "top-center", topRight = "top-right"
    case middleLeft = "middle-left", middleCenter = "middle-center", middleRight = "middle-right"
    case bottomLeft = "bottom-left", bottomCenter = "bottom-center", bottomRight = "bottom-right"

    var vertical: String { String(rawValue.split(separator: "-")[0]) }
    var horizontal: String { String(rawValue.split(separator: "-")[1]) }

    /// Where a w×h box goes on a page at this anchor, `margin` from the edges; its centre.
    func place(page: CGSize, box: CGSize, margin: CGFloat) -> CGPoint {
        let x =
            horizontal == "left" ? margin + box.width / 2 : horizontal == "right" ? page.width - margin - box.width / 2 : page.width / 2
        let y =
            vertical == "top" ? margin + box.height / 2 : vertical == "bottom" ? page.height - margin - box.height / 2 : page.height / 2
        return CGPoint(x: x, y: y)
    }
}

/// A 3×3 (or 3×2) grid of positions on a page.
struct AnchorPicker: View {
    @Binding var anchor: Anchor
    var middle = true

    var body: some View {
        let rows = middle ? ["top", "middle", "bottom"] : ["top", "bottom"]
        Grid(horizontalSpacing: 6, verticalSpacing: 6) {
            ForEach(rows, id: \.self) { row in
                GridRow {
                    ForEach(["left", "center", "right"], id: \.self) { column in
                        let value = Anchor(rawValue: "\(row)-\(column)")!
                        Button {
                            anchor = value
                        } label: {
                            RoundedRectangle(cornerRadius: 4)
                                .fill(value == anchor ? Color.accentColor : Color.secondary.opacity(0.25))
                                .frame(width: 34, height: 24)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("\(row) \(column)")
                        .accessibilityAddTraits(value == anchor ? .isSelected : [])
                    }
                }
            }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 8).strokeBorder(.secondary.opacity(0.4)))
    }
}

/// A label above a control, like a form field.
struct LabeledField<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    init(_ label: String, @ViewBuilder content: () -> Content) {
        self.label = label
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(.subheadline).foregroundStyle(.secondary)
            content.textFieldStyle(.roundedBorder)
        }
    }
}

struct SliderField: View {
    let label: String
    @Binding var value: Double
    let range: ClosedRange<Double>
    let step: Double
    let format: (Double) -> String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(label).font(.subheadline).foregroundStyle(.secondary)
                Spacer()
                Text(format(value)).font(.subheadline.monospacedDigit())
            }
            Slider(value: $value, in: range, step: step)
        }
    }
}

func percent(_ v: Double) -> String { "\(Int((v * 100).rounded()))%" }

struct Note: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        Text(text).font(.footnote).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
    }
}
