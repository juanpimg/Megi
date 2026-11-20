#!/usr/bin/env python3
"""Regression: the cargo-fuzz install must be skipped on a cache hit (#1832)."""

from __future__ import annotations

import unittest
from pathlib import Path

WORKFLOW = Path(__file__).resolve().parent.parent / ".github" / "workflows" / "fuzz-nightly.yml"


class FuzzCacheTests(unittest.TestCase):
    def test_every_cargo_fuzz_install_is_guarded_by_a_cache_hit(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        lines = text.splitlines()
        install_indices = [
            i for i, line in enumerate(lines)
            if line.strip() == "- name: Install cargo-fuzz"
        ]
        self.assertGreaterEqual(len(install_indices), 3, "expected fuzz install steps")

        for install in install_indices:
            install_block = "\n".join(lines[install:install + 8])
            run_lines = [line for line in install_block.splitlines()
                         if "cargo install cargo-fuzz" in line]
            self.assertEqual(len(run_lines), 1, "expected one install command")
            self.assertNotIn("--force", run_lines[0],
                             "install must not force-reinstall over a restored binary")

            # Find the cache step id above the install block.
            prior = "\n".join(lines[:install])
            cache_ids = [
                line.split("id:", 1)[1].strip()
                for line in prior.splitlines()
                if line.strip().startswith("id:") and "cargo-fuzz" in line
            ]
            self.assertTrue(cache_ids, "cache step needs an id to expose cache-hit")
            cache_id = cache_ids[-1]
            self.assertIn(
                f"if: steps.{cache_id}.outputs.cache-hit != 'true'",
                install_block,
                "install must be skipped when the binary was restored from cache",
            )


if __name__ == "__main__":
    unittest.main()
