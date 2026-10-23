// Hooks for the UI tests (ios/PDFToolsUITests). Debug builds only.

import Foundation

enum TestHooks {
    /// Files to open instead of showing the Files picker: the UI tests pass
    /// fixture paths in PDFTOOLS_TEST_FILES, one per line, because the
    /// system's picker runs outside the app and can't be scripted reliably.
    static var files: [URL]? {
        #if DEBUG
        guard let list = ProcessInfo.processInfo.environment["PDFTOOLS_TEST_FILES"], !list.isEmpty else { return nil }
        return list.split(separator: "\n").map { URL(fileURLWithPath: String($0)) }
        #else
        return nil
        #endif
    }
}
