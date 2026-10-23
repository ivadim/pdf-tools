// Helpers for the tool tests (the files themselves are in TestSupport/TestFiles.swift).

import Foundation
import Testing

@testable import PDFTools

typealias Fixtures = TestFiles

/// A tool screen with `files` opened, as if picked in Files.
func session(_ tool: Tool, _ files: URL...) async -> ToolSession {
    let session = ToolSession(tool: tool)
    await session.open(files)
    return session
}

/// Runs the tool and returns its result, failing the test on an error.
func run(_ session: ToolSession) async throws -> ToolResult {
    #expect(session.problem == nil, "problem: \(session.problem ?? "")")
    await session.run()
    #expect(session.error == nil, "error: \(session.error ?? "")")
    return try #require(session.result)
}

/// What the engine reports about a PDF.
func info(_ url: URL) throws -> JSON {
    try Engine.runNow(["info", url.path])
}

func pageCount(_ url: URL) throws -> Int {
    try info(url)["pages"].int
}
