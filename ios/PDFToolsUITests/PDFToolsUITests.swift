// The real app driven through its UI: choose files, set options, run a tool
// and read the result. The Files picker is a separate system process, so the
// tests hand the app their fixture files instead (TestHooks in the app).

import XCTest

final class PDFToolsUITests: XCTestCase {
    @MainActor private var app: XCUIApplication!

    // MARK: Flows

    @MainActor func testMergeTwoFiles() {
        launch(with: [pdf("tracemonkey"), pdf("rotation")])
        openTool("merge")
        tapWhenReady(app.buttons["Choose PDFs"])
        XCTAssertTrue(app.staticTexts["tracemonkey.pdf"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["rotation.pdf"].exists)
        runTool(expecting: "Merged 2 files into 16 pages.")

        // The result can be renamed before it's shared or saved.
        tapWhenReady(app.buttons["Rename merged.pdf"])
        let name = app.alerts["Rename"].textFields.firstMatch
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        // Return confirms, like tapping Rename.
        name.clearAndType("Combined report\n")
        let alert = app.alerts["Rename"]
        if alert.exists {
            alert.buttons["Rename"].coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        }
        XCTAssertTrue(app.buttons["Rename Combined report.pdf"].waitForExistence(timeout: 5))
    }

    @MainActor func testSplitByRanges() {
        launch(with: [pdf("tracemonkey")])
        openTool("split")
        tapWhenReady(app.buttons["Choose a PDF"])
        let ranges = app.textFields["1-3, 4-6, 7-"]
        XCTAssertTrue(ranges.waitForExistence(timeout: 10))
        XCTAssertEqual(problem, "Enter page ranges, e.g. 1-3, 4-6")
        ranges.tap()
        ranges.typeText("1, 2-3, 4-\n")
        runTool(expecting: "Created 3 files.")
        XCTAssertTrue(app.buttons["tracemonkey-3.pdf"].exists || app.staticTexts["tracemonkey-3.pdf"].exists)
    }

    @MainActor func testWatermarkWithDefaults() {
        launch(with: [pdf("tracemonkey")])
        openTool("watermark")
        tapWhenReady(app.buttons["Choose a PDF"])
        runTool(expecting: "Watermarked 14 pages.")
    }

    @MainActor func testSignWithATypedName() {
        launch(with: [pdf("rotation")])
        openTool("signature")
        tapWhenReady(app.buttons["Choose a PDF"])
        let type = app.buttons["Type"]
        XCTAssertTrue(type.waitForExistence(timeout: 10))
        tapWhenReady(type)
        let name = app.textFields["Name"]
        name.tap()
        name.typeText("Ada Lovelace\n")
        XCTAssertEqual(problem, "Tap a page to place the signature.")

        let page = app.buttons["page.1"]
        scrollTo(page)
        tapWhenReady(page)
        XCTAssertTrue(app.buttons["Remove signature"].waitForExistence(timeout: 5))
        runTool(expecting: "Signed 1 page.")
    }

    @MainActor func testLockedFileGoesToUnlock() {
        launch(with: [locked])
        openTool("compress")
        tapWhenReady(app.buttons["Choose a PDF"])
        let alert = app.alerts["This PDF has a password"]
        XCTAssertTrue(alert.waitForExistence(timeout: 10))
        tapWhenReady(alert.buttons["Unlock it"])
        XCTAssertTrue(app.navigationBars["Unlock"].waitForExistence(timeout: 5))

        let password = app.secureTextFields["Password"]
        password.tap()
        password.typeText("test\n")
        runTool(expecting: "Removed the protection.")
    }

    @MainActor func testImagesToPDF() throws {
        let images = try [image(width: 800, height: 600, name: "landscape.png"), image(width: 600, height: 900, name: "portrait.png")]
        launch(with: images)
        openTool("images")
        tapWhenReady(app.buttons["Choose image files"])
        XCTAssertTrue(app.staticTexts["2 images"].waitForExistence(timeout: 10))
        tapWhenReady(app.buttons["Letter"])
        runTool(expecting: "Created a PDF with 2 pages.")
    }

    @MainActor func testPageViewerOpensAndCloses() {
        launch(with: [pdf("tracemonkey")])
        openTool("grayscale")
        tapWhenReady(app.buttons["Choose a PDF"])
        let page = app.buttons["page.2"]
        scrollTo(page)
        tapWhenReady(page)
        XCTAssertTrue(app.navigationBars["Page 2"].waitForExistence(timeout: 5))
        tapWhenReady(app.buttons["Done"])
        XCTAssertTrue(app.buttons["run"].waitForExistence(timeout: 5))
    }

    // MARK: Helpers

    @MainActor private func launch(with files: [URL]) {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchEnvironment["PDFTOOLS_TEST_FILES"] = files.map(\.path).joined(separator: "\n")
        app.launch()
    }

    @MainActor private func pdf(_ name: String) -> URL { TestFiles.pdf(name) }

    @MainActor private var locked: URL { TestFiles.locked }

    /// A PNG written where the app can read it.
    @MainActor private func image(width: Int, height: Int, name: String) throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(name)
        try TestFiles.image(width: width, height: height).write(to: url)
        return url
    }

    @MainActor private func openTool(_ id: String) {
        let tile = app.buttons["tool.\(id)"]
        XCTAssertTrue(tile.waitForExistence(timeout: 10))
        scrollTo(tile)
        tapWhenReady(tile)
    }

    /// Waits until `element` exists and can be tapped (the CI's simulators are
    /// slow: screens may still be sliding in), then taps it.
    @MainActor private func tapWhenReady(_ element: XCUIElement, timeout: TimeInterval = 20, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(element.waitForExistence(timeout: timeout), "\(element) never appeared", file: file, line: line)
        let hittable = expectation(for: NSPredicate(format: "isHittable == true"), evaluatedWith: element)
        wait(for: [hittable], timeout: timeout)
        element.tap()
    }

    /// Swipes up until `element` is on screen and clear of the run bar at the
    /// bottom (grids only create what's on screen; the bar covers content under it).
    @MainActor private func scrollTo(_ element: XCUIElement) {
        let bottom = app.windows.firstMatch.frame.maxY - 160
        func visible() -> Bool { element.exists && element.isHittable && element.frame.midY < bottom }
        // Let the screen settle first: it may still be appearing.
        _ = element.waitForExistence(timeout: 10)
        for _ in 0..<12 where !visible() {
            let before = element.exists ? element.frame : .null
            app.swipeUp(velocity: .slow)
            // At the end of the screen (no run bar there): as far as it goes.
            if element.exists && element.isHittable && element.frame == before { break }
        }
        XCTAssertTrue(element.exists && element.isHittable, "\(element) isn't on screen")
    }

    @MainActor private var problem: String? {
        let text = app.staticTexts["problem"]
        return text.exists ? text.label : nil
    }

    /// Taps the run button once it's enabled and checks the result sheet.
    @MainActor private func runTool(expecting message: String, file: StaticString = #filePath, line: UInt = #line) {
        let run = app.buttons["run"]
        XCTAssertTrue(run.waitForExistence(timeout: 10), file: file, line: line)
        let enabled = expectation(for: NSPredicate(format: "isEnabled == true"), evaluatedWith: run)
        wait(for: [enabled], timeout: 15)
        run.tap()
        let result = app.staticTexts["result.message"].firstMatch
        let shown = result.waitForExistence(timeout: 30)
        let label = shown ? result.label : app.debugDescription
        XCTAssertTrue(shown && label.contains(message), "expected “\(message)”, got: \(label)", file: file, line: line)
    }
}

extension XCUIElement {
    /// Replaces a text field's contents.
    @MainActor func clearAndType(_ text: String) {
        tap()
        if let value = value as? String, !value.isEmpty {
            typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: value.count))
        }
        typeText(text)
    }
}
