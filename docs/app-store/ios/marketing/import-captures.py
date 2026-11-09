#!/usr/bin/env python3
"""Import xcresulttool attachments from one marketing capture run."""
import json
import re
import shutil
import sys
from pathlib import Path

if len(sys.argv) != 3 or sys.argv[1] not in ("duo", "ipad"):
    sys.exit("Usage: import-captures.py {duo|ipad} EXPORTED_ATTACHMENTS_DIR")
device, source = sys.argv[1], Path(sys.argv[2])
root = Path(__file__).resolve().parent / "captures" / device
expected = {(mode, feature) for mode in ("light", "dark") for feature in ("home", "merge", "compress", "redact", "images")}
found = {}
for test in json.loads((source / "manifest.json").read_text()):
    for attachment in test["attachments"]:
        match = re.match(r"marketing-(light|dark)-(home|merge|compress|redact|images)_", attachment["suggestedHumanReadableName"])
        if match:
            key = match.group(1), match.group(2)
            if key in found:
                sys.exit(f"Duplicate capture: {key}")
            found[key] = source / attachment["exportedFileName"]
if set(found) != expected:
    sys.exit(f"Missing captures: {sorted(expected - set(found))}")
for (mode, feature), image in found.items():
    destination = root / mode / f"{feature}.png"
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(image, destination)
    print(destination)
