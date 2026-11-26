#!/usr/bin/env python3
"""Unit tests for tools/test_inspector.py (Python only)."""
from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("test_inspector.py")
_spec = importlib.util.spec_from_file_location("test_inspector", MODULE_PATH)
inspector = importlib.util.module_from_spec(_spec)
sys.modules["test_inspector"] = inspector
_spec.loader.exec_module(inspector)


class SanitizeTests(unittest.TestCase):
    def test_none_is_preserved(self) -> None:
        self.assertIsNone(inspector.sanitize_surrogates(None))

    def test_surrogate_is_escaped_and_normal_text_untouched(self) -> None:
        self.assertEqual(inspector.sanitize_surrogates("ok\ud800"), "ok\\uD800")
        self.assertEqual(inspector.sanitize_surrogates("plain"), "plain")

    def test_nested_values_are_sanitized(self) -> None:
        value = {"a": ["x\udfff", 3], "b": None}
        self.assertEqual(
            inspector._sanitize_value(value), {"a": ["x\\uDFFF", 3], "b": None}
        )

    def test_parser_is_available(self) -> None:
        parser = inspector.build_parser()
        self.assertIsNotNone(parser)


if __name__ == "__main__":
    unittest.main()
