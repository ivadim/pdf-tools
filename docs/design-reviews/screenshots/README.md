# Screenshot review archive

These are fresh captures of the four PDF Tools apps. They record each app as it runs on its own platform; use them to review product language and platform fit, not as pixel templates. Capture output belongs in `desktop`, `web`, `ios`, or `android`, then `light` or `dark`.

## Coverage

Each theme includes the home screen and, when the app provides it, About. Every tool has an empty screen and a configured screen with representative input loaded.

| App | Home | About | Tool screens per theme | Total per theme |
| --- | --- | --- | ---: | ---: |
| Desktop | Yes | Yes | 13 tools × 2 states | 28 |
| Web | Yes | Yes | 13 tools × 2 states | 28 |
| iOS | Yes | Yes | 13 tools × 2 states | 28 |
| Android | Yes | Yes | 13 tools × 2 states | 28 |

Tool IDs in screenshot filenames are `merge`, `split`, `compress`, `protect`, `unlock`, `rotate`, `rearrange`, `numbers`, `watermark`, `metadata`, `signature`, `grayscale`, and `images`. For example, `ios/dark/tools/merge-empty.png` and `ios/dark/tools/merge-configured.png`.

## Refresh captures

Refresh both themes for any changed screen and all related tool states. The tools below generate screenshots from each app's own implementation; do not share UI code to make the screenshots match.

- **Desktop:** `bazel test //crates/pdftools-app:acceptance --test_arg=--ignored --test_arg=screens`. Copy the `normal` size outputs from the acceptance test results into `desktop/<theme>/`, naming tool captures `tools/<tool>-empty.png` and `tools/<tool>-configured.png`.
- **Web:** build `//web:site`, serve `bazel-bin/web/site` at `http://127.0.0.1:4173`, then run `node web/capture-screenshots.mjs`. It uses local PDF/image fixtures and captures both browser color schemes.
- **iOS:** run the `PDFToolsUITests/testCaptureReviewScreenshots` UI test on an iPhone simulator in `ios/PDFTools.xcodeproj`, then export the test result attachments with `xcrun xcresulttool export attachments`. Sort the named captures into `ios/<theme>/`.
- **Android:** run `android/test.sh` on an emulator once with `PDFTOOLS_THEME=light` and once with `PDFTOOLS_THEME=dark`, setting `PDFTOOLS_SCREENSHOT_DIR` to the corresponding `android/<theme>` folder.

Keep image files under theme folders only. Update this index if the tool catalog or a platform's available screens changes.
