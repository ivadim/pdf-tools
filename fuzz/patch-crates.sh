#!/usr/bin/env bash
# Puts patched copies of third-party crates in fuzz/vendor, for the
# [patch.crates-io] section of fuzz/Cargo.toml. Bazel applies the same patches
# with crate.annotation in MODULE.bazel; this does it for the Cargo build.
set -euo pipefail

cd "$(dirname "$0")"
patches=../third_party/patches

# name version sha256 (as in Cargo.lock) patch
crates=(
  "hayro-interpret 0.8.0 ac9df074ff163a63864cec9b0bf640ccbff8abf4e44481df7369ce0ac172ffbe hayro-interpret-rgb-length.patch"
)

sha256() {
  if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1
}

download=$(mktemp -d)
trap 'rm -r "$download"' EXIT

for entry in "${crates[@]}"; do
  read -r name version checksum patch <<<"$entry"
  file="$download/$name-$version.crate"
  curl --fail --silent --show-error --location --output "$file" \
    "https://static.crates.io/crates/$name/$name-$version.crate"
  if [[ $(sha256 "$file") != "$checksum" ]]; then
    echo "$name $version: checksum doesn't match Cargo.lock" >&2
    exit 1
  fi
  rm -rf "vendor/$name"
  mkdir -p "vendor/$name"
  tar -xzf "$file" -C "vendor/$name" --strip-components=1
  patch --quiet -p1 -d "vendor/$name" <"$patches/$patch"
  echo "vendor/$name: $name $version with $patch"
done
