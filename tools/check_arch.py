#!/usr/bin/env python3
"""Architecture linter for the Megi workspace.

Checks the production crate DAG, forbidden ownership patterns and file-size limits.
Dev-dependencies are intentionally excluded (e.g. `megi-net` uses `megi-js` only
in PAC tests), so "no Boa in the light build" means `cargo tree -e normal`.
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

# #1763: match both angle-bracket types and static constructors (`Rc::new`,
# `Weak::upgrade`, ...). `unsafe` matches the keyword followed by a word boundary.
FORBIDDEN_RE = re.compile(r"\b(?:Rc::|Weak::|Rc<|Weak<|unsafe(?=\s|\{))")
QUALIFIED_RE = re.compile(r"\b(?:::)?(megi_[a-z0-9_]+)::")


def qualified_edges(line: str, known: set[str], local_modules: set[str] | None = None) -> set[str]:
    """Crates referenced through a fully qualified path in an expression."""
    local = local_modules or set()
    return {normalize(match.group(1)) for match in QUALIFIED_RE.finditer(line)} & known - local


def normalize(name: str) -> str:
    """Crate names may be `megi-core`, `megi_core` or plain `core`."""
    n = name.replace("-", "_")
    return n[5:] if n.startswith("megi_") else n


def allowed_edges() -> dict[str, set[str]]:
    """Target DAG from docs/architecture-deps.toml (single source of truth, #1838).

    AGENTS.md §3 and docs/ARCHITECTURE.md document the same edges in prose;
    tools/test_check_arch.py asserts the manifest stays in sync with them.
    """
    manifest = tomllib.loads((ROOT / "docs" / "architecture-deps.toml").read_text())
    return {name: set(deps) for name, deps in manifest["edges"].items()}


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

    use_re = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+(?:megi_)?([a-z0-9_]+)\s*(?:::|as\b|;)")

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
        local_modules = set()
        if src.exists():
            local_modules = {p.stem for p in src.glob("*.rs") if p.stem not in {"lib", "main"}}
            for p in src.rglob("*.rs"):
                local_modules.update(
                    re.findall(r"(?:pub\s+)?mod\s+([a-z0-9_]+)\s*;", p.read_text(errors="replace"))
                )
        # AGENTS.md §4 caps EVERY .rs file, including integration tests.
        # Report size violations for both src/ and tests/; only src/ is scanned
        # for import edges (dev-dependencies are not part of the production DAG).
        size_roots = [r for r in (src, ROOT / member / "tests") if r.exists()]
        sized = sorted({p for root in size_roots for p in root.rglob("*.rs")})
        for path in sized:
            rel = path.relative_to(ROOT).as_posix()
            lines = path.read_text(errors="replace").splitlines()
            if len(lines) > MAX_LINES:
                if is_legacy(rel, baseline):
                    legacy_count += 1
                else:
                    violations.append(f"{rel}: {len(lines)} lines (max {MAX_LINES})")
        for path in sorted(src.rglob("*.rs")) if src.exists() else []:
            src_rel = path.relative_to(ROOT).as_posix()
            src_lines = path.read_text(errors="replace").splitlines()
            for lineno, line in enumerate(src_lines, 1):
                if "forbid(unsafe_code)" in line or not _comment_free(line):
                    continue
                key = f"{src_rel}:{lineno}"
                if FORBIDDEN_RE.search(line):
                    if key in baseline.get("forbidden_patterns", {}):
                        legacy_count += 1
                    else:
                        violations.append(f"{key}: forbidden ownership/unsafe pattern")
                m = use_re.match(line)
                if m and m.group(1) in local_modules:
                    continue  # a module of this crate, not an external edge
                if m and m.group(1) in known:
                    msgs = violations_for_edge(name, m.group(1), allowed)
                    if msgs and m.group(1) not in legacy_edges.get(name, set()):
                        violations.append(f"{key}: {msgs[0]}")
                    elif msgs:
                        legacy_count += 1
                for dst in qualified_edges(line, known, local_modules):
                    msgs = violations_for_edge(name, dst, allowed)
                    if msgs and dst not in legacy_edges.get(name, set()):
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
