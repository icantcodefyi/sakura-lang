"""Validate research/12 local links and JSON evidence; no network or build required."""
import json
import re
from pathlib import Path

root = Path(__file__).resolve().parents[2]
files = [root / "research/12-m5-native.md", root / "research/README.md"]
files.extend(sorted((root / "research/12-evidence").glob("*.md")))
count = 0
for path in files:
    for target in re.findall(r"\]\(([^)]+)\)", path.read_text()):
        if target.startswith("https://"):
            continue
        target = target.split("#")[0]
        if target:
            assert (path.parent / target).exists(), (path, target)
        count += 1
json_files = sorted((root / "research/12-evidence").glob("*.json"))
for path in json_files:
    json.loads(path.read_text())
print(f"PASS: {count} local Markdown targets; {len(json_files)} JSON evidence files")
print("External URL results: link-check.json (2026-10-18); fragments not validated")
