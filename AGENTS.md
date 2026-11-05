# Agent guidance for PDF Tools

For any desktop, web, iOS, or Android interface work, read [docs/design-language.md](docs/design-language.md) first. Preserve its shared product contract while using native components and interaction patterns for the platform you are changing. Do not share UI implementation code across platforms to achieve visual consistency.

Inspect the current interface when reviewing or changing it. When a change affects the UX, update all related screenshots in `docs/design-reviews/screenshots` to reflect the changed screens and states. Also update any other affected screenshots maintained in the repository, such as `docs/screenshots`. Screenshots document the interface; they are not a required pixel match for a redesign. In your change description, note how the result preserves tool names, document preview meaning, primary action and result language, where it follows platform conventions, and which screenshots were updated. If a screenshot cannot be regenerated, report which one and why.

The review archive keeps a complete light and dark set for every app. For each tool, keep both its empty and configured screen; include home and About wherever that surface exists. See [the screenshot index](docs/design-reviews/screenshots/README.md) for the coverage list and platform capture steps.

## Project map

- `crates/pdftools-core` contains PDF operations; `crates/pdftools-render` handles page previews and rendered text/strokes. Both are shared by the desktop, browser, iOS, and Android apps.
- `crates/pdftools-app` is the Rust/iced desktop UI. `web/` is the browser UI and WASI loader. `ios/` is SwiftUI; `android/` is Kotlin/Compose.
- Keep each platform's UI implementation and interaction patterns native. Share PDF behavior through the existing engine/API, not by moving UI code between platforms.
- `tests/fixtures/` contains the small PDFs and images used by automated checks. Avoid adding large or private documents as fixtures.

## Build and test

Use Bazelisk (`bazel`) for Rust, desktop, and WebAssembly builds. The pinned version is in `.bazelversion`; Bazel downloads the Rust toolchain. The README's **Develop** section has host prerequisites and release, packaging, and CI details.

```sh
# Common Rust and desktop checks
bazel test //...                                               # Rust/core/desktop tests
bazel test //crates/pdftools-app:ui                            # desktop UI tests
bazel test //crates/pdftools-app:acceptance_main               # ordered real-file workflows
bazel build --config=rustfmt //...                             # check Rust formatting
bazel build --config=clippy //...                               # Clippy

# Web checks
bazel build //web:site
(cd web && npm ci && npm run lint)
(cd web && npm ci && npx playwright install chromium && npm run test:e2e)

# Native mobile checks (macOS with Xcode for iOS; Android emulator or device for Android)
ios/test.sh
android/test.sh
```

`bazel test //...` does not include the Android UI target or the simulator suites. Run the platform command when changing that app. For focused desktop UI tour screenshots use `bazel test //crates/pdftools-app:ui --test_arg=--ignored --test_arg=tour`; for the full desktop review matrix use `bazel test //crates/pdftools-app:acceptance --test_arg=--ignored --test_arg=screens`. `android/test.sh merge split` runs selected Android flows. iOS UI tests include `testCaptureReviewScreenshots`; see `ios/test.sh` and `ios/PDFToolsUITests/` for simulator selection and result-bundle locations.

When changing the Rust FFI used by mobile apps, rebuild the iOS framework with `ios/build-engine.sh` and rebuild the Android APK through `android/run.sh` or `android/test.sh`. Do not hand-edit generated engine binaries or checked build output. Android builds use Bazel's pinned SDK/NDK; use `android/run.sh` to prepare and start its emulator.

Choose checks based on the changed layer: core or renderer changes need the Rust tests and affected app integrations; UI changes need the corresponding platform UI checks and refreshed screenshots; build/dependency changes need the relevant Bazel, lint, or platform build. Report any check that cannot run and why. Do not claim a platform was validated based only on another platform's tests.

## Change hygiene

- Keep changes scoped to the owning layer and follow nearby code and test patterns. Update user-facing help, docs, and fixtures when behavior or supported inputs change.
- Preserve privacy: document processing stays local. Avoid logging document contents, passwords, or sensitive file paths.
- For new dependencies, update the repository's Bazel/Rust/npm declarations and lockfiles through the existing workflow; do not introduce an unpinned system dependency.
- Before handing off, inspect `git diff --check` and `git status`; summarize the change, checks run, and any remaining validation gaps.
