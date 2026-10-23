#!/bin/sh
# Runs the iOS app's unit tests (PDFToolsTests) and UI tests
# (PDFToolsUITests) on an iPhone and an iPad simulator. Needs Xcode; builds
# the engine first if ios/Engine is missing.
#
#   ios/test.sh                                  # newest iPhone and iPad simulators
#   IOS_TEST_DEVICES="iPhone 17 Pro" ios/test.sh # by name or UDID, one per line
#
# Results go to ios/build/TestResults/<device>.xcresult (open them in Xcode).
set -eu

cd "$(dirname "$0")"
[ -d Engine/PdftoolsFFI.xcframework ] || ./build-engine.sh --debug

# The newest available iPhone and iPad, unless IOS_TEST_DEVICES names them.
devices=${IOS_TEST_DEVICES:-$(xcrun simctl list devices available -j | python3 -c '
import json, sys
runtimes = json.load(sys.stdin)["devices"]
ios = sorted((r for r in runtimes if "iOS" in r), key=lambda r: [int(n) for n in r.rsplit("iOS-", 1)[1].split("-")])
for kind in ("iPhone", "iPad"):
    for runtime in reversed(ios):
        found = [d for d in runtimes[runtime] if d["name"].startswith(kind)]
        if found:
            print(found[-1]["udid"])
            break
')}
[ -n "$devices" ] || { echo "no iOS simulators found" >&2; exit 1; }

status=0
echo "$devices" | while IFS= read -r device; do
    case $device in
        *-*-*-*-*) destination="id=$device" ;;
        *) destination="platform=iOS Simulator,name=$device" ;;
    esac
    name=$(xcrun simctl list devices | sed -n "s/^ *\(.*\) ($device) .*/\1/p" | head -n 1)
    echo "Testing on ${name:-$device}"
    # Only this run's results may be read below (an older passing bundle must
    # not stand in for a run that failed before writing one).
    rm -rf build/Logs/Test
    xcodebuild test -project PDFTools.xcodeproj -scheme PDFTools -destination "$destination" \
        -derivedDataPath build CODE_SIGNING_ALLOWED=NO 2>&1 | grep -v invalidDigitCount || true
    results=$(ls -d build/Logs/Test/*.xcresult 2>/dev/null | head -n 1)
    [ -n "$results" ] || { echo "no test results for ${name:-$device}: the build or launch failed" >&2; exit 1; }
    mkdir -p build/TestResults
    kept="build/TestResults/$(echo "${name:-$device}" | tr -c 'A-Za-z0-9\n' '-').xcresult"
    rm -rf "$kept"
    mv "$results" "$kept"
    # The pipe hides xcodebuild's status; the result bundle has it.
    xcrun xcresulttool get test-results summary --path "$kept" | grep -q '"result" : "Passed"' || exit 1
done || status=1
exit $status
