#!/usr/bin/env python3
"""Unit tests for tools/whatwg_spec.py (Python only, needs lxml)."""
from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("whatwg_spec.py")
_spec = importlib.util.spec_from_file_location("whatwg_spec", MODULE_PATH)
spec_tool = importlib.util.module_from_spec(_spec)
sys.modules["whatwg_spec"] = spec_tool
_spec.loader.exec_module(spec_tool)

HTML = """<!doctype html><html><body>
<h2 id='sec-one'><span class='secno'>1.</span> One &sect;</h2>
<p>Body text.</p>
<h3 id='sec-two'>Two &sect;</h3>
<p>More.</p>
</body></html>
"""


class ParseHeadingsTests(unittest.TestCase):
    def test_extracts_section_number_id_and_title(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "spec.html"
            path.write_text(HTML, encoding="utf-8")
            headings = spec_tool.parse_headings(path)
        self.assertEqual(len(headings), 2)
        self.assertEqual(headings[0][0], "1.")
        self.assertEqual(headings[0][1], "sec-one")
        self.assertEqual(headings[0][2], "One §")
        self.assertEqual(headings[1][1], "sec-two")
        self.assertEqual(headings[1][2], "Two §")

    def test_extract_section_text_joins_paragraphs(self) -> None:
        from lxml import html

        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "spec.html"
            path.write_text(HTML, encoding="utf-8")
            heading = spec_tool.parse_headings(path)[0][3]
        text = spec_tool.extract_section_text(heading)
        self.assertIn("# 1. One § (#sec-one)", text)
        self.assertIn("Body text.", text)
        # A deeper heading belongs to this section; a same-level one would stop it.
        self.assertIn("Two §", text)

    def test_parser_is_available(self) -> None:
        self.assertIsNotNone(spec_tool.build_parser())


class LazyDependencyTests(unittest.TestCase):
    def test_import_does_not_require_lxml(self) -> None:
        # #2454: importing the tool must work before lxml is installed.
        script = (
            "import builtins\n"
            "real_import = builtins.__import__\n"
            "def guarded(name, *args, **kwargs):\n"
            "    if name == 'lxml' or name.startswith('lxml.'):\n"
            "        raise ImportError('simulated missing lxml')\n"
            "    return real_import(name, *args, **kwargs)\n"
            "builtins.__import__ = guarded\n"
            f"import importlib.util\n"
            f"s = importlib.util.spec_from_file_location('whatwg', {str(MODULE_PATH)!r})\n"
            "m = importlib.util.module_from_spec(s)\n"
            "s.loader.exec_module(m)\n"
            "assert m._hash_sidecar is not None\n"
        )
        result = subprocess.run(
            [sys.executable, "-c", script], capture_output=True, text=True
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_lxml_message_is_actionable_at_use_time(self) -> None:
        with mock.patch.dict(sys.modules, {"lxml": None, "lxml.html": None}):
            with self.assertRaises(RuntimeError) as caught:
                spec_tool._lxml_html()
        self.assertIn("python3-lxml", str(caught.exception))


class CacheHashTests(unittest.TestCase):
    def test_same_size_corruption_is_detected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            cache = Path(tmp) / "spec.html"
            cache.write_bytes(b"abcdef")
            sidecar = spec_tool._hash_sidecar(cache)
            sidecar.write_text(__import__("hashlib").sha256(b"abcdef").hexdigest())
            self.assertTrue(spec_tool._valid_cached(cache))
            cache.write_bytes(b"ABCDEF")  # identical size, different bytes
            self.assertFalse(spec_tool._valid_cached(cache))

    def test_missing_sidecar_forces_refetch(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            cache = Path(tmp) / "spec.html"
            cache.write_bytes(b"abcdef")
            self.assertFalse(spec_tool._valid_cached(cache))


if __name__ == "__main__":
    unittest.main()
