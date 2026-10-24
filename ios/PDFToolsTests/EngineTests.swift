// The Swift side of the Rust engine (Engine.swift): commands, rendering and page ranges.

import Foundation
import Testing

@testable import PDFTools

struct EngineTests {
    @Test func inspectsFiles() throws {
        let tracemonkey = try info(Fixtures.pdf("tracemonkey"))
        #expect(tracemonkey["pages"].int == 14)
        #expect(!tracemonkey["needs_password"].bool)
        #expect(tracemonkey["file_size"].int > 0)

        let locked = try info(Fixtures.locked)
        #expect(locked["needs_password"].bool)
        #expect(locked["encrypted"].bool)

        let restricted = try info(Fixtures.restricted)
        #expect(restricted["encrypted"].bool && !restricted["needs_password"].bool)
    }

    @Test func reportsErrors() {
        #expect(throws: EngineError.self) { try Engine.runNow(["frobnicate"]) }
        #expect(throws: EngineError.self) { try Engine.runNow(["info", "/no/such/file.pdf"]) }
        do {
            _ = try Engine.runNow(["protect", Fixtures.pdf("sizes").path, "/tmp/out.pdf"])
            Issue.record("protect without a password should fail")
        } catch {
            #expect(error.localizedDescription.contains("password"))
        }
    }

    @Test func rendersPages() async throws {
        let page = try await Engine.render(Fixtures.pdf("tracemonkey"), page: 1, width: 300)
        #expect(abs(page.image.width - 300) <= 1)
        #expect(page.image.height > page.image.width)
        #expect(page.points.width > 600 && page.points.width < 620)

        // A quarter turn swaps the sides.
        let turned = try await Engine.render(Fixtures.pdf("tracemonkey"), page: 1, width: 300, rotation: 90)
        #expect(turned.image.width > turned.image.height)

        await #expect(throws: EngineError.self) { try await Engine.render(Fixtures.pdf("tracemonkey"), page: 99, width: 100) }
    }

    @Test func parsesPageRanges() throws {
        #expect(try Engine.parseGroups("1-3; 5, 8-", total: 9) == [[1, 2, 3], [5], [8, 9]])
        #expect(try Engine.parseList("5, 1-3, 2", total: 9) == [1, 2, 3, 5])
        #expect(throws: EngineError.self) { try Engine.parseGroups("4-2", total: 9) }
        #expect(throws: EngineError.self) { try Engine.parseGroups("12", total: 9) }
        #expect(throws: EngineError.self) { try Engine.parseGroups("", total: 9) }
    }

    @Test func formatsPageLists() {
        #expect(formatList([5, 1, 2, 3, 3]) == "1-3, 5")
        #expect(formatList([]) == "")
        #expect(plural(1, "page") == "1 page")
        #expect(plural(2, "page") == "2 pages")
    }
}
