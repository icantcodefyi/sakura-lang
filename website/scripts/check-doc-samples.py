"""Check the runnable WS7 docs against the source-built CLI (Python 3, no dependencies)."""

import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "compiler/target/debug/sakura"
PAGES = [
    "README.md",
    "website/content/docs/agents/index.mdx",
    "website/content/docs/agents/audit.mdx",
    "website/content/docs/agents/doc.mdx",
    "website/content/docs/agents/build-manifest.mdx",
    "website/content/docs/agents/cassettes.mdx",
    "website/content/docs/tour/capabilities.mdx",
    "website/content/docs/tour/contracts.mdx",
]
# These fenced blocks deliberately demonstrate rejection, not executable programs.
REJECTIONS = {("capabilities", 1): "V6001", ("capabilities", 2): "V6003"}


def main():
    if not BINARY.is_file():
        raise SystemExit("Build compiler/target/debug/sakura first (cargo build -p sakura_cli).")
    checked = 0
    executed = 0
    failures = []
    with tempfile.TemporaryDirectory(prefix="ws7-docs-", dir=ROOT / "compiler/target") as tmp:
        for name in PAGES:
            page = ROOT / name
            blocks = re.findall(r"```sakura[^\n]*\n(.*?)\n```", page.read_text(), re.S)
            for index, code in enumerate(blocks):
                label = f"{page.stem}-{index}"
                project = Path(tmp) / label
                (project / "src").mkdir(parents=True)
                (project / "src/main.sakura").write_text(code + "\n")
                (project / "sakura.toml").write_text('[package]\nname = "docs_sample"\n')
                if page.stem == "cassettes":
                    shutil.copytree(
                        ROOT / "compiler/examples/secrets/tests/fixtures",
                        project / "tests/fixtures",
                    )
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
