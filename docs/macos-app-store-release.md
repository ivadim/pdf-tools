# macOS App Store preparation

The first macOS delivery target is TestFlight only, at version 1.0 to match
iOS. App Review submission and public availability are separate future
decisions. The separate GitHub desktop release retains the shared engine's
version.

The existing macOS release is a Developer ID signed and notarized disk image;
it is not a Mac App Store build. TestFlight delivery uses a separate signing
and upload job and does not change the existing distribution channel.

## Sandbox testing

The first sandbox test exposed an export failure: the Save panel grants access
to the chosen file, while the engine normally writes a sibling `.part` file.
The macOS fallback now stages the result in a system-provided replacement
directory on the destination volume and uses Foundation to move or replace it.
On macOS 26.7.1, the sandboxed app opened and previewed the test PDF, saved a
new compressed PDF, and replaced that output. The resulting PDF was readable.
Additional flows and provider-backed destinations still need verification.

If a replacement fails, the system replacement directory is retained for
recovery and its location is reported in the error. See Apple's
[FileManager replacement documentation](https://developer.apple.com/documentation/foundation/filemanager/replaceitem%28at%3Awithitemat%3Abackupitemname%3Aoptions%3Aresultingitemurl%3A%29?changes=_2_11).

Apple requires App Sandbox for Mac App Store submissions. The candidate
entitlements enable the sandbox and read/write access to user-selected files.
Build a local, ad-hoc signed test app on macOS with Xcode and the project build
prerequisites installed:

```sh
packaging/macos/sandbox-build.sh "$TMPDIR/PDF Tools Sandbox.app"
open "$TMPDIR/PDF Tools Sandbox.app"
```

The destination must not already exist. This is a local test build, not a
signed App Store package. Test it with ordinary non-private files outside the
app container, and check both success and failure messages:

- Choose and drag in PDFs and JPEG/PNG images; preview pages, merge and export.
- Save to a chosen location, including a different folder and an existing file.
- Split a PDF to a chosen folder and verify each output can be opened.
- Exercise all remaining tools, including password-protected input and redaction.
- Reveal a result in Finder, open the About links, and restart after changing theme.
- Test files on Desktop, Downloads, and a file provider where available.

A save dialog grants access to the selected file; a folder selection grants
access to the selected folder. Check extension handling and replacement behavior
in particular, since the app may append `.pdf` to a selected name. Verify drag
and drop access with the actual packaged app; passing unit tests does not prove
sandbox access. Review sandbox denial logs when a workflow fails. The app uses
`open` to reveal files and open links, which also needs verification.

## Before delivery

- Confirm the macOS platform and bundle identifier in App Store Connect, the
  version and build numbering. Uploading requires a macOS TestFlight profile.
- Check the executable deployment target against `LSMinimumSystemVersion` in
  the final build. With Xcode 27.2, the default Bazel build on the development
  machine produced a macOS 27.2 binary while the bundle declared macOS 11.0.
  Both the sandbox build and the desktop distribution build now explicitly set
  `--macos_minimum_os=11.0`. Confirm compatibility on the oldest supported OS.
- Audit the app and bundled dependencies for privacy-manifest required-reason
  APIs, encryption declarations, and App Store policy requirements.
- Prepare genuine macOS App Store screenshots and listing text; verify public
  support and privacy URLs. The review screenshots are not automatically
  App Store listing assets.
- Configure and validate the delivery workflow below with actual credentials.
- Run the normal desktop checks and the sandboxed end-to-end scenarios above;
  verify the uploaded build after Apple processes it. Do not submit for review
  until the release scope has been explicitly chosen.

Apple references: [App Sandbox](https://developer.apple.com/documentation/security/app-sandbox),
[configuring the sandbox](https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox),
[packaging Mac software](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution),
and [provisioning profiles](https://developer.apple.com/documentation/technotes/tn3125-inside-code-signing-provisioning-profiles).

## Combined TestFlight workflow

The manually started [Apple TestFlight delivery workflow](../.github/workflows/ios-app-store.yml)
selects iOS, macOS, or both. It verifies an `ios-v` or `macos-v` tag on the
current main commit and successful tag-push CI. For a Mac-only delivery use
`macos-v1.0` and enter `1.0` as the macOS version. That version must match
`MACOS_APP_STORE_VERSION` in `version.bzl` and the macOS release tag. The
app bundle and About screen both use that version; iOS and DMG releases retain
their own versioning. iOS or combined delivery requires an `ios-v` tag.
If release changes are needed after tagging, a new Mac-only tag such as
`macos-v1.0-2` can identify the revised commit without changing version 1.0.
With both selected, each platform proceeds independently: iOS can upload even
if macOS fails, so the two uploads are not an atomic release.
An `ios-v` or `macos-v` tag builds desktop packages in CI but does not sign a
Developer ID release or publish a GitHub Release. The iOS choice remains the default.

The signing and upload steps are untested with distribution credentials and
must be reviewed and exercised before relying on them. The macOS job reuses
the protected `macos-signing` environment used by DMG signing; verify its
required reviewers and deployment branch policy allow main before use. Both the iOS and macOS environment
checks run for a combined or macOS-only selection. Signing runs on a separate
runner without checking out or building project code.

Add these secrets to the `macos-signing` environment:

| Secret | Value |
| --- | --- |
| `MAC_APP_DISTRIBUTION_CERTIFICATE` | Base64-encoded Apple Distribution `.p12`, with private key. |
| `MAC_APP_DISTRIBUTION_CERTIFICATE_PASSWORD` | Password for that `.p12`. |
| `MAC_INSTALLER_CERTIFICATE` | Base64-encoded Mac Installer Distribution `.p12`, with private key. |
| `MAC_INSTALLER_CERTIFICATE_PASSWORD` | Password for that `.p12`. |
| `MAC_APP_STORE_PROFILE` | Base64-encoded Mac App Store Connect provisioning profile for the bundle ID. |

The workflow also uses the repository's existing `APPLE_TEAM_ID` and the
`APPLE_API_KEY`, `APPLE_API_KEY_ID`, and `APPLE_API_ISSUER_ID` already in
`macos-signing`. The key is stored as raw `.p8` contents for notarization;
confirm that it can also upload to App Store Connect. The Apple Distribution
certificate may be reused from iOS if it matches the Mac profile; the installer
identity and macOS profile are distinct. Sharing the environment means its
approved jobs can access the same set of environment secrets. Confirm the macOS
platform exists in the same App Store Connect app record before uploading.
A successful upload does not submit the app for review or publish it.
