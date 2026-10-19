"""Check authored guide fences with the current CLI; use SAKURA_BIN for worktrees.

Expected diagnostics, holes and shared context are explicit fence metadata.
API declarations and pseudocode must be labelled `fragment`; generated std/error
references and skill fixtures are covered by agent_docs and std tests instead.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
TARGET = os.environ.get("SAKURA_DOC_TARGET", "js")
BINARY = Path(os.environ.get("SAKURA_BIN", ROOT / "compiler/target/debug/sakura"))
FENCE = re.compile(r"```sakura([^\n]*)\n(.*?)\n```", re.S)


def snippet(page, needle):
    return next(code for _, code in FENCE.findall((ROOT / page).read_text()) if needle in code)


def main():
    contexts = {
        "counter": snippet("website/content/docs/tour/handlers.mdx", "fn memory_counter"),
        "fetch-both": snippet("website/content/docs/tour/concurrency.mdx", "fn fetch_both"),
        "normalize": "fn normalize(xs: List<Int>) -> List<Int> { List.unique(List.sort(xs)) }",
    }
    pages = [ROOT / "README.md", ROOT / "docs/llms.txt", ROOT / "docs/mcp.md", ROOT / "docs/release.md"]
    pages += sorted((ROOT / "website/content/docs").rglob("*.mdx"))
    checked = executed = fragments = 0
    failures = []
    with tempfile.TemporaryDirectory(prefix="sakura-doc-sweep-") as tmp:
        for page in pages:
            if page.stem == "stdlib" or "errors" in page.parts:
                continue
            for index, match in enumerate(FENCE.finditer(page.read_text())):
                info, code = match.groups()
                label = f"{page.relative_to(ROOT)}:{page.read_text()[:match.start()].count(chr(10)) + 1}"
                if "fragment" in info.split():
                    fragments += 1
                    print(f"FRAGMENT {label}")
                    continue
                project = Path(tmp) / str(checked)
                (project / "src").mkdir(parents=True)
                (project / "sakura.toml").write_text('[package]\nname = "app"\n')
                recipe = re.search(r"project=([\w-]+)", info)
                if recipe:
                    source = ROOT / "compiler/examples" / recipe[1]
                    shutil.copyfile(source / "sakura.toml", project / "sakura.toml")
                    for folder in ("data", "tests/fixtures"):
                        if (source / folder).exists():
                            shutil.copytree(source / folder, project / folder)
                context = re.search(r"context=([\w-]+)", info)
                if context:
                    name = context[1]
                    if name == "shapes":
                        shape = snippet("website/content/docs/tour/types.mdx", "// src/shapes.sak")
                        (project / "src/shapes.sak").write_text(shape)
                    else:
                        code = contexts[name] + "\n" + code
                (project / "src/main.sak").write_text(code + "\n")
                if page.stem == "cassettes":
                    shutil.copytree(ROOT / "compiler/examples/secrets/tests/fixtures", project / "tests/fixtures")
                command = [str(BINARY), "check", "--json"]
                result = subprocess.run(command, cwd=project, capture_output=True, text=True, timeout=60)
                records = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
                errors = [r for r in records if r.get("severity") == "error"]
                holes = [r for r in records if r.get("kind") == "hole"]
                expected = re.search(r"expect=(\w+)", info)
                ok = (result.returncode == 1 and any(r.get("code") == expected[1] for r in errors)) if expected else (result.returncode == 0 and not errors and bool(holes) == ("holes" in info.split()))
                checked += 1
                if not ok:
                    failures.append(label)
                    print(f"FAIL {label}: {result.stdout}{result.stderr}")
                    continue
                # Special recording/contract regression flows have the dedicated sample gate.
                special = "Llm.recording(" in code or code.startswith("pub fn increment(")
                if not expected and not holes and not special and re.search(r"^(test|property) ", code, re.M):
                    # Snapshot examples need an explicit first recording, then a normal replay.
                    args = ["test", "--json", "--target", TARGET]
                    if "expect_snapshot(" in code:
                        recorded = subprocess.run([str(BINARY), *args, "--update-snapshots"], cwd=project, capture_output=True, text=True, timeout=60)
                        if recorded.returncode:
                            failures.append(label)
                            print(f"FAIL snapshot setup {label}: {recorded.stdout}{recorded.stderr}")
                    result = subprocess.run([str(BINARY), *args], cwd=project, capture_output=True, text=True, timeout=60)
                    executed += 1
                    if result.returncode:
                        failures.append(label)
                        print(f"FAIL test {label}: {result.stdout}{result.stderr}")
                print(f"PASS {label}")
    print(f"{checked} checked; {executed} test invocations; {fragments} explicitly labelled fragments; {len(failures)} failures")
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
