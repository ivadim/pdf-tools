# macOS listing screenshot candidates

These five 2560 × 1600 PNGs are rendered by the desktop app's iced test
simulator on macOS, using fictional sample documents and photographs. They are
review candidates, not captures of a running macOS window. The five were
uploaded to the macOS 1.0 draft in App Store Connect on 14 November 2026.
Review Apple's preview before submission and replace them with genuine Mac
window captures if desired.

Suggested order: home, Merge, Compress, Redact, Images to PDF. The redaction
image shows a mark before export, not a completed export.

To regenerate from the repository root:

```sh
bazel test //crates/pdftools-app:acceptance --test_arg=--ignored --test_arg=macos_listing_screens
```

Copy the five PNGs from `bazel-testlogs/crates/pdftools-app/acceptance/test.outputs/macos-listing/`
and remove the `-tiny-skia` suffix. Verify the images after regenerating.
