import unittest

from check_json import compare, parse

BASE = parse("""
test 1.0 name string required
test 1.0 defect object|null optional
""")


def errors(text):
    return compare(BASE, parse(text))


class JsonContractTests(unittest.TestCase):
    def test_unchanged_passes(self):
        self.assertEqual([], errors("test 1.0 name string required\ntest 1.0 defect object|null optional"))

    def test_removed_line_fails(self):
        self.assertIn("removed", errors("test 1.0 name string required")[0])

    def test_changed_type_fails(self):
        self.assertIn("type changed", errors("test 1.0 name integer required\ntest 1.0 defect object|null optional")[0])

    def test_optional_to_required_fails(self):
        self.assertIn("optional -> required", errors("test 1.0 name string required\ntest 1.0 defect object|null required")[0])

    def test_required_to_optional_fails(self):
        # Design §17.242 (4): a 1.0 consumer that reads a required field must keep finding it.
        found = errors("test 1.0 name string optional\ntest 1.0 defect object|null optional")
        self.assertEqual(["test 1.0 name: required -> optional"], found)

    def test_minor_bump_making_a_required_line_optional_fails(self):
        found = errors("""
test 1.0 name string required
test 1.0 defect object|null optional
test 1.1 name string optional
test 1.1 defect object|null optional
""")
        self.assertEqual(["test 1.1 name: not kept from 1.0 as it was"], found)

    def test_minor_bump_making_an_optional_line_required_fails(self):
        found = errors("""
test 1.0 name string required
test 1.0 defect object|null optional
test 1.1 name string required
test 1.1 defect object|null required
""")
        self.assertEqual(["test 1.1 defect: not kept from 1.0 as it was"], found)

    def test_addition_without_minor_bump_fails(self):
        found = errors("test 1.0 name string required\ntest 1.0 defect object|null optional\ntest 1.0 seed integer required")
        self.assertIn("without a MINOR bump", found[0])

    def test_minor_bump_keeping_every_line_passes(self):
        self.assertEqual([], errors("""
test 1.0 name string required
test 1.0 defect object|null optional
test 1.1 name string required
test 1.1 defect object|null optional
test 1.1 seed integer required
"""))

    def test_minor_bump_dropping_a_line_fails(self):
        found = errors("test 1.0 name string required\ntest 1.0 defect object|null optional\ntest 1.1 name string required")
        self.assertIn("not kept from 1.0", found[0])

    def test_major_bump_fails(self):
        found = errors("test 1.0 name string required\ntest 1.0 defect object|null optional\ntest 2.0 name string required")
        self.assertIn("MAJOR", found[0])

    def test_skipped_minor_fails(self):
        found = errors("test 1.0 name string required\ntest 1.0 defect object|null optional\ntest 1.2 name string required\ntest 1.2 defect object|null optional")
        self.assertIn("next MINOR", found[0])

    def test_new_kind_not_at_1_0_fails(self):
        found = errors("test 1.0 name string required\ntest 1.0 defect object|null optional\naudit 1.3 site string required")
        self.assertIn("starts at 1.0", found[0])

    def test_new_kind_passes(self):
        self.assertEqual([], errors("test 1.0 name string required\ntest 1.0 defect object|null optional\naudit 1.0 site string required"))


if __name__ == "__main__":
    unittest.main()
