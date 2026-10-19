from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from check_history import compare, initial_registry, parse


class HistoryTests(unittest.TestCase):
    def test_deletion_is_rejected(self):
        self.assertIn("removed", compare({"V1001": ("active", "original")}, {})[0])

    def test_reassignment_is_rejected(self):
        self.assertIn("reassigned", compare(
            {"V1001": ("active", "original")},
            {"V1001": ("active", "different")},
        )[0])

    def test_retired_codes_cannot_be_reactivated_or_reserved(self):
        for status in ["active", "reserved"]:
            with self.subTest(status=status):
                self.assertIn("forbidden", compare(
                    {"V1001": ("retired", "original")},
                    {"V1001": (status, "original")},
                )[0])

    def test_issued_codes_cannot_become_unissued(self):
        self.assertTrue(compare(
            {"V1001": ("active", "original")},
            {"V1001": ("reserved", "original")},
        ))

    def test_addition_retirement_and_activation_are_allowed(self):
        self.assertEqual([], compare(
            {"V1001": ("active", "original"), "W8001": ("reserved", "future")},
            {"V1001": ("retired", "original"), "W8001": ("active", "future"),
             "D001": ("active", "new")},
        ))

    def test_malformed_or_empty_registry_fails_closed(self):
        for text in ["", "# comment", "V1001 active", "V1001 unknown title",
                     "V1001 active title\nV1001 active title", "X1001 active title"]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                parse(text)

    def test_initial_source_baseline_preserves_retirement(self):
        source = '    V5002 "old" CONTRACT,\n"Retired (design): never reported.\n'
        self.assertEqual({"V5002": ("retired", "old")}, initial_registry(source))

    def test_bootstrap_rejects_partial_or_duplicate_source_tables(self):
        valid = '    V1001 "valid" RESOLVE,\n"Explanation.\n'
        for source in ["", valid + ' V1002 "missed" RESOLVE,\n"Explanation.\n', valid + valid]:
            with self.subTest(source=source), self.assertRaises(ValueError):
                initial_registry(source)

    def test_bootstrap_includes_active_and_reserved_entries(self):
        source = '    V1001 "active_code" RESOLVE,\n"Explanation.\n'
        source += '    W8001 "copy_on_write" PACKAGE,\n"Explanation.\n'
        self.assertEqual({"V1001": ("active", "active_code"),
                          "W8001": ("reserved", "copy_on_write")}, initial_registry(source))

    def test_deleting_a_code_added_after_the_initial_baseline_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.check_output(["git", "-C", directory, *args], text=True)
            git("init", "--quiet")
            registry = root / "compiler/diagnostics/codes.txt"
            registry.parent.mkdir(parents=True)
            initial = "V1001 active original\n"
            source = root / "compiler/crates/sakura_diagnostics/src/codes.rs"
            source.parent.mkdir(parents=True)
            source.write_text('    V1001 "original" RESOLVE,\n"Explanation.\n')
            registry.write_text(initial)
            def commit():
                git("add", ".")
                git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
                    "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "baseline")
            commit()
            registry.write_text(initial + "V1002 reserved later\n")
            commit()
            base = git("rev-parse", "HEAD").strip()
            registry.write_text(initial)
            for cwd in (root, root / "compiler", registry.parent):
                with self.subTest(cwd=cwd.relative_to(root)):
                    result = subprocess.run(
                        [sys.executable, str(Path(__file__).with_name("check_history.py").resolve()),
                         "--base", base], cwd=cwd, text=True, capture_output=True, check=False,
                    )
                    self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                    self.assertIn("V1002: removed", result.stderr)


if __name__ == "__main__":
    unittest.main()
