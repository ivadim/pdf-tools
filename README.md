# PDF Tools

[![CI](https://github.com/ivadim/pdf-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/ivadim/pdf-tools/actions/workflows/ci.yml)
[![Licence: MIT](https://img.shields.io/badge/licence-MIT-blue.svg)](LICENSE)

A cross-platform desktop toolset for working with PDFs, written in Rust with a [Slint](https://slint.dev) UI.
Everything runs locally; files never leave your computer.

It is a single native executable, about 15 MB, with no web view and no runtime to install. Idle, it uses about 20 MB of memory.

## Download

Get the latest build for your system from [Releases](https://github.com/ivadim/pdf-tools/releases):

| System | File | How to run |
| --- | --- | --- |
| Windows 10/11 (x64) | `pdftools-…-windows-x86_64.zip` | Unzip and run `pdftools.exe`. The binary isn't code-signed, so SmartScreen may ask you to confirm: choose *More info* → *Run anyway*. |
| macOS 11+ (Apple silicon) | `pdftools-…-macos-arm64.zip` | Unzip and move *PDF Tools.app* to Applications. It isn't notarized, so the first time, right-click it and choose *Open*, or run `xattr -dr com.apple.quarantine "/Applications/PDF Tools.app"`. |
| Linux (x86_64, glibc 2.35+) | `pdftools-…-linux-x86_64.tar.gz` | Extract and run `./pdftools`. Works on X11 and Wayland. File dialogs use the XDG desktop portal, which most desktops provide. |

Builds of the latest `main` are attached to each [CI run](https://github.com/ivadim/pdf-tools/actions/workflows/ci.yml) as artifacts.

## Tools

| Tool | What it does |
| --- | --- |
| **Merge** | Combine PDFs. You can reorder files and individual pages (drag, or Alt+←/→) and rotate pages. |
| **Split** | Split by ranges (`1-3, 4-`), every N pages or each page, or extract selected pages into one file. The preview colours pages by output file. |
| **Compress** | *Lossless* drops unused data. *Balanced* and *Strong* also downscale and re-encode large images as JPEG. |
| **Protect** | AES-256 password, plus optional print, copy, edit and annotate restrictions. |
| **Unlock** | Remove a password (you need to know it) or permission restrictions. |
| **Rotate** | Rotate single pages or all pages. Bookmarks and forms are kept. |
| **Rearrange** | Reorder, duplicate or delete pages. Bookmarks and forms are kept. |
| **Page numbers** | Add page numbers at any of 6 positions with formats like `Page {n} of {total}`, a start number and a page selection. Uses the built-in Helvetica font, so Latin characters only. |
| **Watermark** | Text in any installed font and script, or an image. Set size, angle, opacity, position, and draw above or behind the content. |
| **Metadata** | Edit title, author, subject, keywords, creator and producer. |
| **Signature** | Draw, type or upload a signature, then drag and resize it on one or more pages. |
| **Grayscale** | Converts colours in page content, forms, annotations, palettes and images. Text stays real text. Gradients and spot colours are left as they are. |

**Leaving pages out:** in Merge, Rotate, Compress, Protect, Metadata, Grayscale, Page numbers and Watermark, untick a page's checkbox (Shift-click for a range, or press X) to drop it from the result. Removed pages stay in the preview, dimmed, so you can include them again. They're removed before the tool runs, so page numbers and "{total}" count only the pages that remain.

Every tool previews pages live, including overlays for page numbers and watermarks. Click a page to see it full size. You can drop files anywhere on the window, and a locked file can be handed to Unlock in one click.

## Develop

Prerequisites: Rust (stable). On Linux, also these development packages (Debian/Ubuntu names):

```bash
sudo apt-get install libfontconfig1-dev libxkbcommon-dev libwayland-dev libx11-dev
```

Then:

```bash
cargo run --release -p pdftools-app           # build and start the app
cargo build --release -p pdftools-app         # target/release/pdftools(.exe)
cargo test --workspace                        # every test
```

The release profile is tuned for size (`opt-level = "z"`, LTO, stripped, `panic = "abort"`). The PDF renderer, rasteriser and compression crates are built at `opt-level = 3`, so previews stay fast. Dependencies build at `opt-level = 2` in debug builds too, so debug previews aren't slow.

In debug builds, `PDFTOOLS_SNAPSHOT=out.png PDFTOOLS_OPEN=tool[:file.pdf]` opens a tool, saves a screenshot and exits. This is handy for checking layout changes.

Helper examples:

```bash
cargo run -p pdftools-core --example sample_pdfs -- target/samples          # sample inputs
cargo run -p pdftools-core --example smoke -- target/samples target/smoke   # every operation, for checking by eye
cargo run --release -p pdftools-core --example try_file -- some.pdf target/try   # timings on a real file
```

## Testing

| Layer | Where | What it covers |
| --- | --- | --- |
| Unit | `crates/*/src/**` | Every operation on small generated PDFs, plus the app's tools, page ranges, preview rendering, text and signature rasterising |
| Integration | `crates/pdftools-core/tests/real_pdfs.rs` | Every operation on 15 real-world PDFs (`tests/fixtures/pdfjs`). Checks that outputs reopen, text survives, pages and rotation are right, removed pages are gone, encryption round-trips, no colour is left after grayscale, and damaged files don't crash anything |
| End-to-end | `crates/pdftools-app/tests/ui.rs` | The real Slint UI, driven through Slint's testing backend by accessible labels: clicking, typing, choosing options, reordering, every tool's run on real PDFs, then checking the output files |

```bash
cargo test --workspace                                 # unit + integration + UI
cargo llvm-cov --workspace --fail-under-lines 80       # coverage gate (currently ~93% of lines)
```

UI tests replace native file dialogs with scripted answers (`ScriptedDialogs`). They run background jobs inline, so results are deterministic. The tests find controls by accessible label, so every control needs one. That also keeps the app usable with screen readers.

## Layout

| Path | Purpose |
| --- | --- |
| `crates/pdftools-core` | PDF operations with no GUI code, built on [`lopdf`](https://crates.io/crates/lopdf) and [`image`](https://crates.io/crates/image). |
| `crates/pdftools-app/ui` | Slint UI. `app.slint` is the window, `tool-page.slint` the shared layout of single-file tools, and `tools.slint` their option panels. `merge.slint` and `signature.slint` are the two custom pages. |
| `crates/pdftools-app/src/tools` | One module per tool, implementing the `Tool` trait. Single-file tools share `SingleFile`. |
| `crates/pdftools-app/src/previews.rs` | Page thumbnails rendered with [hayro](https://github.com/LaurenzV/hayro) (pure Rust) on a worker thread, with a cache. |
| `crates/pdftools-app/src/raster.rs` | Watermark and signature text (system fonts, any script) and drawn strokes. |

The UI reports every change to Rust as `Tools.option(tool, key, value)`. Rust validates it and pushes back a `ToolView` (pages, status, run state). Heavy work runs on background threads (`jobs.rs`), and the results are delivered on the UI thread.

## Releases

[GitHub Actions](.github/workflows/ci.yml) runs the tests on Windows, Linux and macOS and checks coverage for every push and pull request. It then builds the three packages. To publish a release, push a version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The tag's run attaches the Windows, Linux and macOS packages to a new GitHub release.

## Notes

- Outputs go through a temporary `.part` file and are then renamed, so overwriting an input is safe.
- Merge and Split rebuild the page tree. Merge drops document-level extras (outlines, forms). Rotate, Rearrange and the other single-file tools keep them.
- Files that only restrict permissions open without a password, and outputs from any tool are saved without those restrictions.

## Licence

MIT. See [LICENSE](LICENSE).

The UI uses [Slint](https://slint.dev) under its [Royalty-free licence](https://slint.dev/terms-and-conditions#royalty-free), which requires the "Made with Slint" attribution shown on the home screen. The test PDFs in `tests/fixtures/pdfjs` come from Mozilla's [pdf.js](https://github.com/mozilla/pdf.js) test suite (Apache-2.0).
