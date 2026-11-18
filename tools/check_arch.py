#!/usr/bin/env python3
"""Architecture linter for the Megi workspace.

Checks the crate DAG, forbidden ownership patterns and file-size limits.
Anything recorded in docs/architecture-baseline.json is reported as `legacy`
(informational); anything new is a violation (exit 1).
"""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASELINE_PATH = ROOT / "docs" / "architecture-baseline.json"
MAX_LINES = 500


def normalize(name: str) -> str:
    """Crate names may be `megi-core`, `megi_core` or plain `core`."""
    n = name.replace("-", "_")
    return n[5:] if n.startswith("megi_") else n


def allowed_edges() -> dict[str, set[str]]:
    """Target DAG from design doc §2.3 (plain crate names, no `megi_` prefix)."""
    return {
        "core": set(),
        "html": {"core"},
        "css": {"core"},
        "fonts": set(),
        "image": set(),
        "net": {"image"},  # MIME sniffing / image decode in the loader
        "js": {"core", "html", "css"},
        "display": {"core", "fonts"},
        "layout": {"core", "css", "fonts"},
        "paint": {"core", "css", "layout", "fonts", "image", "display"},
        "raster": {"core", "display", "fonts"},
        "svg": {"image", "fonts", "display"},
        "render": {
            "core", "html", "css", "layout", "fonts", "image",
            "display", "paint", "raster", "net", "js", "svg",
        },
        "webview": {"core", "display", "net", "render"},
        "browser": {"webview"},
        "shell": {"webview"},  # pre-rename alias of `browser`
        "test_harness": {"core", "html", "css", "js"},
        "bench": {"core", "html", "css", "layout", "fonts", "image", "paint", "render"},
    }


def violations_for_edge(src: str, dst: str, allowed: dict[str, set[str]]) -> list[str]:
    """Return a non-empty list when `src -> dst` is not an allowed edge."""
    a, b = normalize(src), normalize(dst)
    if a == b:
        return []  # a crate referring to itself by package name is not an edge
    if a not in allowed:
        return [f"unknown source crate: {src}"]
    if b in allowed[a]:
        return []
    return [f"forbidden edge: {a} -> {b}"]


def is_legacy(path: str, baseline: dict) -> bool:
    """True when the oversized file was already recorded in the baseline."""
    return str(path) in baseline.get("oversized_files", {})


def _package_names() -> dict[str, str]:
    """Map `crates/<dir>` to the normalized package name."""
    root = tomllib.loads((ROOT / "Cargo.toml").read_text())
    out: dict[str, str] = {}
    for member in root["workspace"]["members"]:
        manifest = ROOT / member / "Cargo.toml"
        if manifest.exists():
            name = tomllib.loads(manifest.read_text())["package"]["name"]
            out[member] = normalize(name)
    return out


def _comment_free(line: str) -> bool:
    s = line.strip()
    return not (s.startswith("//") or s.startswith("/*") or s.startswith("*"))


def scan() -> tuple[list[str], int]:
    baseline = json.loads(BASELINE_PATH.read_text())
    allowed = allowed_edges()
    packages = _package_names()
    known = set(packages.values())
    legacy_edges = {k: set(v) for k, v in baseline.get("edges", {}).items()}
    violations: list[str] = []
    legacy_count = 0

    use_re = re.compile(r"^\s*(?:pub\s+)?use\s+(?:megi_)?([a-z0-9_]+)(?:::|;)")
    pattern_re = re.compile(r"\b(?:Rc<|Weak<|unsafe )")

    for member, name in packages.items():
        manifest = tomllib.loads((ROOT / member / "Cargo.toml").read_text())
        # Dev-dependencies are intentionally not part of the production DAG.
        for dep in manifest.get("dependencies", {}):
            dst = normalize(dep)
            if dst not in known:
                continue
            msgs = violations_for_edge(name, dst, allowed)
            if msgs and dst not in legacy_edges.get(name, set()):
                violations.append(f"{member}/Cargo.toml: {msgs[0]}")
            elif msgs:
                legacy_count += 1

        src = ROOT / member / "src"
        for path in sorted(src.rglob("*.rs")) if src.exists() else []:
            rel = path.relative_to(ROOT).as_posix()
            lines = path.read_text(errors="replace").splitlines()
            if len(lines) > MAX_LINES:
                if is_legacy(rel, baseline):
                    legacy_count += 1
                else:
                    violations.append(f"{rel}: {len(lines)} lines (max {MAX_LINES})")
            for lineno, line in enumerate(lines, 1):
                if "forbid(unsafe_code)" in line or not _comment_free(line):
                    continue
                key = f"{rel}:{lineno}"
                if pattern_re.search(line):
                    if key in baseline.get("forbidden_patterns", {}):
                        legacy_count += 1
                    else:
                        violations.append(f"{key}: forbidden ownership/unsafe pattern")
                m = use_re.match(line)
                if m and m.group(1) in known:
                    msgs = violations_for_edge(name, m.group(1), allowed)
                    if msgs and m.group(1) not in legacy_edges.get(name, set()):
                        violations.append(f"{key}: {msgs[0]}")
                    elif msgs:
                        legacy_count += 1
    return violations, legacy_count


def main() -> int:
    violations, legacy_count = scan()
    print(f"check_arch: {legacy_count} legacy items (informational)")
    if violations:
        print(f"check_arch: {len(violations)} violation(s):")
        for v in violations:
            print(f"  - {v}")
        return 1
    print("check_arch: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
