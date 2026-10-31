#!/bin/sh
# Runs the requested Bazel iOS tests with simulator, Xcode, and host versions
# included in the test environment so changes invalidate cached test results.
set -eu
cd "$(dirname "$0")/.."

# Pick the newest installed iOS runtime supporting both CI device types.
runtime=$(xcrun simctl list runtimes -j | python3 -c '
import json, sys
runtimes = json.load(sys.stdin)["runtimes"]
compatible = [r for r in runtimes if r.get("isAvailable") and
    r["identifier"].startswith("com.apple.CoreSimulator.SimRuntime.iOS-") and
    {"iPhone 17", "iPad (A16)"}.issubset({d["name"] for d in r.get("supportedDeviceTypes", [])})]
if not compatible:
    sys.exit("No available iOS runtime supports both iPhone 17 and iPad (A16)")
r = max(compatible, key=lambda item: tuple(map(int, item["version"].split("."))))
print(r["version"] + ":" + r["buildversion"])
')
version=${runtime%%:*}
xcode=$(xcodebuild -version | tr '\n' '_')
host=$(sw_vers -buildVersion)

bazel test --config=ci \
    "--@rules_apple//apple/build_settings:ios_simulator_version=$version" \
    "--test_env=PDFTOOLS_IOS_RUNTIME=$runtime" \
    "--test_env=PDFTOOLS_IOS_XCODE=$xcode" \
    "--test_env=PDFTOOLS_IOS_HOST=$host" \
    --test_timeout=3600 --local_test_jobs=1 "$@"
