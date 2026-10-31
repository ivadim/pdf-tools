#!/bin/sh
# Bazel owns the APK, test inputs, emulator and system image. This script owns
# one fresh CI emulator and leaves diagnostics in TEST_UNDECLARED_OUTPUTS_DIR.
set -eu

runfile() {
    path="$TEST_SRCDIR/$1"
    [ -f "$path" ] || { echo "Missing runfile: $1" >&2; exit 1; }
    printf '%s\n' "$path"
}
apk=$(runfile "$1")
adb=$(runfile "$2")
emulator=$(runfile "$3")
image_properties=$(runfile "$4")
test_script=$(runfile "$5")
fixture_file=$(runfile "$6")

sdk="$TEST_TMPDIR/sdk"
avd_home="$TEST_TMPDIR/avd"
mkdir -p "$sdk/platform-tools" "$sdk/system-images/android-36/default" "$avd_home/pdftools.avd" "${TEST_UNDECLARED_OUTPUTS_DIR:?}"
ln -s "$(dirname "$emulator")" "$sdk/emulator"
ln -s "$adb" "$sdk/platform-tools/adb"
ln -s "$(dirname "$image_properties")" "$sdk/system-images/android-36/default/x86_64"
export ANDROID_HOME="$sdk" ANDROID_SDK_ROOT="$sdk" ANDROID_AVD_HOME="$avd_home"
export ANDROID_USER_HOME="$TEST_TMPDIR/android-user" ANDROID_ADB_SERVER_PORT=5038 ANDROID_SERIAL=emulator-5554
PDFTOOLS_TEST_ADB=$adb
PDFTOOLS_TEST_FIXTURES=$(dirname "$(dirname "$fixture_file")")
export PDFTOOLS_TEST_ADB PDFTOOLS_TEST_FIXTURES

printf 'avd.ini.encoding=UTF-8\npath=%s\ntarget=android-36\n' "$avd_home/pdftools.avd" >"$avd_home/pdftools.ini"
cat >"$avd_home/pdftools.avd/config.ini" <<EOF
AvdId=pdftools
avd.ini.displayname=PDF Tools
abi.type=x86_64
hw.cpu.arch=x86_64
hw.cpu.ncore=4
hw.ramSize=4096
hw.lcd.width=1080
hw.lcd.height=2400
hw.lcd.density=420
hw.keyboard=yes
hw.gpu.enabled=yes
hw.gpu.mode=auto
disk.dataPartition.size=6G
image.sysdir.1=system-images/android-36/default/x86_64/
tag.id=default
EOF

log="$TEST_UNDECLARED_OUTPUTS_DIR/emulator.log"
"$emulator" -avd pdftools -port 5554 -accel on -no-window -gpu swiftshader_indirect -no-audio -no-snapshot -no-boot-anim >"$log" 2>&1 &
emulator_pid=$!
trap 'kill "$emulator_pid" 2>/dev/null || true; "$adb" kill-server >/dev/null 2>&1 || true' EXIT
waited=0
until [ "$("$adb" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] &&
    "$adb" shell pm path android >/dev/null 2>&1; do
    if [ "$waited" -ge 600 ] || ! kill -0 "$emulator_pid" 2>/dev/null; then
        echo "The emulator did not start:" >&2
        tail -40 "$log"
        exit 1
    fi
    sleep 2
    waited=$((waited + 2))
done
"$adb" install -r -t "$apk"
sh "$test_script"
