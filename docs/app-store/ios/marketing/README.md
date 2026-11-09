# iOS App Store marketing package

English-language assets and copy for a future App Store submission. These are
prepared for review, not uploaded or submitted. The app's existing icon remains
in the iOS asset catalog.

## Suggested presentation

Use one image per feature in this order. The recommended mix shows both native
appearances without repeating the same screen; the complete alternate set is
provided if a consistent light or dark treatment is preferred.

| Order | File | Message |
| --- | --- | --- |
| 1 | `listing/light/01-on-device.jpg` | On-device tools |
| 2 | `listing/dark/02-merge.jpg` | Merge and reorder |
| 3 | `listing/light/03-compress.jpg` | Compression options |
| 4 | `listing/dark/04-redact.jpg` | Redacted copy |
| 5 | `listing/light/05-images.jpg` | Images to PDF |

Each feature has both light and dark versions. The same filenames and order
are available in these folders:

| Device | Folder | Size | Source |
| --- | --- | --- | --- |
| iPhone 17 | `listing/light/` and `listing/dark/` | 1206 × 2622 | iOS review archive |
| iPhone Duo, outer display | `listing/duo/light/` and `listing/duo/dark/` | 1398 × 2034 | Duo simulator, iOS 27.1 |
| iPad Pro 13-inch | `listing/ipad/light/` and `listing/ipad/dark/` | 2064 × 2752 | iPad simulator, iOS 27.0 |

All are opaque JPEGs. The original device captures are in `captures/duo/` and
`captures/ipad/`; the iPad originals contain the real simulator date. The
finished iPad compositions crop the status bar so the listing images are
date-free. The Duo set shows the outer display; no inner-display set is included.
Actual tool names, document previews, primary actions and native controls remain
visible. The Images to PDF screen uses Image size and No margin so the page fits
the sample photos. Redact shows two marked fictional fields in the membership
summary at `tests/fixtures/review-documents/`. The preview shows marks before
export; the redacted output is image-only, with text and interactive content
removed, as the screen says.

`creative/universal-header-search.png` is a 5244 × 2950 opaque PNG for the
universal header and search-result placement. It contains a genuine dark Merge
screen and a compact value proposition. Check both placements, device sizes,
orientations, cropping and text legibility in App Store Connect Preview before
submission; the artwork has not been checked against Apple's downloadable
safe-area template. Apple's acceptance and review remain authoritative.

## Listing copy

See [listing-copy.md](listing-copy.md). These are proposed English metadata;
confirm URLs, availability and current product behavior before submission.

## Rebuild on macOS

From the repository root, with Xcode's Swift and macOS `sips`:

```sh
swift -module-cache-path /private/tmp/pdf-tools-swift-cache \
  docs/app-store/ios/marketing/make-assets.swift "$PWD" \
  "$PWD/docs/app-store/ios/marketing"
sips -s format png docs/app-store/ios/marketing/creative/universal-header-search.jpg \
  --out docs/app-store/ios/marketing/creative/universal-header-search.png
rm docs/app-store/ios/marketing/creative/universal-header-search.jpg
swift -module-cache-path /private/tmp/pdf-tools-swift-cache \
  docs/app-store/ios/marketing/make-device-listings.swift \
  "$PWD/docs/app-store/ios/marketing"
```

Update the source captures and regenerate after relevant UI changes. Review
both appearances at actual size and at App Store thumbnail size.

To refresh Duo or iPad sources, run `PDFToolsUITests/testCaptureMarketingScreenshots`
on the relevant simulator, export the test result attachments with `xcrun
xcresulttool export attachments`, then run:

```sh
python3 docs/app-store/ios/marketing/import-captures.py duo /path/to/exported-attachments
python3 docs/app-store/ios/marketing/import-captures.py ipad /path/to/exported-attachments
```

Use the export for the corresponding device in each command, then run the
Swift generator above. Check the Duo display mode before capturing.

Apple references, checked 8 November 2026: [asset best practices](https://developer.apple.com/app-store/asset-best-practices/),
[screenshot specifications](https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications/),
and [creative asset specifications](https://developer.apple.com/help/app-store-connect/reference/app-information/creative-assets-specifications/).
