"""Keep the JSON contract append-only (design §17.232 (b), RFC 0001).

Against the baseline at the merge base: a removed line, a changed type, optional -> required,
required -> optional (design §17.242 (4)) or a MAJOR bump fails; a line added to an existing kind
needs a MINOR bump of that kind in the same diff (one MINOR step), the new MINOR keeps every line
of the kind's previous version as it was, and a new kind starts at 1.0.

The file's first non-comment line may be `baseline MAJOR.MINOR.PATCH` (design §17.257 (1)): the
history is append-only from the release it names. The one transition passed without comparing
is "no line -> `baseline 0.2.0`", the reset; once the line is on both sides the rules above
apply, and a changed or removed line, or a reset naming another release, fails.
"""

import argparse
from pathlib import Path
import re
import subprocess
from typing import NamedTuple

BASELINE = "compiler/contracts/json.txt"
TYPES = {"string", "integer", "number", "boolean", "null", "object", "array"}
RESET = "0.2.0"


class Contract(NamedTuple):
    """The `baseline` release (`None` without the line) and `{(kind, version, field): (types,
    required)}`."""

    baseline: str | None
    entries: dict


def parse(text):
    """The file's `Contract`."""
    baseline = None
    entries = {}
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        fields = line.split()
        if fields[0] == "baseline":
            if baseline is not None:
                raise ValueError(f"line {number}: a second baseline line")
            if entries:
                raise ValueError(f"line {number}: the baseline line goes before every field line")
            if len(fields) != 2 or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", fields[1]):
                raise ValueError(f"line {number}: expected baseline MAJOR.MINOR.PATCH")
            baseline = fields[1]
            continue
        if len(fields) != 5:
            raise ValueError(f"line {number}: expected KIND SCHEMA_VERSION FIELD TYPE REQUIRED")
        kind, version, field, types, req = fields
        if not re.fullmatch(r"[a-z_]+", kind) or not re.fullmatch(r"[0-9]+\.[0-9]+", version):
            raise ValueError(f"line {number}: invalid kind or schema_version")
        if not set(types.split("|")) <= TYPES or req not in {"required", "optional"}:
            raise ValueError(f"line {number}: invalid type or required|optional")
        if (kind, version, field) in entries:
            raise ValueError(f"line {number}: duplicate {kind} {version} {field}")
        entries[(kind, version, field)] = (frozenset(types.split("|")), req == "required")
    return Contract(baseline, entries)


def is_reset(previous, current):
    """Whether `previous` -> `current` is the one reset (§17.257 (1) (a))."""
    return previous.baseline is None and current.baseline == RESET


def compare(previous, current):
    """The errors of `previous` -> `current` (two `Contract`s); none for the reset."""
    before, after = previous.baseline, current.baseline
    if is_reset(previous, current):
        return []
    if before is None and after is not None:
        return [f"baseline {after}: a reset names {RESET}"]
    if before is not None and after is None:
        return [f"baseline {before}: baseline line removed"]
    if before != after:
        return [f"baseline changed from {before} to {after}: the one reset is spent"]
    return compare_entries(previous.entries, current.entries)


def versions(entries):
    out = {}
    for kind, version, _ in entries:
        out.setdefault(kind, set()).add(tuple(map(int, version.split("."))))
    return out


def compare_entries(previous, current):
    """The additive rules over two `entries` maps."""
    errors = []
    for key, (types, required) in sorted(previous.items()):
        name = " ".join(key)
        if key not in current:
            errors.append(f"{name}: removed")
            continue
        new_types, new_required = current[key]
        if new_types != types:
            errors.append(f"{name}: type changed from {'|'.join(sorted(types))}")
        if new_required and not required:
            errors.append(f"{name}: optional -> required")
        if required and not new_required:
            errors.append(f"{name}: required -> optional")
    old, new = versions(previous), versions(current)
    for kind in sorted(new.keys() - old.keys()):
        if new[kind] != {(1, 0)}:
            errors.append(f"{kind}: a new kind starts at 1.0 alone")
    for kind in sorted(new.keys() & old.keys()):
        latest = max(old[kind])
        for version in sorted(new[kind] - old[kind]):
            label = f"{kind} {version[0]}.{version[1]}"
            if version[0] != latest[0]:
                errors.append(f"{label}: a MAJOR bump is refused within a compiler major")
                continue
            if version != (latest[0], latest[1] + 1):
                errors.append(f"{label}: the next MINOR after {latest[0]}.{latest[1]} is {latest[1] + 1}")
                continue
            base = f"{latest[0]}.{latest[1]}"
            for (k, v, field), entry in sorted(previous.items()):
                if k != kind or v != base:
                    continue
                now = current.get((k, f"{version[0]}.{version[1]}", field))
                if now is None or now != entry:
                    errors.append(f"{label} {field}: not kept from {base} as it was")
    for key in sorted(current.keys() - previous.keys()):
        kind, version, field = key
        if kind in old and tuple(map(int, version.split("."))) in old[kind]:
            errors.append(f"{' '.join(key)}: added without a MINOR bump of {kind}")
    return errors


def git(*args):
    return subprocess.check_output(["git", *args], text=True, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True, help="full SHA of the comparison commit")
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.base):
        parser.error("--base must be a full commit SHA")
    root = Path(git("rev-parse", "--show-toplevel").strip())
    current = parse((root / BASELINE).read_text(encoding="utf-8"))
    if not git("ls-tree", "--full-tree", "--name-only", args.base, "--", BASELINE).strip():
        print(f"No JSON contract at {args.base}; {len(current.entries)} lines start the history.")
        return
    previous = parse(git("show", f"{args.base}:{BASELINE}"))
    errors = compare(previous, current)
    if errors:
        parser.exit(1, "\n".join(errors) + "\n")
    if is_reset(previous, current):
        print(f"JSON contract baseline reset to {RESET}: "
              f"{len(current.entries)} lines replace {len(previous.entries)}.")
        return
    print(f"JSON contract keeps {len(previous.entries)} lines from {args.base}.")


if __name__ == "__main__":
    main()
