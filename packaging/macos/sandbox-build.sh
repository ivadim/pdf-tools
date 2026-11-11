#!/bin/bash
# Build an ad-hoc signed sandboxed app for local testing, not distribution.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
if [[ $# != 1 || -e "$1" ]]; then
  echo "Usage: $0 /path/to/new/PDF\ Tools.app" >&2
  exit 1
fi
app="$1"
[[ "$app" == /* && "$app" == *.app ]] || { echo 'Use an absolute .app path.' >&2; exit 1; }
[[ $(uname -s) == Darwin ]] || { echo 'This script requires macOS.' >&2; exit 1; }
cd "$root"
bazel build --config=release --macos_minimum_os=12.0 --define=macos_app_store=true //crates/pdftools-app:pdftools
version=$(sed -n 's/^MACOS_APP_STORE_VERSION = "\([0-9.]*\)"$/\1/p' version.bzl)
[[ -n "$version" ]] || { echo 'Invalid Mac App Store version in version.bzl.' >&2; exit 1; }
tmp=$(mktemp -d "${TMPDIR:-/tmp}/pdftools-sandbox.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
bundle="$tmp/PDF Tools.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources" "$tmp/AppIcon.iconset"
cp bazel-bin/crates/pdftools-app/pdftools "$bundle/Contents/MacOS/pdftools"
chmod u+w "$bundle/Contents/MacOS/pdftools"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" packaging/icon.png --out "$tmp/AppIcon.iconset/icon_${size}x${size}.png" > /dev/null
  sips -z "$((size * 2))" "$((size * 2))" packaging/icon.png --out "$tmp/AppIcon.iconset/icon_${size}x${size}@2x.png" > /dev/null
done
iconutil -c icns "$tmp/AppIcon.iconset" -o "$bundle/Contents/Resources/AppIcon.icns"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$bundle/Contents/Info.plist"
# Apple requires macOS 12 or later for an Apple-silicon-only App Store app.
lipo -verify_arch arm64 "$bundle/Contents/MacOS/pdftools"
vtool -show-build "$bundle/Contents/MacOS/pdftools" | awk '$1 == "minos" && $2 == "12.0" { found = 1 } END { exit !found }'
codesign --force --sign - --entitlements packaging/macos/AppStore.entitlements "$bundle"
codesign --verify --deep --strict --verbose=2 "$bundle"
mkdir -p "$(dirname "$app")"
ditto "$bundle" "$app"
echo "Sandboxed test app: $app"
