// PDF Tools for iOS: every tool runs on the device with the same Rust engine
// as the desktop app; files never leave it.

import SwiftUI

@main
struct PDFToolsApp: App {
    init() {
        Workspace.clean()
        Engine.configure()
    }

    var body: some Scene {
        WindowGroup {
            HomeView()
        }
    }
}
