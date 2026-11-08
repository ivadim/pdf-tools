# iOS App Store screenshots

These five iPhone listing images use genuine iOS captures from the light-theme
review set, with a concise headline and one supporting line added above the
app interface. The interface remains the main part of each image; the copy
describes behavior visible in the capture. Files are 1206 × 2622 JPEGs for the
iPhone 6.3-inch display slot.

| Order | File | Feature shown |
| ---: | --- | --- |
| 1 | `01-private-by-design.jpg` | Home and on-device privacy promise |
| 2 | `02-merge-and-arrange.jpg` | Merge and page ordering |
| 3 | `03-compress-pdfs.jpg` | Compression choices and preview |
| 4 | `04-redact-private-details.jpg` | Marking an area for redaction |
| 5 | `05-images-to-pdf.jpg` | Image-to-PDF page options |

The screenshots are built from `docs/design-reviews/screenshots/ios/light/`
using `make-listing-screenshots.swift`. Rebuild them with:

```sh
swift -module-cache-path /private/tmp/pdf-tools-swift-cache \
  docs/app-store/ios/make-listing-screenshots.swift "$PWD"
```

For iPad, use the genuine 13-inch capture at
`docs/app-store/ios/ipad-home.jpg` (2064 × 2752). Its status bar includes the
simulator date; this was explicitly accepted for the listing.
