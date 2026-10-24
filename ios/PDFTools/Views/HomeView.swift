import SwiftUI

struct HomeView: View {
    @State private var path: [Tool] = []

    var body: some View {
        NavigationStack(path: $path) {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("Everything runs on this device. Your files never leave it.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 160), spacing: 12)], spacing: 12) {
                        ForEach(Tool.allCases) { tool in
                            NavigationLink(value: tool) { ToolTile(tool: tool) }
                                .buttonStyle(.plain)
                                .accessibilityIdentifier("tool.\(tool.rawValue)")
                        }
                    }
                }
                .padding()
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle("PDF Tools")
            .navigationDestination(for: Tool.self) { tool in
                ToolView(tool: tool)
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Link(destination: URL(string: "https://github.com/ivadim/pdf-tools")!) {
                            Label("Source code", systemImage: "chevron.left.forwardslash.chevron.right")
                        }
                        Link(destination: URL(string: "https://buymeacoffee.com/ivadim")!) {
                            Label("Buy me a coffee", systemImage: "cup.and.saucer")
                        }
                        Text("Version \(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "")")
                    } label: {
                        Image(systemName: "info.circle")
                    }
                    .accessibilityLabel("About")
                }
            }
        }
    }
}

private struct ToolTile: View {
    let tool: Tool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Image(systemName: tool.symbol)
                .font(.title3.weight(.semibold))
                .foregroundStyle(.white)
                .frame(width: 40, height: 40)
                .background(tool.tint.gradient, in: RoundedRectangle(cornerRadius: 10))
            Text(tool.name).font(.headline)
            Text(tool.summary)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(3, reservesSpace: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 14))
        .contentShape(RoundedRectangle(cornerRadius: 14))
    }
}
