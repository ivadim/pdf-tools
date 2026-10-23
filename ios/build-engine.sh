#!/bin/sh
# Builds the PDF engine (crates/pdftools-ffi) for iOS devices and the
# simulator and packs it into ios/Engine/PdftoolsFFI.xcframework, which the
# Xcode project links. Run it after changing the Rust code. Needs Xcode.
#
#   ios/build-engine.sh            # release build (what the app ships)
#   ios/build-engine.sh --debug    # faster to build
set -eu

cd "$(dirname "$0")/.."
config=--config=release
[ "${1:-}" = "--debug" ] && config=--compilation_mode=fastbuild

out=ios/Engine
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

for platform in ios_arm64 ios_sim_arm64; do
    flags="$config --platforms=@apple_support//platforms:$platform"
    bazel build $flags //crates/pdftools-ffi:pdftools_ffi
    lib=$(bazel cquery $flags --output=files //crates/pdftools-ffi:pdftools_ffi 2>/dev/null)
    mkdir -p "$work/$platform"
    cp "$lib" "$work/$platform/libpdftools_ffi.a"
done

rm -rf "$out/PdftoolsFFI.xcframework"
mkdir -p "$out"
xcodebuild -create-xcframework \
    -library "$work/ios_arm64/libpdftools_ffi.a" -headers crates/pdftools-ffi/include \
    -library "$work/ios_sim_arm64/libpdftools_ffi.a" -headers crates/pdftools-ffi/include \
    -output "$out/PdftoolsFFI.xcframework"
echo "Built $out/PdftoolsFFI.xcframework"
