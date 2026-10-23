// Test files shared by the unit tests and the UI tests: the repository's
// tests/fixtures folder (bundled with each test target) and generated images.

import UIKit

private final class BundleToken {}

enum TestFiles {
    static let fixtures = Bundle(for: BundleToken.self).url(forResource: "fixtures", withExtension: nil)!

    /// tracemonkey.pdf (14 pages), rotation.pdf (2), sizes.pdf…
    static func pdf(_ name: String) -> URL { fixtures.appendingPathComponent("pdfjs/\(name).pdf") }

    /// AES-256, opens with the password "test".
    static let locked = fixtures.appendingPathComponent("encrypted/aes256-password-test.pdf")
    /// AES-256 with restrictions but no password to open.
    static let restricted = fixtures.appendingPathComponent("encrypted/aes256-restrictions-only.pdf")

    /// A width×height image, as PNG or JPEG bytes.
    static func image(width: Int, height: Int, jpeg: Bool = false) -> Data {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let image = UIGraphicsImageRenderer(size: CGSize(width: width, height: height), format: format).image { context in
            UIColor.systemTeal.setFill()
            context.fill(CGRect(x: 0, y: 0, width: width, height: height))
            UIColor.systemOrange.setFill()
            context.fill(CGRect(x: 0, y: 0, width: width / 2, height: height / 2))
        }
        return jpeg ? image.jpegData(compressionQuality: 0.8)! : image.pngData()!
    }
}
