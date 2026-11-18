#!/usr/bin/env python3
"""Tests for the architecture linter (Task 0.1 RED fixture)."""

import unittest

try:
    from tools import check_arch
except ImportError:  # RED: the module is implemented in Task 0.2
    check_arch = None


class TestCheckArchPredicates(unittest.TestCase):
    def setUp(self):
        if check_arch is None:
            self.fail("tools.check_arch is missing (Task 0.2 not implemented)")

    def test_unknown_edge_is_violation(self):
        allowed = check_arch.allowed_edges()
        self.assertTrue(
            check_arch.violations_for_edge("paint", "render", allowed),
            "paint -> render must be reported as a forbidden edge",
        )

    def test_known_edge_is_allowed(self):
        allowed = check_arch.allowed_edges()
        self.assertEqual(
            check_arch.violations_for_edge("html", "core", allowed),
            [],
            "html -> core is the allowed direction",
        )

    def test_oversized_file_is_legacy_when_in_baseline(self):
        baseline = {"oversized_files": {"crates/js/src/runtime.rs": 11028}}
        self.assertTrue(
            check_arch.is_legacy("crates/js/src/runtime.rs", baseline),
            "a file recorded in the baseline is legacy, not a new violation",
        )


if __name__ == "__main__":
    unittest.main()
