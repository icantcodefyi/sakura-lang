import argparse
import contextlib
import datetime
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import release


def metadata(removed=None, since="1.3.0", date="2026-01-31", earliest="2.0.0", earliest_date="2026-07-31"):
    text = f'''[[deprecation]]
feature = "old_feature"
code = "W9001"
since = {{ version = "{since}", date = {date} }}
replacement = "new_feature"
earliest_removal = {{ version = "{earliest}", date = {earliest_date} }}
'''
    return text + (f'removed = "{removed}"\n' if removed else "")


class Prepare(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.compiler = self.root / "compiler"
        self.compiler.mkdir()
        self.git("init", "--quiet")
        self.git("config", "user.name", "Release fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.patch = patch.multiple(release, ROOT=self.root, COMPILER=self.compiler)
        self.patch.start()
        self.addCleanup(self.patch.stop)
        self.write("1.3.0")
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "initial")

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True, stderr=subprocess.PIPE)

    def write(self, version, msrv="1.99", deprecations="deprecation = []\n"):
        (self.compiler / "Cargo.toml").write_text(
            f'[workspace.package]\nversion = "{version}"\nrust-version = "{msrv}"\n')
        (self.compiler / "deprecations.toml").write_text(deprecations)

    def tag(self, version, deprecations="deprecation = []\n", msrv="1.99"):
        self.write(version, msrv, deprecations)
        self.git("add", ".")
        self.git("commit", "--quiet", "--allow-empty", "-m", version)
        self.git("tag", f"v{version}")

    def check(self, previous=None, date="2026-07-31"):
        with contextlib.redirect_stdout(io.StringIO()):
            release.prepare(argparse.Namespace(previous_tag=previous, date=date))

    def refusal(self, message):
        return self.assertRaisesRegex(SystemExit, message)

    def window(self):
        self.tag("1.3.0", metadata())
        self.tag("1.4.0", metadata())
        self.write("2.0.0", deprecations=metadata("2.0.0"))

    def test_empty_first_minor_candidate(self):
        self.check()

    def test_first_patch_without_previous_tag_is_refused(self):
        self.write("1.3.1")
        with self.refusal("without release history"):
            self.check()

    def test_first_removal_without_previous_tag_is_refused(self):
        self.write("2.0.0", deprecations=metadata("2.0.0"))
        with self.refusal("without release history"):
            self.check()

    def test_patch_msrv_must_not_change_in_either_direction(self):
        self.tag("1.3.0")
        for msrv in ("1.98", "1.100"):
            self.write("1.3.1", msrv)
            with self.subTest(msrv=msrv), self.refusal("patch release MSRV differs"):
                self.check()
        self.write("1.3.1")
        self.check()

    def test_minor_msrv_change_is_allowed(self):
        self.tag("1.3.0")
        self.write("1.4.0", "1.100")
        self.check()

    def test_prerelease_promotion_preserves_msrv(self):
        self.tag("1.3.1-alpha.1")
        self.write("1.3.1", "1.100")
        with self.refusal("patch release MSRV differs"):
            self.check()

    def test_previous_tag_cannot_bypass_latest_baseline(self):
        self.tag("1.2.0", msrv="1.98")
        self.tag("1.3.0")
        self.write("1.3.1", "1.98")
        with self.refusal("latest preceding ancestor"):
            self.check("v1.2.0")

    def test_future_tags_are_ignored_and_semver_orders_prereleases(self):
        self.tag("1.3.0-alpha.2")
        self.tag("1.3.0-alpha.10")
        self.tag("9.0.0")
        self.write("1.3.0")
        self.check("v1.3.0-alpha.10")

    def test_shallow_history_is_refused(self):
        original = release.git_text
        def git_text(*args):
            return "true\n" if args[0] == "rev-parse" else original(*args)
        with patch.object(release, "git_text", side_effect=git_text), self.refusal("full history"):
            self.check()

    def test_removal_exact_version_date_and_major_boundary(self):
        self.window()
        self.check()
        with self.refusal("removal date precedes"):
            self.check(date="2026-07-30")

    def test_removal_version_threshold_independently_refused(self):
        self.write("2.0.0", deprecations=metadata("2.0.0", earliest="3.0.0"))
        with self.refusal("removed precedes earliest_removal"):
            self.check()

    def test_removal_outside_major_bump_refused(self):
        self.tag("1.3.0", metadata())
        self.tag("1.4.0", metadata())
        self.tag("2.0.0", metadata())
        self.write("2.1.0", deprecations=metadata("2.1.0"))
        with self.refusal("occur in a major bump"):
            self.check()

    def test_prereleases_do_not_count_as_shipped_minors(self):
        self.tag("1.3.0", metadata())
        self.tag("1.4.0-alpha.1", metadata())
        self.write("2.0.0", deprecations=metadata("2.0.0"))
        with self.refusal("two shipped final minor"):
            self.check()

    def test_tags_without_the_warning_do_not_start_its_clock(self):
        self.tag("1.3.0")
        self.tag("1.4.0", metadata())
        self.write("2.0.0", deprecations=metadata("2.0.0"))
        with self.refusal("two shipped final minor"):
            self.check()

    def test_historical_removal_does_not_fail_later_patch(self):
        self.window()
        self.tag("2.0.0", metadata("2.0.0"))
        self.write("2.0.1", deprecations=metadata("2.0.0"))
        self.check()

    def test_history_cannot_delete_or_rewrite_clock(self):
        self.tag("1.3.0", metadata())
        self.write("1.4.0")
        with self.refusal("retain historical"):
            self.check()
        self.write("1.4.0", deprecations=metadata(date="2026-02-01", earliest_date="2026-08-01"))
        with self.refusal("historical since"):
            self.check()

    def test_metadata_refuses_unknown_malformed_and_duplicate_entries(self):
        cases = ("broken [", "other = []", metadata() + 'unknown = true\n',
                 metadata() + metadata(), metadata().replace('code = "W9001"', 'code = "V9001"'),
                 metadata().replace('replacement = "new_feature"', 'reason = ""'),
                 metadata(since="1.3.0-alpha.1"), metadata(date="2026-02-30"),
                 metadata(earliest="1.4.0"), metadata(earliest_date="2026-07-30"))
        for text in cases:
            with self.subTest(text=text), self.assertRaises(SystemExit):
                release.deprecations(text)

    def test_calendar_month_end_is_clamped(self):
        entry = release.deprecations(metadata(date="2025-08-31", earliest_date="2026-02-28"))
        self.assertEqual(entry["old_feature"]["earliest_removal"]["date"], datetime.date(2026, 2, 28))

    def test_history_cannot_change_code_earliest_removal_or_a_shipped_removal(self):
        self.tag("1.3.0", metadata())
        self.write("1.4.0", deprecations=metadata().replace('code = "W9001"', 'code = "W9002"'))
        with self.refusal("historical code"):
            self.check()
        self.write("1.4.0", deprecations=metadata(earliest="2.1.0"))
        with self.refusal("historical earliest_removal"):
            self.check()
        self.tag("1.4.0", metadata())
        self.tag("2.0.0", metadata("2.0.0"))
        self.write("3.0.0", deprecations=metadata("3.0.0"))
        with self.refusal("historical removal"):
            self.check()

    def test_a_new_removal_must_name_this_candidate(self):
        self.window()
        self.write("2.0.0", deprecations=metadata("3.0.0"))
        with self.refusal("must name this candidate"):
            self.check()

    def test_tags_off_the_ancestry_are_not_the_previous_release(self):
        self.tag("1.3.0")
        self.git("checkout", "--quiet", "-b", "side")
        self.tag("1.3.1", msrv="1.98")
        self.git("checkout", "--quiet", "-")
        self.write("1.3.2")
        self.check("v1.3.0")
        with self.refusal("latest preceding ancestor"):
            self.check("v1.3.1")

    def test_the_checked_in_metadata_is_valid(self):
        text = (Path(release.__file__).resolve().parents[2] / "compiler/deprecations.toml").read_text()
        self.assertIsInstance(release.deprecations(text), dict)

    def test_missing_old_metadata_is_allowed_before_registry_existed(self):
        (self.compiler / "deprecations.toml").unlink()
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "before registry")
        self.git("tag", "v1.3.0")
        self.write("1.3.1")
        self.check()


if __name__ == "__main__":
    unittest.main()
