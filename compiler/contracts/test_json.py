import unittest

from check_json import compare, is_reset, parse

LINES = """
test 1.0 name string required
test 1.0 defect object|null optional
"""
BASE = parse(LINES)


def errors(text):
    """The errors of BASE -> `text`, the same with no `baseline` line (§17.257 (1) (b)) as with
    `baseline 0.2.0` on both sides ((c))."""
    without = compare(BASE, parse(text))
    with_line = compare(parse("baseline 0.2.0\n" + LINES), parse("baseline 0.2.0\n" + text))
    assert without == with_line, (without, with_line)
    return without


class BaselineTransitionTests(unittest.TestCase):
    """Design §17.257 (1): the six transitions of the `baseline` line."""

    def test_a_no_line_to_0_2_0_is_the_reset_and_compares_nothing(self):
        current = parse("baseline 0.2.0\nsummary 1.0 kind string required\n")
        self.assertTrue(is_reset(BASE, current))
        self.assertEqual([], compare(BASE, current))

    def test_b_no_line_on_either_side_keeps_the_rules(self):
        self.assertIn("removed", compare(BASE, parse("test 1.0 name string required"))[0])

    def test_c_the_same_line_on_both_sides_keeps_the_rules(self):
        previous = parse("baseline 0.2.0\n" + LINES)
        current = parse("baseline 0.2.0\ntest 1.0 name string required\n")
        self.assertFalse(is_reset(previous, current))
        self.assertEqual(["test 1.0 defect: removed"], compare(previous, current))

    def test_d_a_changed_line_fails_the_reset_is_spent(self):
        found = compare(parse("baseline 0.2.0\n" + LINES), parse("baseline 0.3.0\n" + LINES))
        self.assertEqual(["baseline changed from 0.2.0 to 0.3.0: the one reset is spent"], found)

    def test_e_a_removed_line_fails(self):
        found = compare(parse("baseline 0.2.0\n" + LINES), parse(LINES))
        self.assertEqual(["baseline 0.2.0: baseline line removed"], found)

    def test_f_a_reset_naming_another_release_fails(self):
        found = compare(BASE, parse("baseline 0.3.0\n" + LINES))
        self.assertEqual(["baseline 0.3.0: a reset names 0.2.0"], found)

    def test_a_second_line_and_a_malformed_version_do_not_parse(self):
        for text in ["baseline 0.2.0\nbaseline 0.2.0\n" + LINES, "baseline 0.2\n" + LINES,
                     "baseline 0.2.0 extra\n" + LINES, LINES + "baseline 0.2.0\n"]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                parse(text)


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
