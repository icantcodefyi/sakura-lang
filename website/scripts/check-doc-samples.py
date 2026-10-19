"""Check the runnable documentation against the source-built CLI (Python 3, no dependencies)."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("SAKURA_BIN", ROOT / "compiler/target/debug/sakura"))
PAGES = [
    "README.md",
    "website/content/docs/agents/index.mdx",
    "website/content/docs/agents/audit.mdx",
    "website/content/docs/agents/doc.mdx",
    "website/content/docs/agents/build-manifest.mdx",
    "website/content/docs/agents/cassettes.mdx",
    "website/content/docs/tour/capabilities.mdx",
    "website/content/docs/tour/contracts.mdx",
    "website/content/docs/tour/testing.mdx",
]
# These fenced blocks deliberately demonstrate rejection, not executable programs.
REJECTIONS = {("capabilities", 1): "V6001", ("capabilities", 2): "V6003"}


def main():
    if not BINARY.is_file():
        raise SystemExit("Build compiler/target/debug/sakura first (cargo build -p sakura_cli).")
    checked = 0
    executed = 0
    failures = []
    with tempfile.TemporaryDirectory(prefix="ws7-docs-", dir="/tmp") as tmp:
        for name in PAGES:
            page = ROOT / name
            blocks = re.findall(r"```sakura[^\n]*\n(.*?)\n```", page.read_text(), re.S)
            for index, code in enumerate(blocks):
                # The rest of the testing tour contains fragments referring to earlier blocks.
                if page.stem == "testing" and not (
                    code.startswith("pub fn increment(") or code.startswith("import std/fs.")
                ):
                    continue
                label = f"{page.stem}-{index}"
                project = Path(tmp) / label
                (project / "src").mkdir(parents=True)
                (project / "src/main.sak").write_text(code + "\n")
                (project / "sakura.toml").write_text('[package]\nname = "docs_sample"\n')
                if page.stem == "cassettes":
                    shutil.copytree(
                        ROOT / "compiler/examples/secrets/tests/fixtures",
                        project / "tests/fixtures",
                    )
                if "Llm.recording(" in code:
                    with (project / "sakura.toml").open("a") as manifest:
                        manifest.write('\n[permissions]\nnet = ["api.anthropic.com:443"]\n')
                    recorded = subprocess.run(
                        [str(BINARY), "test", "--record"],
                        cwd=project, text=True, capture_output=True, timeout=60,
                    )
                    cassette = project / "tests/fixtures/hello.jsonl"
                    if recorded.returncode != 0 or not cassette.is_file():
                        failures.append(label)
                        print(recorded.stdout, recorded.stderr)
                    else:
                        assert "synthetic-key" not in cassette.read_text()
                        # Replay must work without any response from the outer HTTP handler.
                        (project / "src/main.sak").write_text(
                            code.replace(
                                "Http.scripted([(Matcher.Any, response)])", "Http.scripted([])"
                            )
                        )
                    executed += 1
                if code.startswith("pub fn increment("):
                    args = [str(BINARY), "test", "--contracts", "--seed", "42"]
                    for replay in (False, True):
                        result = subprocess.run(
                            args, cwd=project, text=True, capture_output=True, timeout=60
                        )
                        assert result.returncode == 3, result.stdout + result.stderr
                        assert "x = 0: defect D021" in result.stdout, result.stdout
                        assert ("stored regression replayed" in result.stdout) == replay
                        executed += 1
                    regression = json.loads(
                        (project / "tests/regressions/increment.json").read_text()
                    )
                    documented = re.search(r"```json\n(.*?)\n```", page.read_text(), re.S)
                    assert documented and regression == json.loads(documented.group(1))
                    (project / "src/main.sak").write_text(
                        code.replace("{ x }", "{ x + 1 }")
                    )
                    result = subprocess.run(
                        args, cwd=project, text=True, capture_output=True, timeout=60
                    )
                    assert result.returncode == 0, result.stdout + result.stderr
                    executed += 1
                    print(f"{label}: contract failure, regression replay, fixed body: PASS")
                expected = REJECTIONS.get((page.stem, index))
                commands = ["check"]
                if not expected:
                    if re.search(r"^test ", code, re.M):
                        commands.append("test")
                    elif "pub fn main" in code:
                        commands.append("run")
                for command in commands:
                    args = [str(BINARY), command] + ([] if command == "run" else ["--json"])
                    result = subprocess.run(
                        args, cwd=project, text=True, capture_output=True, timeout=60
                    )
                    ok = result.returncode == (1 if expected else 0)
                    if expected:
                        records = [json.loads(line) for line in result.stdout.splitlines()]
                        ok = ok and any(r.get("code") == expected for r in records)
                    if command == "check":
                        checked += 1
                    else:
                        executed += 1
                    print(f"{label}: {command}: {'PASS' if ok else 'FAIL'}")
                    if not ok:
                        failures.append(label)
                        print(result.stdout, result.stderr)
    print(f"{checked} snippets checked; {executed} run/test invocations; {len(failures)} failures")
    raise SystemExit(bool(failures))


if __name__ == "__main__":
    main()
