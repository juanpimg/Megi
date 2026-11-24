"""Static guard for the fuzz-target inventory (M3).

The fuzz crate is a standalone Cargo workspace, so a missing ``[[bin]]`` entry or an
orphaned source file is invisible to the main workspace's build.  These checks read the
manifest directly and fail when the declared inventory shrinks or a declared source is
absent.
"""

from __future__ import annotations

import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "fuzz" / "Cargo.toml"
MIN_TARGETS = 31
NEW_TARGETS = ("cookie_parse", "css_animation", "svg_path")


def declared_bins() -> dict[str, Path]:
    manifest = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))
    entries = manifest.get("bin", [])
    return {
        entry["name"]: ROOT / "fuzz" / entry["path"]
        for entry in entries
    }


class FuzzTargetInventoryTests(unittest.TestCase):
    def test_at_least_31_targets(self) -> None:
        entries = declared_bins()
        self.assertGreaterEqual(
            len(entries),
            MIN_TARGETS,
            f"only {len(entries)} fuzz targets declared ({sorted(entries)})",
        )

    def test_every_declared_bin_source_exists(self) -> None:
        missing = sorted(
            str(path) for path in declared_bins().values() if not path.is_file()
        )
        self.assertEqual(missing, [], "declared fuzz targets without a source file")

    def test_new_targets_are_declared(self) -> None:
        entries = declared_bins()
        for name in NEW_TARGETS:
            self.assertIn(name, entries, f"missing [[bin]] entry for {name}")

    def test_new_targets_have_no_explicit_unsafe(self) -> None:
        for name in NEW_TARGETS:
            path = declared_bins().get(name)
            if path is None or not path.is_file():
                continue  # covered by the two inventory tests above
            self.assertNotRegex(
                path.read_text(encoding="utf-8"),
                r"\bunsafe\b",
                f"{name} must not contain explicit unsafe code",
            )


if __name__ == "__main__":
    unittest.main()
