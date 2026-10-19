"""Classify CI inputs. Unknown paths and unavailable history run the full suite."""

import os
from pathlib import Path
import re
import subprocess

AREAS = ("compiler", "website", "docs", "workflows")


def classify(paths):
    changed = set()
    for path in paths:
        if path.startswith((".github/", "tools/", "packaging/")):
            changed.add("workflows")
        elif path.startswith(("compiler/", "runtime/", "std/", "bench/programs/", "editors/")):
            changed.add("compiler")
        elif path.startswith("website/"):
            changed.add("website")
        elif path.startswith(("docs/", "research/")) or path.endswith(".md"):
            changed.add("docs")
        else:
            return set(AREAS)
    return changed


def comparison_base(event, base):
    if event == "workflow_dispatch" and base:
        return base
    if event in ("push", "schedule", "workflow_dispatch"):
        try:
            base = subprocess.check_output([
                "gh", "api", "--method", "GET",
                f"repos/{os.environ['GITHUB_REPOSITORY']}/actions/workflows/ci.yml/runs",
                "-f", "branch=main", "-f", "status=success", "-f", "per_page=100",
                "--jq", '.workflow_runs | map(select(.event == "push" or .event == "schedule")) | .[0].head_sha // empty',
            ], text=True).strip()
        except (subprocess.CalledProcessError, OSError):
            print("Unavailable successful CI baseline; running all areas.")
            return ""
    return base


def detect(event, base, head, force_all=False):
    if event == "schedule" or force_all:
        return set(AREAS)
    if not all(re.fullmatch(r"[0-9a-fA-F]{40}", sha) and int(sha, 16) for sha in (base, head)):
        print("Unavailable comparison commits; running all areas.")
        return set(AREAS)
    try:
        if event == "pull_request":
            base = subprocess.check_output(["git", "merge-base", base, head], text=True).strip()
        paths = subprocess.check_output(["git", "diff", "--name-only", "-z", "--no-renames", base, head])
    except subprocess.CalledProcessError:
        print("Unavailable comparison history; running all areas.")
        return set(AREAS)
    return classify(paths.decode("utf-8", errors="surrogateescape").split("\0")[:-1])


if __name__ == "__main__":
    event = os.environ["EVENT_NAME"]
    requested_base = os.environ["BASE_SHA"]
    base = comparison_base(event, requested_base)
    areas = detect(event, base, os.environ["HEAD_SHA"],
                   force_all=event == "workflow_dispatch" and not requested_base)
    outputs = f"base={base}\n" + "".join(f"{area}={str(area in areas).lower()}\n" for area in AREAS)
    print(outputs, end="")
    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
        output.write(outputs)
