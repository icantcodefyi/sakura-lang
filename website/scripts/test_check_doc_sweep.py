"""Run with python3 -m unittest discover -s website/scripts -p 'test_check_doc_sweep.py'."""

import importlib.util
from pathlib import Path
import shutil
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("check_doc_sweep", HERE / "check-doc-sweep.py")
sweep = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sweep)


class MilestoneGuardTests(unittest.TestCase):
    """design §17.256 (5): no milestone name in user-facing text outside the roadmap."""

    def test_checked_in_tree_names_no_milestone(self):
        self.assertEqual(sweep.milestone_names(), [])

    def test_planted_names_are_caught_and_exemptions_hold(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for part in ("website/content/docs", "skills/sakura"):
                shutil.copytree(sweep.ROOT / part, root / part)
            for part in ("docs/llms.txt", "website/lib/source.ts"):
                (root / part).parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(sweep.ROOT / part, root / part)
            self.assertEqual(sweep.milestone_names(root), [])

            def plant(part, old, new):
                path = root / part
                text = path.read_text(encoding="utf-8")
                self.assertIn(old, text)
                path.write_text(text.replace(old, new, 1), encoding="utf-8")

            # Exempt: the roadmap page, and lib/source.ts outside its llms.txt template.
            plant("website/content/docs/roadmap.mdx", "\n", "\nM4 is named here.\n")
            plant("website/lib/source.ts", "\n", "\n// M3 in a code comment\n")
            self.assertEqual(sweep.milestone_names(root), [])

            plant("website/content/docs/faq.mdx", "\n", "\nShipped in M4.\n")
            plant("website/content/docs/tour/memory.mdx", "\n", "\nThe next Milestone.\n")
            plant("skills/sakura/references/language.md", "\n", "\nSee M5.\n")
            plant("docs/llms.txt", "\n", "\nAfter M2.\n")
            plant("website/lib/source.ts", "Install it:", "M0 onward. Install it:")
            hits = sweep.milestone_names(root)
            self.assertEqual(len(hits), 5, hits)
            for where in ("faq.mdx:2:", "memory.mdx:2:", "language.md:2:", "docs/llms.txt:2:", "website/lib/source.ts:"):
                self.assertTrue(any(where in hit for hit in hits), (where, hits))
            # Words and paths that only look like a name are not hits.
            plant("website/content/docs/faq.mdx", "Shipped in M4.", "Measured on M40 and HM4; see #m5-native.")
            self.assertEqual(len(sweep.milestone_names(root)), 4)


if __name__ == "__main__":
    unittest.main()
