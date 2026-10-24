// A page card: the page drawn by the engine, the tool's overlay, a checkbox to
// leave it out and the tool's buttons.

import SwiftUI
import UniformTypeIdentifiers

struct PageCard: View {
    let session: ToolSession
    let item: PageItem
    let index: Int
    let decor: CardDecor
    let open: () -> Void

    private var reorderable: Bool { session.tool == .merge || session.tool == .rearrange }
    private var dimmed: Bool { !item.keep || decor.dimmed }

    var body: some View {
        VStack(spacing: 6) {
            Color.clear
                .aspectRatio(5 / 6, contentMode: .fit)
                .overlay {
                    PageImage(session: session, item: item, rotation: item.rotation, gray: decor.gray) { points in
                        ZStack {
                            if decor.onTap == nil, let overlay = decor.overlay { overlay(points) }
                            GeometryReader { geometry in
                                Color.clear.contentShape(Rectangle())
                                    .accessibilityElement()
                                    .accessibilityLabel(decor.onTap == nil ? "View page \(item.page)" : "Page \(item.page)")
                                    .accessibilityAddTraits(.isButton)
                                    .accessibilityIdentifier("page.\(index + 1)")
                                    .onTapGesture { location in
                                        if let onTap = decor.onTap {
                                            onTap(CGPoint(x: location.x / geometry.size.width, y: location.y / geometry.size.height))
                                        } else {
                                            open()
                                        }
                                    }
                            }
                            // Placed signatures stay on top of the tap area, so they can be dragged.
                            if decor.onTap != nil, let overlay = decor.overlay { overlay(points) }
                        }
                    }
                    .overlay { if let color = decor.partColor { Rectangle().strokeBorder(color, lineWidth: 3) } }
                    .opacity(dimmed ? 0.35 : 1)
                }
                .overlay(alignment: .topLeading) {
                    if session.tool.leavesOutPages { keepButton }
                }
            HStack(spacing: 2) {
                if reorderable { grip }
                Text("\(item.page)").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                if session.tool == .merge, let file = session.files.firstIndex(where: { $0 === item.file }) {
                    Circle().fill(fileColor(file)).frame(width: 7, height: 7)
                }
                Spacer(minLength: 0)
                ForEach(decor.controls) { control in
                    Button(action: control.action) {
                        Image(systemName: control.symbol).frame(width: 26, height: 26)
                    }
                    .disabled(control.disabled)
                    .accessibilityLabel(control.label)
                }
            }
            .buttonStyle(.borderless)
        }
        .contextMenu { menu }
        .modifier(Reorder(enabled: reorderable, session: session, item: item))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Page \(item.page)")
    }

    /// Drag this onto another page to move the page there. Only the handle
    /// starts a drag, so swiping over the pages still scrolls.
    private var grip: some View {
        Image(systemName: "line.3.horizontal")
            .foregroundStyle(.secondary)
            .frame(width: 24, height: 26)
            .contentShape(Rectangle())
            .draggable(item.id.uuidString) {
                Label("Page \(item.page)", systemImage: "doc").padding(8).background(.regularMaterial, in: Capsule())
            }
            .accessibilityHidden(true)
    }

    private var keepButton: some View {
        Button {
            session.items[index].keep.toggle()
        } label: {
            Image(systemName: item.keep ? "checkmark.circle.fill" : "circle")
                .font(.title3)
                .symbolRenderingMode(.palette)
                .foregroundStyle(.white, item.keep ? Color.accentColor : Color.secondary)
                .background(Circle().fill(.black.opacity(item.keep ? 0 : 0.15)))
                .padding(6)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(item.keep ? "Leave out page \(item.page)" : "Include page \(item.page)")
    }

    @ViewBuilder private var menu: some View {
        Button("View", systemImage: "eye", action: open)
        if session.tool.leavesOutPages {
            Button(item.keep ? "Leave out" : "Include", systemImage: item.keep ? "minus.circle" : "plus.circle") {
                session.items[index].keep.toggle()
            }
        }
        ForEach(decor.controls) { control in
            Button(control.label, systemImage: control.symbol, action: control.action).disabled(control.disabled)
        }
    }
}

/// Drop a page's handle on a card to move that page there.
private struct Reorder: ViewModifier {
    let enabled: Bool
    let session: ToolSession
    let item: PageItem

    func body(content: Content) -> some View {
        if enabled {
            content
                .dropDestination(for: String.self) { ids, _ in
                    guard let id = ids.first.flatMap(UUID.init(uuidString:)) else { return false }
                    withAnimation { session.move(id, to: item.id) }
                    return true
                }
        } else {
            content
        }
    }
}

/// A page drawn by the engine, fitted into the space it's given, with an overlay sized to it.
struct PageImage<Overlay: View>: View {
    let session: ToolSession
    let item: PageItem
    let rotation: Int
    let gray: Bool
    @ViewBuilder let overlay: (CGSize) -> Overlay
    @State private var page: RenderedPage?
    @State private var failed = false
    /// The space the page has, rounded up to 40 pt steps so small layout
    /// changes don't render it again.
    @State private var width: CGFloat = 0
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        Group {
            if let page {
                Image(decorative: page.image, scale: displayScale)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .background(.white)
                    .overlay { overlay(page.points) }
                    .shadow(color: .black.opacity(0.15), radius: 2, y: 1)
            } else if failed {
                Image(systemName: "exclamationmark.triangle").foregroundStyle(.secondary)
            } else {
                ProgressView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onGeometryChange(for: CGFloat.self, of: { ($0.size.width / 40).rounded(.up) * 40 }) { width = $0 }
        .task(id: "\(item.file.id)-\(item.page)-\(rotation)-\(gray)-\(width)") {
            guard width > 0 else { return }
            let pixels = Int(width * displayScale)
            let key = "\(item.file.url.path)|\(item.page)|\(pixels)|\(rotation)|\(gray)" as NSString
            if let cached = Thumbnails.cache.object(forKey: key) {
                page = cached.page
            } else {
                do {
                    let rendered = try await Engine.render(item.file.url, page: item.page, width: pixels, rotation: rotation, gray: gray)
                    Thumbnails.cache.setObject(Thumbnails.Entry(rendered), forKey: key, cost: rendered.image.bytesPerRow * rendered.image.height)
                    page = rendered
                } catch {
                    failed = true
                }
            }
            if let page, rotation == 0 { session.pagePoints[ToolSession.key(item.file, item.page)] = page.points }
        }
    }
}

enum Thumbnails {
    final class Entry {
        let page: RenderedPage
        init(_ page: RenderedPage) { self.page = page }
    }

    /// Capped by the pixels' size: a 32nd of the device's memory, at most
    /// 150 MB (about 100 phone-sized pages). iOS also empties it under memory pressure.
    static let cache: NSCache<NSString, Entry> = {
        let cache = NSCache<NSString, Entry>()
        cache.totalCostLimit = Int(min(150 << 20, ProcessInfo.processInfo.physicalMemory / 32))
        return cache
    }()
}

/// A page at full size; pinch or double-tap to zoom.
struct PageViewer: View {
    let item: PageItem
    let gray: Bool
    @State private var page: RenderedPage?
    @State private var failed = false
    @Environment(\.displayScale) private var displayScale
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            GeometryReader { geometry in
                Group {
                    if let page {
                        ZoomableImage(image: UIImage(cgImage: page.image))
                    } else if failed {
                        ContentUnavailableView("Couldn’t draw this page", systemImage: "exclamationmark.triangle")
                    } else {
                        ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
                // The sheet has no size while it opens; render once it does, sharp enough to zoom into.
                .task(id: Int(geometry.size.width)) {
                    guard geometry.size.width > 0, page == nil else { return }
                    let width = Int(min(3000, geometry.size.width * displayScale * 2))
                    do {
                        page = try await Engine.render(item.file.url, page: item.page, width: width, rotation: item.rotation, gray: gray)
                    } catch {
                        failed = true
                    }
                }
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle("Page \(item.page)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }
}

/// An image fitted to its space and centred, in a scroll view that zooms.
private struct ZoomableImage: UIViewRepresentable {
    let image: UIImage

    func makeUIView(context: Context) -> ZoomView {
        ZoomView(image: image)
    }

    func updateUIView(_ view: ZoomView, context: Context) {
        if view.imageView.image !== image { view.imageView.image = image }
    }

    final class ZoomView: UIScrollView, UIScrollViewDelegate {
        let imageView: UIImageView

        init(image: UIImage) {
            imageView = UIImageView(image: image)
            super.init(frame: .zero)
            imageView.backgroundColor = .white
            addSubview(imageView)
            delegate = self
            minimumZoomScale = 1
            maximumZoomScale = 6
            showsHorizontalScrollIndicator = false
            showsVerticalScrollIndicator = false
            contentInsetAdjustmentBehavior = .never
            let doubleTap = UITapGestureRecognizer(target: self, action: #selector(toggleZoom(_:)))
            doubleTap.numberOfTapsRequired = 2
            addGestureRecognizer(doubleTap)
        }

        required init?(coder: NSCoder) { fatalError("not used") }

        private var fittedFor: CGSize = .zero

        override func layoutSubviews() {
            super.layoutSubviews()
            // Fit the page at zoom 1 whenever the space changes (e.g. rotation).
            if bounds.size != fittedFor, let size = imageView.image?.size, size.width > 0, size.height > 0 {
                fittedFor = bounds.size
                zoomScale = 1
                let scale = min(bounds.width / size.width, bounds.height / size.height)
                imageView.frame = CGRect(origin: .zero, size: CGSize(width: size.width * scale, height: size.height * scale))
                contentSize = imageView.frame.size
            }
            centre()
        }

        func viewForZooming(in scrollView: UIScrollView) -> UIView? { imageView }

        func scrollViewDidZoom(_ scrollView: UIScrollView) { centre() }

        /// Keeps the page in the middle while it's smaller than the view.
        private func centre() {
            let x = max(0, (bounds.width - imageView.frame.width) / 2)
            let y = max(0, (bounds.height - imageView.frame.height) / 2)
            contentInset = UIEdgeInsets(top: y, left: x, bottom: y, right: x)
        }

        @objc private func toggleZoom(_ tap: UITapGestureRecognizer) {
            if zoomScale > 1 {
                setZoomScale(1, animated: true)
            } else {
                let point = tap.location(in: imageView)
                let size = CGSize(width: bounds.width / 2.5, height: bounds.height / 2.5)
                zoom(to: CGRect(x: point.x - size.width / 2, y: point.y - size.height / 2, width: size.width, height: size.height), animated: true)
            }
        }
    }
}
