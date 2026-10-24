// Every tool run end to end on real PDFs, as the app's screens drive them,
// with the outputs checked by the engine.

import Foundation
import PencilKit
import SwiftUI
import Testing

@testable import PDFTools

struct MergeTests {
    @Test func mergesFilesAndPages() async throws {
        let merge = await session(.merge, Fixtures.pdf("tracemonkey"), Fixtures.pdf("rotation"))
        #expect(merge.items.count == 16)
        let result = try await run(merge)
        #expect(result.message == "Merged 2 files into 16 pages.")
        #expect(try pageCount(result.files[0]) == 16)
    }

    @Test func leavesOutPagesAndReorders() async throws {
        let merge = await session(.merge, Fixtures.pdf("tracemonkey"), Fixtures.pdf("rotation"))
        merge.items[0].keep = false
        merge.move(merge.items[15].id, to: merge.items[0].id)
        merge.items[0].rotation = 90
        let result = try await run(merge)
        #expect(try pageCount(result.files[0]) == 15)
    }

    @Test func leavesOutLockedFilesButKeepsTheOthers() async {
        let merge = await session(.merge, Fixtures.pdf("tracemonkey"), Fixtures.locked, Fixtures.pdf("rotation"))
        #expect(merge.files.map(\.name) == ["tracemonkey.pdf", "rotation.pdf"])
        #expect(merge.lockedFile == nil)
        #expect(merge.error?.contains("aes256-password-test.pdf has a password") == true)
        #expect(merge.tool == .merge)
    }

    @Test func needsTwoFiles() async {
        let merge = await session(.merge, Fixtures.pdf("tracemonkey"))
        #expect(merge.problem == "Add at least two PDFs.")
        merge.removeFile(merge.files[0])
        #expect(merge.items.isEmpty)
    }
}

struct SplitTests {
    @Test func splitsByRanges() async throws {
        let split = await session(.split, Fixtures.pdf("tracemonkey"))
        let options = try #require(split.options as? SplitOptions)
        #expect(options.base == "tracemonkey")
        options.ranges = "1, 2-3, 4-"
        let result = try await run(split)
        #expect(result.message == "Created 3 files.")
        #expect(result.files.map(\.lastPathComponent) == ["tracemonkey-1.pdf", "tracemonkey-2.pdf", "tracemonkey-3.pdf"])
        #expect(try result.files.map(pageCount) == [1, 2, 11])
    }

    @Test func splitsEveryNPages() async throws {
        let split = await session(.split, Fixtures.pdf("tracemonkey"))
        let options = try #require(split.options as? SplitOptions)
        options.mode = .every
        options.every = 5
        let result = try await run(split)
        #expect(try result.files.map(pageCount) == [5, 5, 4])
    }

    @Test func extractsPages() async throws {
        let split = await session(.split, Fixtures.pdf("tracemonkey"))
        let options = try #require(split.options as? SplitOptions)
        options.mode = .extract
        options.extract = "2-3, 7"
        let result = try await run(split)
        #expect(result.files.map(\.lastPathComponent) == ["tracemonkey-pages-2-3_7.pdf"])
        #expect(try pageCount(result.files[0]) == 3)
    }

    @Test func explainsBadRanges() async throws {
        let split = await session(.split, Fixtures.pdf("tracemonkey"))
        let options = try #require(split.options as? SplitOptions)
        #expect(split.problem?.contains("Enter page ranges") == true)
        options.ranges = "3-1"
        #expect(split.problem?.contains("backwards") == true)
        options.ranges = "1-3"
        options.base = " "
        #expect(split.problem == "Enter a name for the files.")
        // Pages outside every part are shown dimmed.
        options.base = "part"
        #expect(options.card(split, item: split.items[5], index: 5).dimmed)
        #expect(!options.card(split, item: split.items[0], index: 0).dimmed)
    }
}

struct SingleFileTests {
    @Test func countsImagesOnlyForCompress() async throws {
        let compress = await session(.compress, Fixtures.pdf("tracemonkey"))
        // Opening reads only what every tool needs.
        #expect(compress.pdf?.info["images"].value is NSNull)
        let options = try #require(compress.options as? CompressOptions)
        await options.loadImageStats(try #require(compress.pdf))
        #expect(options.imageStats?.count == 180)
        #expect((options.imageStats?.bytes ?? 0) > 0)
    }

    @Test func compresses() async throws {
        let compress = await session(.compress, Fixtures.pdf("tracemonkey"))
        compress.items[13].keep = false
        let result = try await run(compress)
        #expect(result.message.contains("(1 page left out)"))
        #expect(try pageCount(result.files[0]) == 13)
    }

    @Test func usesFewerThreadsWithLittleMemory() {
        #expect(Engine.threads(memory: 2_900_000_000) == 2)  // 3 GB devices
        #expect(Engine.threads(memory: 3_900_000_000) == 4)  // 4 GB and up
        #expect(Engine.threads(memory: 5_990_000_000) == 4)
    }

    @Test func protectsAndUnlocks() async throws {
        let protect = await session(.protect, Fixtures.pdf("sizes"))
        let options = try #require(protect.options as? ProtectOptions)
        #expect(protect.problem == "Enter a password.")
        options.password = "secret"
        options.confirm = "other"
        #expect(protect.problem == "The passwords don’t match.")
        options.confirm = "secret"
        options.print = false
        let protected = try await run(protect).files[0]
        #expect(try info(protected)["needs_password"].bool)

        // And back again with Unlock.
        let unlock = await session(.unlock, protected)
        #expect(unlock.items.isEmpty)
        #expect(unlock.problem == "Enter the password.")
        let unlockOptions = try #require(unlock.options as? UnlockOptions)
        unlockOptions.password = "secret"
        let unlocked = try await run(unlock).files[0]
        #expect(!(try info(unlocked)["encrypted"].bool))
    }

    @Test func reportsAWrongPassword() async throws {
        let unlock = await session(.unlock, Fixtures.locked)
        try #require(unlock.options as? UnlockOptions).password = "wrong"
        await unlock.run()
        #expect(unlock.result == nil)
        #expect(unlock.error != nil)
    }

    @Test func liftsRestrictionsWithoutAPassword() async throws {
        let unlock = await session(.unlock, Fixtures.restricted)
        #expect(unlock.problem == nil)
        let result = try await run(unlock)
        #expect(!(try info(result.files[0])["encrypted"].bool))

        let plain = await session(.unlock, Fixtures.pdf("sizes"))
        #expect(plain.problem == "Nothing to unlock.")
    }

    @Test func sendsLockedFilesToUnlock() async {
        let watermark = await session(.watermark, Fixtures.locked)
        #expect(watermark.pdf == nil)
        let locked = watermark.lockedFile
        #expect(locked != nil)
        watermark.switchToUnlock(locked!)
        #expect(watermark.tool == .unlock)
        #expect(watermark.pdf === locked)
    }

    @Test func rotates() async throws {
        let rotate = await session(.rotate, Fixtures.pdf("rotation"))
        #expect(rotate.problem == "Rotate a page first.")
        rotateControls(rotate, 0)[1].action()
        #expect(rotate.items[0].rotation == 90)
        let result = try await run(rotate)
        #expect(result.message == "Rotated 1 page.")
        // Page 1 was portrait; turned, it renders landscape.
        let page = try await Engine.render(result.files[0], page: 1, width: 100)
        #expect(page.image.width > page.image.height)
    }

    @Test func rearranges() async throws {
        let rearrange = await session(.rearrange, Fixtures.pdf("tracemonkey"))
        let options = try #require(rearrange.options as? RearrangeOptions)
        options.gridActions(rearrange).first { $0.label == "Reverse" }!.action()
        #expect(rearrange.items.first?.page == 14)
        options.card(rearrange, item: rearrange.items[0], index: 0).controls.first { $0.label == "Duplicate" }!.action()
        options.card(rearrange, item: rearrange.items[2], index: 2).controls.first { $0.label == "Delete" }!.action()
        options.card(rearrange, item: rearrange.items[2], index: 2).controls.first { $0.label == "Delete" }!.action()
        let result = try await run(rearrange)
        #expect(result.message == "Saved 13 pages.")
        #expect(try pageCount(result.files[0]) == 13)
    }

    @Test func editsMetadata() async throws {
        let metadata = await session(.metadata, Fixtures.pdf("sizes"))
        let options = try #require(metadata.options as? MetadataOptions)
        options.values["title"] = "Test title"
        options.values["author"] = "Ada"
        let result = try await run(metadata)
        #expect(try info(result.files[0])["metadata"]["title"].string == "Test title")
        #expect(try info(result.files[0])["metadata"]["author"].string == "Ada")
    }

    @Test func convertsToGrayscale() async throws {
        let grayscale = await session(.grayscale, Fixtures.pdf("calrgb"))
        #expect(grayscale.options.card(grayscale, item: grayscale.items[0], index: 0).gray)
        let result = try await run(grayscale)
        #expect(result.message.hasPrefix("Converted"))
        let page = try await Engine.render(result.files[0], page: 1, width: 60)
        #expect(page.image.width > 0)
    }

    @Test func needsAPageToKeep() async {
        let compress = await session(.compress, Fixtures.pdf("rotation"))
        for i in compress.items.indices { compress.items[i].keep = false }
        #expect(compress.problem == "Keep at least one page.")
    }
}

struct StampTests {
    @Test func numbersPages() async throws {
        let numbers = await session(.numbers, Fixtures.pdf("tracemonkey"))
        let options = try #require(numbers.options as? NumbersOptions)
        options.format = "Page {n} of {total}"
        options.color = Color(hex: 0x336699)
        let result = try await run(numbers)
        #expect(result.message == "Numbered 14 pages.")
    }

    @Test func numbersChosenPagesAfterLeftOutOnes() async throws {
        let numbers = await session(.numbers, Fixtures.pdf("tracemonkey"))
        let options = try #require(numbers.options as? NumbersOptions)
        numbers.items[0].keep = false
        options.pages.custom = true
        options.pages.text = "2-4"
        // Original pages 2-4 are positions 1-3 once page 1 is left out.
        #expect(try options.pages.positions(numbers) == [1, 2, 3])
        #expect(!options.pages.includes(numbers, numbers.items[0]))
        #expect(options.pages.includes(numbers, numbers.items[1]))
        let result = try await run(numbers)
        #expect(result.message == "Numbered 3 pages (1 page left out).")
        #expect(try pageCount(result.files[0]) == 13)
    }

    @Test func followsChangesToTheChosenPages() async throws {
        let numbers = await session(.numbers, Fixtures.pdf("tracemonkey"))
        let pages = try #require(numbers.options as? NumbersOptions).pages
        pages.custom = true
        pages.text = "2-4"
        #expect(pages.stamped(numbers).map(\.page) == [2, 3, 4])
        pages.text = "9"
        #expect(pages.stamped(numbers).map(\.page) == [9])
        numbers.items[8].keep = false
        #expect(pages.problem(numbers) == "None of the chosen pages are kept.")
        pages.text = "20"
        #expect(pages.problem(numbers)?.contains("outside") == true)
        pages.custom = false
        #expect(pages.stamped(numbers).count == 13)
    }

    @Test func rejectsNonLatinNumberFormats() async throws {
        let numbers = await session(.numbers, Fixtures.pdf("sizes"))
        let options = try #require(numbers.options as? NumbersOptions)
        options.format = "Страница {n}"
        #expect(numbers.problem == "The format can only use Latin characters.")
        options.format = "– {n} –"
        #expect(numbers.problem == nil)
    }

    @Test func watermarksWithText() async throws {
        let watermark = await session(.watermark, Fixtures.pdf("tracemonkey"))
        let options = try #require(watermark.options as? WatermarkOptions)
        let picture = try #require(options.picture)
        #expect(picture.size.width > picture.size.height)
        options.under = true
        let result = try await run(watermark)
        #expect(result.message == "Watermarked 14 pages.")
        #expect(options.card(watermark, item: watermark.items[0], index: 0).overlay != nil)
    }

    @Test func watermarksWithAnImageOnSomePages() async throws {
        let watermark = await session(.watermark, Fixtures.pdf("tracemonkey"))
        let options = try #require(watermark.options as? WatermarkOptions)
        options.mode = .image
        #expect(watermark.problem == "Choose an image.")
        options.image = Picture(imageData: Fixtures.image(width: 200, height: 100))
        options.pages.custom = true
        options.pages.text = "1, 3"
        let result = try await run(watermark)
        #expect(result.message == "Watermarked 2 pages.")
        #expect(options.card(watermark, item: watermark.items[1], index: 1).overlay == nil)
    }

    @Test func drawsTextPictures() throws {
        let picture = try #require(Picture.text("Hello\nworld", family: Picture.systemFont, bold: true, color: .red))
        let png = try #require(UIImage(data: picture.data))
        #expect(png.size.height > 100)
        // Trimmed to the text: no fully transparent canvas left over.
        #expect(Picture.text("   ", family: Picture.systemFont, bold: false, color: .red) == nil)
    }

    @Test func placesStampsLikeTheEngine() {
        let page = CGSize(width: 600, height: 800)
        let box = CGSize(width: 100, height: 50)
        #expect(Anchor.middleCenter.place(page: page, box: box, margin: 10) == CGPoint(x: 300, y: 400))
        #expect(Anchor.topLeft.place(page: page, box: box, margin: 10) == CGPoint(x: 60, y: 35))
        #expect(Anchor.bottomRight.place(page: page, box: box, margin: 10) == CGPoint(x: 540, y: 765))
        #expect(Color(hex: 0x336699).hexString == "336699")
    }
}

struct SignatureTests {
    @Test func signsWithATypedName() async throws {
        let signature = await session(.signature, Fixtures.pdf("tracemonkey"))
        let options = try #require(signature.options as? SignatureOptions)
        #expect(signature.problem == "Draw your signature.")
        options.source = .type
        options.name = "Ada Lovelace"
        #expect(signature.problem == "Tap a page to place the signature.")

        // Tapping pages 1 and 3 places it; tapping a placed one's × would remove it.
        let card = options.card(signature, item: signature.items[0], index: 0)
        let tap = try #require(card.onTap)
        tap(CGPoint(x: 0.7, y: 0.9))
        options.card(signature, item: signature.items[2], index: 2).onTap?(CGPoint(x: 0.5, y: 0.5))
        #expect(options.placed.count == 2)

        // Boxes stay on the page even when placed at the edge.
        let box = options.box(options.placed[0], points: CGSize(width: 612, height: 792))
        #expect(box.maxX <= 1 && box.maxY <= 1)

        let result = try await run(signature)
        #expect(result.message == "Signed 2 pages.")
    }

    @Test func forgetsPlacementsWhenAnotherFileOpens() async throws {
        let signature = await session(.signature, Fixtures.pdf("tracemonkey"))
        let options = try #require(signature.options as? SignatureOptions)
        options.placed = [.init(page: 9, cx: 0.5, cy: 0.5)]
        await signature.open([Fixtures.pdf("rotation")])
        #expect(options.placed.isEmpty)
    }

    @Test func redrawsTheSignatureAfterEachStroke() async throws {
        let signature = await session(.signature, Fixtures.pdf("rotation"))
        let options = try #require(signature.options as? SignatureOptions)
        let stroke = { (x: CGFloat) in
            PKStroke(ink: PKInk(.pen, color: .black), path: PKStrokePath(controlPoints: [
                PKStrokePoint(location: CGPoint(x: x, y: 10), timeOffset: 0, size: CGSize(width: 4, height: 4), opacity: 1, force: 1, azimuth: 0, altitude: 0),
                PKStrokePoint(location: CGPoint(x: x + 80, y: 40), timeOffset: 0.1, size: CGSize(width: 4, height: 4), opacity: 1, force: 1, azimuth: 0, altitude: 0),
            ], creationDate: .now))
        }
        options.drawing = PKDrawing(strokes: [stroke(0)])
        let first = try #require(options.picture)
        options.drawing = PKDrawing(strokes: [stroke(0), stroke(200)])
        let second = try #require(options.picture)
        #expect(second.size.width > first.size.width)
    }

    @Test func signsWithAnUploadedImage() async throws {
        let signature = await session(.signature, Fixtures.pdf("rotation"))
        let options = try #require(signature.options as? SignatureOptions)
        options.source = .upload
        #expect(signature.problem == "Choose an image.")
        options.upload = Picture(imageData: Fixtures.image(width: 300, height: 100, jpeg: true))
        options.placed = [.init(page: 2, cx: 0.5, cy: 0.5)]
        let result = try await run(signature)
        #expect(result.message == "Signed 1 page.")
    }
}

struct ImagesTests {
    @Test func makesAPDFFromImages() async throws {
        let images = ToolSession(tool: .images)
        #expect(images.problem == "Add at least one image.")
        let landscape = try #require(ImagesOptions.item(Fixtures.image(width: 400, height: 300, jpeg: true), name: "a.jpg"))
        let portrait = try #require(ImagesOptions.item(Fixtures.image(width: 300, height: 400), name: "b.png"))
        #expect(landscape.url.pathExtension == "jpg" && portrait.url.pathExtension == "png")
        images.images = [landscape, portrait]
        images.images[1].rotation = 90

        let options = try #require(images.options as? ImagesOptions)
        // Automatic orientation follows the image (turned, the portrait one is landscape too).
        #expect(options.layout(images.images[0]).page.width > options.layout(images.images[0]).page.height)
        #expect(options.layout(images.images[1]).page.width > options.layout(images.images[1]).page.height)
        options.size = .letter
        options.orientation = .portrait
        options.margin = 36
        let layout = options.layout(images.images[0])
        #expect(layout.page == CGSize(width: 612, height: 792))
        #expect(abs(layout.image.width - (612 - 72)) < 0.001)

        let result = try await run(images)
        #expect(result.message == "Created a PDF with 2 pages.")
        #expect(try pageCount(result.files[0]) == 2)
    }

    @Test func convertsOtherFormatsToJPEG() throws {
        // HEIC isn't readable by the engine; anything that isn't PNG or JPEG becomes JPEG.
        let heic = try #require(UIImage(data: Fixtures.image(width: 50, height: 40))?.heicData())
        let item = try #require(ImagesOptions.item(heic, name: "photo.heic"))
        #expect(item.url.pathExtension == "jpg")
        #expect(item.size == CGSize(width: 50, height: 40))
        #expect(ImagesOptions.item(Data("not an image".utf8), name: "x") == nil)
    }
}

struct RenameTests {
    @Test func renamesOutputsKeepingTheExtension() async throws {
        let split = await session(.split, Fixtures.pdf("tracemonkey"))
        try #require(split.options as? SplitOptions).ranges = "1, 2-"
        let files = try await run(split).files
        let renamed = try OutputFiles.rename(files[0], to: "  Cover page  ")
        #expect(renamed.lastPathComponent == "Cover page.pdf")
        #expect(FileManager.default.fileExists(atPath: renamed.path))
        #expect(!FileManager.default.fileExists(atPath: files[0].path))
        // Typing the extension doesn't double it; the same name is no change.
        #expect(try OutputFiles.rename(renamed, to: "Intro.PDF").lastPathComponent == "Intro.pdf")
        let intro = renamed.deletingLastPathComponent().appendingPathComponent("Intro.pdf")
        #expect(try OutputFiles.rename(intro, to: "Intro") == intro)
    }

    @Test func refusesBadOrTakenNames() async throws {
        let split = await session(.split, Fixtures.pdf("tracemonkey"))
        try #require(split.options as? SplitOptions).ranges = "1, 2-"
        let files = try await run(split).files
        for bad in ["", "   ", "a/b", "a:b", ".hidden", ".pdf"] {
            #expect(throws: EngineError.self, "\(bad)") { try OutputFiles.rename(files[0], to: bad) }
        }
        #expect(throws: EngineError.self) { try OutputFiles.rename(files[0], to: files[1].deletingPathExtension().lastPathComponent) }
        #expect(FileManager.default.fileExists(atPath: files[0].path), "left as it was")
    }
}
