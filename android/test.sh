#!/bin/sh
# Runs every tool in the Android app on the emulator (started by
# android/run.sh if no device is connected) and checks each result. The app's
# debug build reads fixture paths from its launch intent instead of showing
# the system's document picker, which runs outside the app.
#
#   android/test.sh              # every tool
#   android/test.sh merge split  # some of them
#
# Screenshots of failures go to android/build/test/<tool>.png.
set -eu

cd "$(dirname "$0")/.."
android/run.sh --install
adb="${XDG_CACHE_HOME:-$HOME/.cache}/pdftools-android/sdk/platform-tools/adb"
pkg=io.github.ivadim.pdftools
out=android/build/test
mkdir -p "$out"
# No animations: screens settle at once, so taps land where the dump said.
for scale in window_animation_scale transition_animation_scale animator_duration_scale; do
    "$adb" shell settings put global $scale 0
done

# Fixtures go into the app's own files, where it can read them.
dir=/data/data/$pkg/files/fixtures
"$adb" shell run-as $pkg mkdir -p $dir
put() { "$adb" shell run-as $pkg sh -c "'cat > $dir/$(basename "$1")'" <"$1"; }
for f in tests/fixtures/pdfjs/tracemonkey.pdf tests/fixtures/pdfjs/rotation.pdf tests/fixtures/encrypted/aes256-password-test.pdf; do
    put "$f"
done
# Two plain PNGs (landscape and portrait) for Images to PDF.
python3 -I - "$out" <<'EOF'
import struct, sys, zlib
def png(path, w, h, rgb):
    raw = b"".join(b"\0" + bytes(rgb) * w for _ in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
    open(path, "wb").write(data)
png(sys.argv[1] + "/wide.png", 400, 300, (20, 184, 166))
png(sys.argv[1] + "/tall.png", 300, 400, (249, 115, 22))
EOF
put "$out/wide.png"
put "$out/tall.png"

# The screen's elements, by test tag (resource id), text or content description.
dump() {
    "$adb" shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1 || true
    "$adb" shell cat /sdcard/ui.xml >"$out/ui.xml"
}
locate() {
    python3 -I - "$out/ui.xml" "$1" <<'EOF'
import re, sys
xml, query = open(sys.argv[1]).read(), sys.argv[2]
for node in re.findall(r"<node [^>]*>", xml):
    attrs = dict(re.findall(r'([\w-]+)="([^"]*)"', node))
    if query in (attrs.get("resource-id"), attrs.get("text"), attrs.get("content-desc")) or (
        query.endswith("*") and attrs.get("text", "").startswith(query[:-1])
    ):
        x1, y1, x2, y2 = map(int, re.findall(r"\d+", attrs["bounds"]))
        print((x1 + x2) // 2, (y1 + y2) // 2)
        break
EOF
}
# Waits up to 30 s for an element, then taps it.
tap() {
    for _ in $(seq 60); do
        dump
        point=$(locate "$1")
        if [ -n "$point" ]; then
            "$adb" shell input tap $point
            sleep 0.5
            return 0
        fi
        sleep 0.5
    done
    echo "  no “$1” on screen"
    return 1
}
type_text() { "$adb" shell input text "$(echo "$1" | sed 's/ /%s/g')"; }
hide_keyboard() {
    if "$adb" shell dumpsys input_method | grep -q "mInputShown=true"; then
        "$adb" shell input keyevent KEYCODE_BACK
        sleep 0.5
    fi
}
# Waits up to 60 s for the result's message and checks it starts with `$1`.
expect() {
    for _ in $(seq 120); do
        dump
        message=$(python3 -I - "$out/ui.xml" <<'EOF'
import re, sys
match = re.search(r'text="([^"]*)" resource-id="result.message"', open(sys.argv[1]).read())
print(match.group(1) if match else "")
EOF
)
        if [ -n "$message" ]; then
            case "$message" in
            "$1"*) echo "  $message" && return 0 ;;
            *) echo "  expected “$1…”, got “$message”" && return 1 ;;
            esac
        fi
        sleep 0.5
    done
    echo "  no result"
    return 1
}
# Opens a tool with fixtures (comma-separated) and chooses them.
open_tool() {
    "$adb" shell am force-stop $pkg
    "$adb" shell am start -n $pkg/.MainActivity --es tool "$1" --es files "$2" >/dev/null
    tap choose
}

merge() {
    open_tool merge "$dir/tracemonkey.pdf,$dir/rotation.pdf"
    tap run && expect "Merged 2 files into 16 pages."
}
split() {
    open_tool split "$dir/tracemonkey.pdf"
    tap split.ranges && type_text "1-3,4-8" && hide_keyboard
    tap run && expect "Created 2 files."
}
compress() {
    open_tool compress "$dir/tracemonkey.pdf"
    tap run && expect "1.0 MB →"
}
protect() {
    open_tool protect "$dir/rotation.pdf"
    tap password && type_text secret && tap confirm && type_text secret && hide_keyboard
    tap run && expect "Protected with a password."
}
unlock() {
    # A locked file opened in another tool offers Unlock.
    open_tool protect "$dir/aes256-password-test.pdf"
    tap "Unlock it" && tap password && type_text test && hide_keyboard
    tap run && expect "Removed the protection."
}
rotate() {
    open_tool rotate "$dir/rotation.pdf"
    tap "Rotate right" && tap run && expect "Rotated 1 page."
}
rearrange() {
    open_tool rearrange "$dir/tracemonkey.pdf"
    tap Reverse && tap Delete && tap run && expect "Saved 13 pages."
}
numbers() {
    open_tool numbers "$dir/tracemonkey.pdf"
    tap keep.2 && tap run && expect "Numbered 13 pages (1 page left out)."
}
watermark() {
    open_tool watermark "$dir/tracemonkey.pdf"
    tap run && expect "Watermarked 14 pages."
}
metadata() {
    open_tool metadata "$dir/tracemonkey.pdf"
    tap metadata.author && type_text "Ada Lovelace" && hide_keyboard
    tap run && expect "Saved the details."
}
signature() {
    open_tool signature "$dir/rotation.pdf"
    tap Type && tap signature.name && type_text "Ada Lovelace" && hide_keyboard
    "$adb" shell input swipe 540 1900 540 900 300
    tap page.2 && tap run && expect "Signed 1 page."
}
grayscale() {
    open_tool grayscale "$dir/tracemonkey.pdf"
    tap run && expect "Converted"
}
images() {
    open_tool images "$dir/wide.png,$dir/tall.png"
    tap Rotate && tap run && expect "Created a PDF with 2 pages."
}

tools=${*:-merge split compress protect unlock rotate rearrange numbers watermark metadata signature grayscale images}
failed=""
for tool in $tools; do
    echo "$tool"
    if ! $tool; then
        failed="$failed $tool"
        "$adb" exec-out screencap -p >"$out/$tool.png"
    fi
done
if [ -n "$failed" ]; then
    echo "Failed:$failed (screenshots in $out)"
    exit 1
fi
echo "All passed."
