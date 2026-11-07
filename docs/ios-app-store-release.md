# iOS App Store release preparation

## Product information draft

- Name: PDF Tools
- Platform: iOS and iPadOS, iOS 18 or later
- Version: 0.5.1; the existing `v0.5.0` tag predates this iOS release
  preparation and must not be reused.
- Bundle ID: `io.github.ivadim.pdftools`
- Price and availability: free, worldwide
- Suggested primary category: Productivity
- Support URL: https://github.com/ivadim/pdf-tools/issues
- Privacy policy: publish the HTML page at [web/privacy.html](../web/privacy.html)
  through the GitHub Pages workflow at https://ivadim.github.io/pdf-tools/privacy.html.
  Keep it in sync with [the iOS privacy policy text](ios-privacy-policy.md), and
  add the public URL in App Store Connect after the Pages workflow deploys it.

### Subtitle

PDF editing on your device

### Description

Work with PDFs on your iPhone or iPad. PDF Tools processes documents on your
device, with no account, ads or tracking.

- Merge and split PDFs, rearrange or rotate pages, and reduce file size.
- Protect a document with a password or unlock one you can access.
- Add page numbers, watermarks and signatures, or edit document details.
- Redact marked areas, convert pages to grayscale, and create PDFs from images.

Choose files from your device or a file provider, preview your pages, and save
or share the result.

### Keywords

merge,split,compress,sign,redact,rotate,watermark,encrypt,pages,photos

## Before the TestFlight upload

- Confirm the app record, bundle ID, team, rights, age rating, category, review
  contact, export compliance and App Store privacy answers in App Store Connect.
- Publish the privacy policy and verify the support and policy URLs are public.
- The privacy manifest declares file metadata access in the app container
  (`C617.1`), used to obtain document sizes. Review the app and its dependencies
  for any additional required-reason APIs and validate the manifest with the
  distribution archive.
- The [regular iPhone 17 home capture](app-store/ios/iphone-home.jpg) is
  1206 × 2622 pixels. The [13-inch iPad Pro home capture](app-store/ios/ipad-home.jpg)
  is 2064 × 2752 pixels and includes the simulator's real date in its status
  bar. Both are genuine simulator captures. Consider additional tool and result
  screens before a future App Store submission.
- Run `ios/test.sh` and confirm the tag's GitHub CI run passes. Configure and
  use the GitHub workflow below for all release signing and uploads; do not
  sign or upload a release build locally. Wait for Apple's processing and test
  the uploaded build with TestFlight.
- The current release goal stops after the build is uploaded for TestFlight.
  App Review submission and public App Store availability are separate future
  actions.

Apple's current instructions: [app privacy](https://developer.apple.com/help/app-store-connect/manage-app-information/manage-app-privacy/),
[required-reason APIs](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api),
[screenshots](https://developer.apple.com/help/app-store-connect/reference/screenshot-specifications/),
and [uploading builds](https://developer.apple.com/help/app-store-connect/manage-builds/upload-builds/).

## GitHub delivery setup

The [iOS App Store delivery workflow](../.github/workflows/ios-app-store.yml)
is manually started and does not run when a tag is pushed. It checks that it
was started from `main` at the exact commit targeted by the supplied `vX.Y.Z`
tag, and that CI passed on that tag. It builds the release engine on
a separate runner without the remote cache, then waits for the `ios-app-store`
environment approval before signing, validating, and uploading a build to App
Store Connect. Uploading a build does not submit it to App Review or publish it.

Before using it:

1. Verify the `ios-app-store` environment in GitHub repository settings has
   required reviewers and a deployment branch rule allowing `main`. These were
   configured for this repository during preparation. The workflow refuses to
   proceed without reviewer and branch protection. Put the secrets below in
   that environment, not in the repository or the workflow file.
2. In Apple Developer, register the exact app ID `io.github.ivadim.pdftools`,
   create an Apple Distribution certificate with its private key, and create an
   App Store Connect provisioning profile for that ID and certificate. Create
   the corresponding App Store Connect app record before uploading.
3. Ensure the existing repository `APPLE_TEAM_ID` secret identifies the same
   team, and create an App Store Connect team API key with access to upload
   builds. Keep the private key and distribution certificate secure.
4. Select **Actions → iOS App Store delivery → Run workflow**, choose `main` at
   the tagged commit, and enter the previously CI-tested tag. Approve the
   protected environment only after reviewing the run. Its build number is
   the workflow run number followed by the run attempt (for example `12.1`);
   check App Store Connect for conflicts if builds have already been uploaded.

| Environment secret | Contents |
| --- | --- |
| `IOS_DISTRIBUTION_CERTIFICATE` | Base64-encoded `.p12` containing the Apple Distribution certificate and private key. |
| `IOS_DISTRIBUTION_CERTIFICATE_PASSWORD` | Password used to export that `.p12`. |
| `IOS_APP_STORE_PROFILE` | Base64-encoded App Store Connect `.mobileprovision` profile for this bundle ID. |
| `APP_STORE_CONNECT_API_KEY` | Base64-encoded contents of the downloaded App Store Connect API key `.p8` file. |
| `APP_STORE_CONNECT_API_KEY_ID` | API key ID. |
| `APP_STORE_CONNECT_ISSUER_ID` | App Store Connect API issuer ID. |

For example, copy a certificate or profile's encoded contents using
`base64 -i path/to/file | pbcopy`; paste the value directly into the matching
GitHub environment secret. The signing job checks out the workflow's exact
main commit (verified against the CI-tested tag) and
uses Xcode to build the signed archive after importing the certificate. Unlike
the macOS signing job, this executes project build steps on the signing runner;
review project build phases and dependencies before approving it. The
credentials and profiles are deleted at the end of the job. The workflow has
not yet been exercised with real distribution credentials.
