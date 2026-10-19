import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from changes import AREAS, classify, comparison_base, detect


class ChangeDetectionTests(unittest.TestCase):
    def test_area_boundaries(self):
        cases = {
            "docs/guide.md": {"docs"}, "README.md": {"docs"},
            "research/notes.txt": {"docs"}, "website/content/docs/page.mdx": {"website"},
            "compiler/README.md": {"compiler"},
            "compiler/contracts/json.txt": {"compiler"},
            "compiler/diagnostics/codes.txt": {"compiler"}, "runtime/js/rt.js": {"compiler"},
            "std/core.sak": {"compiler"}, "bench/programs/fib.sak": {"compiler"},
            "editors/vscode/package.json": {"compiler"},
            ".github/workflows/ci.yml": {"workflows"}, "tools/install/test.sh": {"workflows"},
            "packaging/targets.json": {"workflows"}, ".githooks/pre-push": set(AREAS),
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                self.assertEqual(classify([path]), expected)
        self.assertEqual(classify([]), set())
        self.assertEqual(classify(["README.md", "website/app/page.tsx"]), {"docs", "website"})

    def test_full_runs_and_missing_history(self):
        for event, base in (("schedule", ""), ("workflow_dispatch", ""), ("merge_group", "0" * 40), ("merge_group", "bad")):
            self.assertEqual(detect(event, base, "a" * 40), set(AREAS))
        with patch("subprocess.check_output", side_effect=subprocess.CalledProcessError(128, "git")):
            self.assertEqual(detect("merge_group", "b" * 40, "a" * 40), set(AREAS))

    def test_manual_default_is_full_and_explicit_comparison_can_be_focused(self):
        base, head = "a" * 40, "b" * 40
        self.assertEqual(detect("workflow_dispatch", base, head, force_all=True), set(AREAS))
        self.assertEqual(detect("schedule", base, head), set(AREAS))
        self.assertEqual(comparison_base("workflow_dispatch", base), base)
        with patch("subprocess.check_output", return_value=b"README.md\0"):
            self.assertEqual(detect("workflow_dispatch", base, head), {"docs"})

    def test_cancelled_compiler_push_is_carried_into_docs_push(self):
        green, cancelled, docs = "a" * 40, "b" * 40, "c" * 40
        def command(args, **kwargs):
            if args[0] == "gh":
                return green
            self.assertEqual(args[-2:], [green, docs])
            return b"compiler/src/lib.rs\0README.md\0"
        with patch.dict(os.environ, GITHUB_REPOSITORY="owner/repo"):
            with patch("subprocess.check_output", side_effect=command):
                base = comparison_base("push", cancelled)
                self.assertEqual(base, green)
                self.assertEqual(detect("push", base, docs), {"compiler", "docs"})
            with patch("subprocess.check_output", return_value=""):
                base = comparison_base("push", cancelled)
                self.assertEqual(base, "")
                self.assertEqual(detect("push", base, docs), set(AREAS))
            with patch("subprocess.check_output", side_effect=OSError("gh unavailable")):
                base = comparison_base("push", cancelled)
                self.assertEqual(base, "")
                self.assertEqual(detect("push", base, docs), set(AREAS))

    def test_push_range_includes_all_commits_and_deleted_renamed_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            def git(*args):
                return subprocess.check_output(["git", "-C", tmp, *args], text=True).strip()
            git("init", "-q")
            git("config", "user.email", "ci@example.invalid")
            git("config", "user.name", "CI test")
            root = Path(tmp)
            (root / "compiler").mkdir()
            (root / "compiler/old.sak").write_text("old")
            git("add", ".")
            git("commit", "-qm", "initial")
            base = git("rev-parse", "HEAD")
            git("mv", "compiler/old.sak", "README.md")
            git("commit", "-qm", "rename")
            (root / "website").mkdir()
            (root / "website/file with\nnewline.txt").write_text("website")
            git("add", ".")
            git("commit", "-qm", "website")
            head = git("rev-parse", "HEAD")
            previous = os.getcwd()
            try:
                os.chdir(tmp)
                self.assertEqual(detect("merge_group", base, head), {"compiler", "docs", "website"})
                self.assertEqual(detect("pull_request", base, head), {"compiler", "docs", "website"})
            finally:
                os.chdir(previous)


if __name__ == "__main__":
    unittest.main()
