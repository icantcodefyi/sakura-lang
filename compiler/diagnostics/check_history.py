"""Reject diagnostic code deletion, reassignment, and resurrection (RFC 0001)."""

import argparse
from pathlib import Path
import re
import subprocess

REGISTRY = "compiler/diagnostics/codes.txt"
SOURCE = "compiler/crates/sakura_diagnostics/src/codes.rs"
STATUSES = {"active", "reserved", "retired"}


def parse(text):
    entries = {}
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        fields = line.split()
        if len(fields) != 3:
            raise ValueError(f"line {number}: expected CODE STATUS TITLE")
        code, status, title = fields
        if not re.fullmatch(r"[VWD][0-9]{3,4}", code):
            raise ValueError(f"line {number}: invalid code {code}")
        if status not in STATUSES or not re.fullmatch(r"[a-z][a-z0-9_]*", title):
            raise ValueError(f"line {number}: invalid status or title")
        if code in entries:
            raise ValueError(f"line {number}: duplicate {code}")
        entries[code] = (status, title)
    if not entries:
        raise ValueError("empty diagnostic registry")
    return entries


def compare(previous, current):
    errors = []
    for code, (status, title) in sorted(previous.items()):
        if code not in current:
            errors.append(f"{code}: removed; retain it as retired")
            continue
        new_status, new_title = current[code]
        if title != new_title:
            errors.append(f"{code}: reassigned from {title} to {new_title}")
        allowed = {status, "retired"}
        if status == "reserved":
            allowed.add("active")
        if new_status not in allowed:
            errors.append(f"{code}: forbidden status change {status} -> {new_status}")
    return errors


def initial_registry(source):
    # Before codes.txt existed, codes.rs retained retired entries in its explanations.
    entries = {}
    pattern = r'^    ([VWD][0-9]{3,4}) "([a-z_]+)" [A-Z]+,\n"([^\n]*)'
    for code, title, explanation in re.findall(pattern, source, re.MULTILINE):
        status = "retired" if explanation.startswith("Retired (") else "active"
        if code == "W8001":
            status = "reserved"
        entries[code] = (status, title)
    declared = re.findall(r'^[ \t]*([VWD][0-9]{3,4})[ \t]+"', source, re.MULTILINE)
    if not entries or len(declared) != len(entries) or set(declared) != entries.keys():
        raise ValueError("base revision diagnostic code table was not fully parsed")
    return entries


def git(*args):
    return subprocess.check_output(["git", *args], text=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True, help="full SHA of the comparison commit")
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.base):
        parser.error("--base must be a full commit SHA")
    root = Path(git("rev-parse", "--show-toplevel").strip())
    if git("ls-tree", "--full-tree", "--name-only", args.base, "--", REGISTRY).strip():
        previous = parse(git("show", f"{args.base}:{REGISTRY}"))
    else:
        previous = initial_registry(git("show", f"{args.base}:{SOURCE}"))
    errors = compare(previous, parse((root / REGISTRY).read_text()))
    if errors:
        parser.exit(1, "\n".join(errors) + "\n")
    print(f"Diagnostic history preserves {len(previous)} assignments from {args.base}.")


if __name__ == "__main__":
    main()
