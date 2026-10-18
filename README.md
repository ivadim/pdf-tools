# PDF Tools

[![CI](https://github.com/ivadim/pdf-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/ivadim/pdf-tools/actions/workflows/ci.yml)
[![Licence: MIT](https://img.shields.io/badge/licence-MIT-blue.svg)](LICENSE)

A cross-platform desktop toolset for working with PDFs, written in Rust with an [iced](https://iced.rs) UI.
Everything runs locally; files never leave your computer.

It is a single native executable, about 10 MB, with no web view and no runtime to install. Idle, it uses about 40 MB of memory.

![PDF Tools home screen with all thirteen tools](docs/screenshots/home.png)

<details>
<summary>In the dark theme</summary>

![The home screen in the dark theme](docs/screenshots/home-dark.png)

</details>

## Download

Get the latest build for your system from [Releases](https://github.com/ivadim/pdf-tools/releases):

| System | File | How to run |
| --- | --- | --- |
| Windows 10/11 (x64) | `pdftools-…-windows-x86_64-setup.exe` | Run the installer. It installs for your account without administrator rights (or for all users, if you choose), and adds PDF Tools to the Start menu and to *Settings → Apps* for uninstalling. It isn't code-signed, so SmartScreen may ask you to confirm: choose *More info* → *Run anyway*. |
| Windows, portable | `pdftools-…-windows-x86_64.zip` | Unzip and run `pdftools.exe`; nothing is installed. |
| macOS 11+ (Apple silicon) | `pdftools-…-macos-arm64.dmg` | Open the disk image and drag *PDF Tools* onto *Applications*. The first time, macOS blocks it: see below. |
| Linux (x86_64, glibc 2.35+) | `pdftools-…-linux-x86_64.tar.gz` | Extract and run `./pdftools`. Works on X11 and Wayland. File dialogs use the XDG desktop portal, which most desktops provide. |

**First launch on macOS.** The app isn't notarized by Apple yet, so macOS says *"PDF Tools.app" Not Opened*. To allow it once:

1. Click **Done** in that message.
2. Open **System Settings → Privacy & Security**, scroll down to *Security* and click **Open Anyway** next to "PDF Tools.app was blocked".
3. Confirm with your password, then click **Open**.

Or run this in Terminal, then open the app as usual:

```bash
xattr -dr com.apple.quarantine "/Applications/PDF Tools.app"
```

Builds of the latest `main` are attached to each [CI run](https://github.com/ivadim/pdf-tools/actions/workflows/ci.yml) as artifacts.

## Tools

| Tool | What it does |
| --- | --- |
| **Merge** | Combine PDFs. You can reorder files and individual pages (with the arrows that appear on each) and rotate pages. |
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
| **Grayscale** | Converts colours in page content, forms, annotations, palettes and images. Text stays real text. Gradients and spot colours are left as they are, and calibrated colours are converted as plain RGB. |
| **Images to PDF** | Turn JPEG and PNG images into a PDF, one per page. Reorder and rotate them, and choose the page size (A4, Letter or the image's own size), orientation and margin. JPEGs are embedded without re-compression, camera rotation (EXIF) is honoured and PNG transparency is kept. |

| | |
| --- | --- |
| ![Merging three PDFs](docs/screenshots/merge.png) | ![Splitting a PDF by page ranges](docs/screenshots/split.png) |
| **Merge**: reorder files and pages, rotate or leave out pages | **Split**: pages are coloured by the file they'll end up in |
| ![A watermark previewed on every page](docs/screenshots/watermark.png) | ![Photos turned into PDF pages](docs/screenshots/images.png) |
| **Watermark**: see the result on every page before saving | **Images to PDF**: each photo on a page of the right shape |

**Leaving pages out:** in Merge, Rotate, Compress, Protect, Metadata, Grayscale, Page numbers and Watermark, untick a page's checkbox (Shift-click for a range) to drop it from the result. Removed pages stay in the preview, dimmed, so you can include them again. They're removed before the tool runs, so page numbers and "{total}" count only the pages that remain.

Every tool previews pages live, including overlays for page numbers and watermarks. Click a page to see it full size. You can drop files anywhere on the window, and a locked file can be handed to Unlock in one click.

The app follows the system's light or dark appearance; the switch at the bottom of the sidebar picks one instead, and is remembered.

## Develop

The build uses [Bazel](https://bazel.build) with [rules_rust](https://github.com/bazelbuild/rules_rust); it downloads its own Rust toolchain. Prerequisites: [Bazelisk](https://github.com/bazelbuild/bazelisk) (`brew install bazelisk`, `winget install Bazel.Bazelisk`, or `npm install -g @bazel/bazelisk`), which runs the Bazel version in `.bazelversion`. On Linux, also these development packages (Debian/Ubuntu names):

```bash
sudo apt-get install libfontconfig1-dev libxkbcommon-dev libwayland-dev libx11-dev
```

Then:

```bash
bazel run --config=release //crates/pdftools-app:pdftools     # build and start the app
bazel build --config=release //crates/pdftools-app:pdftools   # bazel-bin/crates/pdftools-app/pdftools(.exe)
bazel test //...                                              # every test
bazel build --config=rustfmt //...                            # check formatting
bazel run @rules_rust//:rustfmt                               # fix formatting
```

Third-party crates are still declared in the crates' `Cargo.toml` files and pinned by `Cargo.lock`; Bazel reads both (crate_universe in `MODULE.bazel`), so add or update a dependency there and Bazel picks it up. The version lives in `version.bzl`.

`--config=release` (in `.bazelrc`) is tuned for size (`opt-level = "z"`, LTO, stripped, `panic = "abort"`). The PDF renderer, rasteriser and compression crates are built at `opt-level = 3`, so previews stay fast. Dependencies build at `opt-level = 2` in development builds too, so debug previews aren't slow.

The UI tests save screenshots of the main screens, in both themes, to `bazel-testlogs/crates/pdftools-app/ui/test.outputs/iced-shots`, which is handy for checking layout changes. `bazel test //crates/pdftools-app:ui --test_arg=--ignored --test_arg=tour` adds more of them.

Helper examples (`bazel run` starts them in Bazel's own directory, so give them absolute paths):

```bash
bazel run //crates/pdftools-core:sample_pdfs -- "$PWD/samples"                  # sample inputs
bazel run //crates/pdftools-core:sample_images -- "$PWD/samples"                # sample photos for Images to PDF
bazel run //crates/pdftools-core:smoke -- "$PWD/samples" "$PWD/smoke"           # every operation, for checking by eye
bazel run --config=release //crates/pdftools-core:try_file -- "$PWD/some.pdf" "$PWD/try"   # timings on a real file
```

## Testing

| Layer | Where | What it covers |
| --- | --- | --- |
| Unit | `crates/*/src/**` | Every operation on small generated PDFs, plus the app's tools, page ranges, preview rendering, text and signature rasterising |
| Integration | `crates/pdftools-core/tests/real_pdfs.rs` | Every operation on 15 real-world PDFs (`tests/fixtures/pdfjs`). Checks that outputs reopen, text survives, pages and rotation are right, removed pages are gone, encryption round-trips, no colour is left after grayscale, and damaged files don't crash anything |
| End-to-end | `crates/pdftools-app/tests/ui.rs` | The real app (`update` and `view`), driven through iced's headless simulator by clicking visible text and by the messages the widgets send: choosing files and options, reordering, every tool's run on real PDFs, then checking the output files |

```bash
bazel test //...                                 # unit + integration + UI
bazel coverage --combined_report=lcov //...      # coverage, in bazel-out/_coverage/_coverage_report.dat (CI requires 80% of lines; currently ~89%)
```

UI tests replace native file dialogs with scripted answers (`ScriptedDialogs`). They run background jobs inline, so results are deterministic.

## Layout

| Path | Purpose |
| --- | --- |
| `crates/pdftools-core` | PDF operations with no GUI code, built on [`lopdf`](https://crates.io/crates/lopdf) and [`image`](https://crates.io/crates/image). |
| `crates/pdftools-render` | Shared rendering: pages with [hayro](https://github.com/LaurenzV/hayro) (pure Rust), watermark and signature text (system fonts, any script), drawn strokes and page ranges. |
| `crates/pdftools-app/src/app.rs` | The [iced](https://iced.rs) application: state, messages, `update`, `view`, the sidebar and the home screen. |
| `crates/pdftools-app/src/single.rs`, `src/tools` | Tools that work on one PDF share `Single` (the file, pages left out, the last run); each implements `ToolOptions` for its options panel, preview overlays and job. |
| `crates/pdftools-app/src/merge.rs`, `src/images.rs` | The two multi-file tools. |
| `crates/pdftools-app/src/previews.rs` | Page thumbnails rendered in the background, with a cache. |
| `crates/pdftools-app/src/theme.rs`, `src/widgets.rs` | Light and dark colours, widget styles and shared building blocks. |

Every change in the UI is a `Message`; `update` applies it and starts heavy work (rendering, running a tool) as a background task whose result comes back as another message.

## Releases

[GitHub Actions](.github/workflows/ci.yml) runs the tests on Windows, Linux and macOS and checks coverage for every push and pull request. It then builds the packages: the Windows installer ([Inno Setup](https://jrsoftware.org/isinfo.php), `packaging/windows/pdftools.iss`) and portable zip, the macOS disk image and the Linux archive. It also tries them out: it installs and uninstalls the Windows setup silently, and mounts the disk image and checks the app inside. To publish a release, push a version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The tag's run attaches all four packages to a new GitHub release.

## Notes

- Outputs go through a temporary `.part` file and are then renamed, so overwriting an input is safe.
- Merge and Split rebuild the page tree. Merge drops document-level extras (outlines, forms). Rotate, Rearrange and the other single-file tools keep them.
- Damaged files, such as ones cut short by an interrupted download, open with the pages that can be recovered; the app says so, and outputs contain only those pages.
- Files that only restrict permissions open without a password, and outputs from any tool are saved without those restrictions.

## Licence

MIT. See [LICENSE](LICENSE).

The test PDFs in `tests/fixtures/pdfjs` come from Mozilla's [pdf.js](https://github.com/mozilla/pdf.js) test suite (Apache-2.0).
