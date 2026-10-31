#!/bin/sh
# Builds the Android app, starts the emulator if no device is connected,
# installs the app and opens it. Bazel downloads the emulator and its system
# image (see MODULE.bazel), so nothing needs installing.
#
#   android/run.sh              # debug build
#   android/run.sh --release    # what the app ships (smaller, faster engine)
#   android/run.sh --install    # build and install, but don't open it (android/build/app.apk)
#   android/run.sh --emulator   # just start the emulator
#
# The emulator runs the host's architecture: arm64 on Apple silicon, x86_64
# elsewhere (Linux needs KVM). With CI set it runs without a window and
# cold-boots every time.
set -eu

cd "$(dirname "$0")/.."
config=
mode=open
for arg in "$@"; do
    case "$arg" in
    --release) config=--config=release ;;
    --install) mode=install ;;
    --emulator) mode=emulator ;;
    *) echo "unknown option $arg" >&2 && exit 2 ;;
    esac
done

case "$(uname -m)" in
arm64 | aarch64)
    arch=arm64 abi=arm64-v8a
    # Devices and Apple silicon emulators.
    platforms=@rules_android//:arm64-v8a
    ;;
*)
    arch=x86_64 abi=x86_64
    # Devices, and this host's emulator; in CI only the emulator's, which
    # halves the engine's build.
    platforms=@rules_android//:arm64-v8a,@rules_android//:x86_64
    [ -n "${CI:-}" ] && platforms=@rules_android//:x86_64
    ;;
esac

# One set of flags for every Bazel command, so none of them discards
# another's analysis or moves bazel-bin.
flags="--config=android --android_platforms=$platforms $config"
[ -n "${CI:-}" ] && flags="$flags --config=ci"
# The app (first, so its build doesn't compete with a booting emulator) and
# the emulator with its system image.
image=@android_system_image_36_$arch//:files
app=//android:app
[ $mode = emulator ] && app=
# shellcheck disable=SC2086
bazel build $flags $app @androidsdk//:emulator_runtime @androidsdk//:adb "$image"
base=$(bazel info $flags output_base)
# shellcheck disable=SC2086
files() { bazel cquery $flags --output=files "$1" 2>/dev/null | head -1; }
path() { echo "$base/$(files "$1")"; }
# The repository (external/<name>) holding a target's files.
repository() { echo "$base/$(files "$1" | cut -d/ -f1-2)"; }

# The emulator's SDK: links into Bazel's downloads, in the layout it expects.
cache="${XDG_CACHE_HOME:-$HOME/.cache}/pdftools-android"
sdk="$cache/sdk"
mkdir -p "$sdk/platform-tools" "$sdk/system-images/android-36/default"
ln -sfn "$(dirname "$(path @androidsdk//:emulator)")" "$sdk/emulator"
ln -sfn "$(path @androidsdk//:adb)" "$sdk/platform-tools/adb"
ln -sfn "$(repository "$image")" "$sdk/system-images/android-36/default/$abi"
adb="$sdk/platform-tools/adb"
export ANDROID_HOME="$sdk" ANDROID_SDK_ROOT="$sdk"
export ANDROID_AVD_HOME="$cache/avd"

if ! "$adb" get-state >/dev/null 2>&1; then
    avd=pdftools-$arch
    mkdir -p "$ANDROID_AVD_HOME/$avd.avd"
    printf 'avd.ini.encoding=UTF-8\npath=%s\ntarget=android-36\n' "$ANDROID_AVD_HOME/$avd.avd" >"$ANDROID_AVD_HOME/$avd.ini"
    # A 1080×2400 phone (android/test.sh's swipes assume it).
    cat >"$ANDROID_AVD_HOME/$avd.avd/config.ini" <<EOF
AvdId=$avd
avd.ini.displayname=PDF Tools
abi.type=$abi
hw.cpu.arch=$arch
hw.cpu.ncore=4
hw.ramSize=4096
hw.lcd.width=1080
hw.lcd.height=2400
hw.lcd.density=420
hw.keyboard=yes
hw.gpu.enabled=yes
hw.gpu.mode=auto
disk.dataPartition.size=6G
image.sysdir.1=system-images/android-36/default/$abi/
tag.id=default
EOF
    if [ -n "${CI:-}" ]; then
        # No display: software rendering, no audio, and a cold boot each time.
        options="-no-window -gpu swiftshader_indirect -no-audio -no-snapshot"
    else
        options="-no-snapshot-save"
    fi
    log="$ANDROID_AVD_HOME/emulator.log"
    echo "Starting the emulator…"
    # shellcheck disable=SC2086
    "$sdk/emulator/emulator" -avd "$avd" -no-boot-anim $options >"$log" 2>&1 &
    # Booting takes a minute or two (longer with software rendering).
    waited=0
    until [ "$("$adb" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] &&
        "$adb" shell pm path android >/dev/null 2>&1; do
        if [ $waited -ge 600 ] || ! kill -0 $! 2>/dev/null; then
            echo "The emulator didn't start:" >&2
            tail -40 "$log" >&2
            exit 1
        fi
        sleep 2
        waited=$((waited + 2))
    done
fi
[ $mode = emulator ] && exit 0

# Where Bazel put it (not bazel-bin: --android_platforms has its own output
# folder), copied to a fixed place for android/test.sh and CI.
apk=android/build/app.apk
mkdir -p android/build
cp -f "$(bazel cquery $flags --output=files //android:app 2>/dev/null | grep '/app\.apk$')" $apk
chmod u+w $apk
"$adb" install -r -t $apk
[ $mode = install ] && exit 0
"$adb" shell am start -n io.github.ivadim.pdftools/.MainActivity
