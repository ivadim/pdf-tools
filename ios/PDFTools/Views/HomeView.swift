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
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 280), spacing: 10)], spacing: 10) {
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
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: tool.symbol)
                .font(.body.weight(.medium))
                .foregroundStyle(.secondary)
                .frame(width: 24, height: 24)
            VStack(alignment: .leading, spacing: 3) {
                Text(tool.name).font(.headline)
                Text(tool.summary)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 14))
        .contentShape(RoundedRectangle(cornerRadius: 14))
    }
}
