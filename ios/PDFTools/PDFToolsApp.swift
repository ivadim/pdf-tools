// PDF Tools for iOS: every tool runs on the device with the same Rust engine
// as the desktop app; files never leave it.

import SwiftUI

@main
struct PDFToolsApp: App {
    private var reviewColorScheme: ColorScheme? {
        let arguments = ProcessInfo.processInfo.arguments
        guard let index = arguments.firstIndex(of: "-uiUserInterfaceStyle"), index + 1 < arguments.count else { return nil }
        return arguments[index + 1].lowercased() == "dark" ? .dark : .light
    }

    init() {
        Workspace.clean()
        Engine.configure()
    }

    var body: some Scene {
        WindowGroup {
            HomeView()
                .preferredColorScheme(reviewColorScheme)
        }
    }
}
