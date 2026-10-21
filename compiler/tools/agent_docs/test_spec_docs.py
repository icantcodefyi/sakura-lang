"""Run with python3 -m unittest discover -s compiler/tools/agent_docs -p 'test_spec_docs.py'."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import spec_docs


class SpecDocsTests(unittest.TestCase):
    def test_checked_in_table_is_fresh(self):
        result = subprocess.run(
            [sys.executable, str(Path(spec_docs.__file__)), "--check"],
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_baseline_and_fixture_changes_affect_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            baseline = root / "compiler/diagnostics/codes.txt"
            baseline.parent.mkdir(parents=True)
            baseline.write_text("V0001 active syntax_error\nW8001 reserved copy_on_write\n")
            ui = root / "compiler/tests/ui"
            ui.mkdir(parents=True)
            fixture = ui / "V0001_bad.sak"
            fixture.write_text("// milestone: M0\n// expect: V0001\nfn\n")
            before = spec_docs.render(root)
            self.assertIn("V0001_bad.sak", before)
            self.assertIn("| reserved |", before)
            baseline.write_text("V0001 retired syntax_error\nW8001 reserved copy_on_write\n")
            self.assertNotEqual(before, spec_docs.render(root))
            fixture.write_text("// milestone: M0\n// expect: V0001\n// skip: pending\n")
            self.assertNotIn("V0001_bad.sak", spec_docs.render(root))

    def test_missing_fixture_is_an_explicit_gap(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            baseline = root / "compiler/diagnostics/codes.txt"
            baseline.parent.mkdir(parents=True)
            baseline.write_text("V0001 active syntax_error\n")
            self.assertIn("Gap: no runnable emission fixture indexed", spec_docs.render(root))

    def test_removed_named_rust_fixture_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            baseline = root / "compiler/diagnostics/codes.txt"
            baseline.parent.mkdir(parents=True)
            baseline.write_text("D090 active runtime_internal_error\n")
            relative, name = spec_docs.TEST_FIXTURES["D090"]
            fixture = root / relative
            fixture.parent.mkdir(parents=True)
            fixture.write_text(f"#[test]\nfn {name}() {{}}\n")
            self.assertIn(name, spec_docs.render(root))
            fixture.write_text("")
            with self.assertRaisesRegex(ValueError, "missing indexed test"):
                spec_docs.render(root)

    def test_removed_named_js_fixture_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            baseline = root / "compiler/diagnostics/codes.txt"
            baseline.parent.mkdir(parents=True)
            baseline.write_text("V8003 active abi_version_mismatch\n")
            relative, name = spec_docs.TEST_FIXTURES["V8003"]
            fixture = root / relative
            fixture.parent.mkdir(parents=True)
            fixture.write_text(f'test("{name}", async () => {{}});\n')
            self.assertIn(name, spec_docs.render(root))
            fixture.write_text(f'// test("{name}"\n')
            with self.assertRaisesRegex(ValueError, "missing indexed test"):
                spec_docs.render(root)

    def test_broken_link_and_uncited_rule_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            spec = root / "docs/spec"
            spec.mkdir(parents=True)
            (spec / "grammar.md").write_text("**G01** A rule.\n[missing](missing.sak)\n")
            count, errors = spec_docs.check_links(root)
            self.assertEqual(count, 1)
            self.assertEqual(len(errors), 2)

    def test_semantics_labels_need_fixtures(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            spec = root / "docs/spec"
            spec.mkdir(parents=True)
            fixture = root / "compiler/tests/semantics/ok.sak"
            fixture.parent.mkdir(parents=True)
            fixture.write_text("")
            (spec / "evaluation.md").write_text(
                "**EV01** Cited. [ok](../../compiler/tests/semantics/ok.sak)\n"
                "**MO02** Uncited.\n"
                "**XY03** Not a rule label.\n"
            )
            count, errors = spec_docs.check_links(root)
            self.assertEqual(count, 2)
            self.assertEqual(errors, ["evaluation.md:2: rule has no fixture"])


if __name__ == "__main__":
    unittest.main()
