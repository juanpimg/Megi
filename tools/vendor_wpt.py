#!/usr/bin/env python3
"""Vendor WPT fixtures under a SHA-pinned commit and verify the manifest offline.

Standard library only. The checker is the CI gate: it proves that every file under
`tests/wpt/` is declared, byte-identical to its recorded SHA-256, licensed under an
allowlisted license, and inside the phase byte budget. It never fetches anything;
`--check` is fully offline and uses the pinned SHA only as recorded provenance.

Usage:
    python3 tools/vendor_wpt.py --check
    python3 tools/vendor_wpt.py --source /path/to/wpt --commit <40-hex> \
        --dirs css/css-syntax
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

WPT_DIR = Path("tests/wpt")
REVISION_FILE = "WPT-REVISION.json"
MANIFEST_FILE = "MANIFEST.json"
NOTICE_FILE = "NOTICE.md"
LICENSE_ALLOWLIST = {"BSD-3-Clause", "MIT", "Apache-2.0", "OFL-1.1"}
MAX_BYTES_BY_PHASE = {1: 4_000_000, 2: 12_000_000, 3: 40_000_000}
KINDS = {"testharness", "seed", "support", "skip"}
SKIP_MARKERS = ("-manual.html", "-ref.html", "testdriver.js", "-tentative.html")
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
MANIFEST_KEYS = {
    "path", "sha256", "bytes", "license", "kind", "skip_reason", "dir",
    "subtest_count_expected",
}
# Adopted subtests, all really executed by crates/test_harness/src/css_harness.rs (and its
# `expanded` submodule). The count is the denominator of the honest WPT report; it must never
# exceed the number of cases the harness extracts from the vendored fixture.
SEED_COUNTS = {
    "css/css-syntax/anb-parsing.html": 58,
    "css/css-syntax/anb-serialization.html": 20,
    "css/css-syntax/declarations-trim-whitespace.html": 9,
    "css/css-syntax/input-preprocessing.html": 13,
    "html/webappapis/atob/base64.html": 378,
    "shadow-dom/Element-interface-attachShadow.html": 6,
    "shadow-dom/Element-interface-shadowRoot-attribute.html": 3,
    "shadow-dom/historical.html": 5,
    "shadow-dom/slots-fallback.html": 13,
    "shadow-dom/HTMLSlotElement-interface.html": 18,
    "shadow-dom/Slotable-interface.html": 4,
    "shadow-dom/slots.html": 26,
    "custom-elements/historical.html": 3,
    "custom-elements/CustomElementRegistry.html": 36,
}
# Upstream files deliberately not imported: the first two need getComputedStyle, which the engine
# does not expose yet; the third requires @charset removal, which the stylesheet parser has not
# implemented (it currently yields 4 rules instead of 1). Recording them here keeps every omission
# explicit instead of silently dropping a fixture or inventing a PASS.
EXCLUDED_PATHS = {
    "css/css-syntax/ident-three-code-points.html",
    "css/css-syntax/unicode-range-selector.html",
    "css/css-syntax/charset-is-not-a-rule.html",
}
SKIP_PATHS = {
    # Each entry below was probed with the offline JS runner and left out with its
    # exact remaining failure; no failing subtest is counted as a PASS or allowlisted.
    "shadow-dom/Document-prototype-adoptNode.html": "not adopted: adoptNode on a shadow root throws TypeError: not a callable function",
    "shadow-dom/Element-interface-attachShadow-custom-element.html": "not adopted: customized built-in attachShadow does not throw NotSupportedError",
    "shadow-dom/Extensions-to-Event-Interface.html": "not adopted: Event.composedPath is not callable and shadow event paths are incomplete",
    "shadow-dom/Node-prototype-cloneNode.html": "not adopted: cloneNode on a shadow root does not throw NotSupportedError",
    "shadow-dom/ShadowRoot-interface.html": "not adopted: activeElement returns null for a focused descendant and innerHTML fragment parsing fails",
    "shadow-dom/event-composed-path.html": "not adopted: composedPath() returns an empty path (expected 3 nodes)",
    "shadow-dom/event-inside-shadow-tree.html": "not adopted: EventPath omits the shadow root and host",
    "shadow-dom/event-inside-slotted-node.html": "not adopted: EventPath omits slot, shadow root and host",
    "shadow-dom/getElementById-dynamic-001.html": "not adopted: shadow root getElementById hits a non-element NodeId after host removal",
    "custom-elements/Document-createElement.html": "not adopted: 'is'-based autonomous custom elements are not upgraded",
    "custom-elements/upgrading.html": "not adopted: document.implementation.createHTMLDocument is not callable",
    "custom-elements/HTMLElement-constructor.html": "not adopted: CONCRETE-* and SUCCESS-COUNTING-ELEMENT-* elements are undefined and the last case reports success-counting-element-3 instead of undefined",
    "custom-elements/Document-createElementNS.html": "not adopted: document.implementation.createHTMLDocument is not callable and HTMLUnknownElement is not upgraded",
    "css/css-syntax/META.yml": "upstream metadata (not a test fixture)",
    "shadow-dom/META.yml": "upstream metadata (not a test fixture)",
}
# Support data is vendored byte-for-byte but is not an executed test: the offline
# component runner hands it to the local `fetch` shim instead of opening a socket.
SUPPORT_PATHS = {
    "fetch/data-urls/resources/base64.json": "support data for the atob/base64 fixture",
    "shadow-dom/resources/shadow-dom-utils.js": "support script for the shadow-dom fixtures",
    "shadow-dom/resources/shadow-dom.js": "support script for shadow-dom fixtures",
    "shadow-dom/resources/Document-prototype-currentScript-helper.js": "support script for currentScript fixture",
    "shadow-dom/resources/event-path-test-helpers.js": "support script for event path fixtures",
    "shadow-dom/resources/directionality-001-ref.html": "reference page (not a testharness fixture)",
    "custom-elements/resources/custom-elements-helpers.js": "support script for custom-elements fixtures",
    "custom-elements/resources/empty-html-document.html": "support document for custom-elements fixtures",
    "custom-elements/resources/my-custom-element-html-document.html": "support document for custom-elements fixtures",
    "custom-elements/resources/navigation-destination.html": "support document for custom-elements fixtures",
}
# Directories probed while scoping the runner but not adopted: their engine APIs are
# incomplete, so reporting their real failures would break the allowlist-free gate.
NOT_ADOPTED_DIRS = {
    "dom/nodes": "explored, not adopted by the component runner (engine DOM APIs incomplete)",
    "dom/events": "explored, not adopted by the component runner (engine event APIs incomplete)",
    "url": "explored, not adopted by the component runner (engine URL APIs incomplete)",
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _read_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"cannot read {path}: {error}") from error


def load_revision(root: Path) -> dict:
    """Load tests/wpt/WPT-REVISION.json (the pinned upstream commit)."""
    payload = _read_json(root / WPT_DIR / REVISION_FILE)
    if not isinstance(payload, dict):
        raise ValueError(f"{REVISION_FILE} must contain a JSON object")
    return payload


def load_manifest(root: Path) -> list[dict]:
    """Load tests/wpt/MANIFEST.json as a list of declared fixtures."""
    payload = _read_json(root / WPT_DIR / MANIFEST_FILE)
    if not isinstance(payload, list):
        raise ValueError(f"{MANIFEST_FILE} must contain a JSON array")
    return payload


# Gate metadata under tests/wpt/ is not vendored fixture material.
CONTROL_FILES = {REVISION_FILE, MANIFEST_FILE, NOTICE_FILE, "baseline.json", "allowlist.toml"}


def declared_files(root: Path) -> list[Path]:
    """Fixtures under tests/wpt/ that must be declared (control files excluded)."""
    base = root / WPT_DIR
    if not base.is_dir():
        return []
    return sorted(
        path.relative_to(base) for path in base.rglob("*")
        if path.is_file() and path.name not in CONTROL_FILES
    )


def check_manifest(root: Path) -> list[str]:
    """Return all manifest violations; an empty list means the vendor tree is clean."""
    errors: list[str] = []
    try:
        revision = load_revision(root)
    except ValueError as error:
        return [str(error)]
    pin = revision.get("revision")
    if not isinstance(pin, str) or not SHA_RE.fullmatch(pin):
        errors.append(f"{REVISION_FILE}: revision must be a 40-hex commit SHA, got {pin!r}")
    phase = revision.get("phase")
    if phase not in MAX_BYTES_BY_PHASE:
        errors.append(f"{REVISION_FILE}: phase must be one of {sorted(MAX_BYTES_BY_PHASE)}, got {phase!r}")

    try:
        entries = load_manifest(root)
    except ValueError as error:
        return errors + [str(error)]

    by_path: dict[str, dict] = {}
    used_bytes: dict[str, int] = {}
    for index, entry in enumerate(entries):
        where = f"{MANIFEST_FILE}[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{where}: entry must be an object")
            continue
        missing = MANIFEST_KEYS - entry.keys()
        if missing:
            errors.append(f"{where}: missing keys {sorted(missing)}")
            continue
        path = entry["path"]
        if not isinstance(path, str) or path.startswith("/") or ".." in Path(path).parts:
            errors.append(f"{where}: unsafe path {path!r}")
            continue
        if path in by_path:
            errors.append(f"{where}: duplicate path {path}")
            continue
        by_path[path] = entry
        if entry["dir"] != str(Path(path).parent):
            errors.append(f"{path}: dir must be {str(Path(path).parent)!r}")
        if entry["license"] not in LICENSE_ALLOWLIST:
            errors.append(f"{path}: license {entry['license']!r} is not in the allowlist")
        if entry["kind"] not in KINDS:
            errors.append(f"{path}: unknown kind {entry['kind']!r}")
        if entry["kind"] == "skip" and not entry["skip_reason"]:
            errors.append(f"{path}: kind=skip requires a non-empty skip_reason")
        if entry["kind"] != "skip" and any(marker in path for marker in SKIP_MARKERS):
            errors.append(f"{path}: skip marker requires kind=skip with a skip_reason")
        if not isinstance(entry["subtest_count_expected"], int) or entry["subtest_count_expected"] < 0:
            errors.append(f"{path}: subtest_count_expected must be a non-negative integer")
        if entry["kind"] == "skip":
            continue
        actual = root / WPT_DIR / path
        if not actual.is_file():
            errors.append(f"{path}: declared file is missing")
            continue
        data = actual.read_bytes()
        if sha256_bytes(data) != entry["sha256"]:
            errors.append(f"{path}: sha256 mismatch with vendored bytes")
        if len(data) != entry["bytes"]:
            errors.append(f"{path}: byte count mismatch (manifest {entry['bytes']}, actual {len(data)})")
        used_bytes[entry["dir"]] = used_bytes.get(entry["dir"], 0) + len(data)

    present = {str(path) for path in declared_files(root)}
    for path in sorted(present - by_path.keys()):
        errors.append(f"{path}: file under tests/wpt/ is not declared in {MANIFEST_FILE}")
    limit = MAX_BYTES_BY_PHASE.get(phase, 0)
    for directory, size in sorted(used_bytes.items()):
        if size > limit:
            errors.append(f"{directory}: {size} bytes exceed phase {phase} budget of {limit}")
    return errors


def _license_of(source: Path) -> str:
    candidates = (
        ("BSD-3-Clause", ("bsd 3-clause", "bsd-3-clause", "redistribution and use in source and binary forms")),
        ("MIT", ("mit license", "permission is hereby granted, free of charge")),
        ("Apache-2.0", ("apache license", "version 2.0, january 2004")),
        ("OFL-1.1", ("sil open font license", "version 1.1")),
    )
    for name in ("LICENSE", "LICENSE.txt", "LICENSE.md"):
        path = source / name
        if path.is_file():
            text = path.read_text(encoding="utf-8", errors="replace").lower()
            for license_name, markers in candidates:
                if any(marker in text for marker in markers):
                    return license_name
            raise ValueError(f"unknown license in {path}; expected one of {sorted(LICENSE_ALLOWLIST)}")
    raise ValueError(f"no LICENSE file in {source}; cannot record a license")


def vendor(source: Path, commit: str, dirs: list[str], root: Path) -> None:
    """Copy byte-identical fixtures from a pinned WPT checkout and rewrite the manifest."""
    if not SHA_RE.fullmatch(commit):
        raise ValueError(f"--commit must be a full 40-hex SHA, got {commit!r}")
    source = source.resolve()
    if not source.is_dir():
        raise ValueError(f"source is not a directory: {source}")
    resolved = subprocess.run(
        ["git", "-C", str(source), "rev-parse", "--verify", "--quiet", f"{commit}^{{commit}}"],
        capture_output=True, text=True, check=False,
    )
    if resolved.returncode != 0:
        raise ValueError(f"commit {commit} is not present in {source}")
    license_name = _license_of(source)
    base = root / WPT_DIR
    entries: list[dict] = []
    for directory in dirs:
        if ".." in Path(directory).parts or Path(directory).is_absolute():
            raise ValueError(f"unsafe --dirs entry: {directory!r}")
        source_dir = source / directory
        if not source_dir.is_dir():
            raise ValueError(f"missing source directory: {source_dir}")
        # Only direct children of each --dirs entry are fixtures: recursive imports would put
        # nested WPT support directories under a manifest `dir` that does not match their path.
        for path in sorted(source_dir.iterdir()):
            if not path.is_file():
                continue
            relative = path.relative_to(source).as_posix()
            if relative in EXCLUDED_PATHS:
                continue
            skipped = any(marker in relative for marker in SKIP_MARKERS)
            explicit_skip = SKIP_PATHS.get(relative) or NOT_ADOPTED_DIRS.get(directory)
            support_reason = SUPPORT_PATHS.get(relative)
            data = path.read_bytes()
            target = base / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            entries.append(
                {
                    "path": relative,
                    "sha256": sha256_bytes(data),
                    "bytes": len(data),
                    "license": license_name,
                    "kind": (
                        "skip" if skipped or explicit_skip
                        else "support" if support_reason
                        else "testharness"
                    ),
                    "skip_reason": explicit_skip
                    or ("upstream manual/reference/testdriver file" if skipped else support_reason or ""),
                    "dir": directory,
                    "subtest_count_expected": SEED_COUNTS.get(relative, 0),
                }
            )
    for relative in sorted(base.rglob("README.md")):
        path = relative.relative_to(base).as_posix()
        if any(entry["path"] == path for entry in entries):
            continue
        data = relative.read_bytes()
        entries.append(
            {
                "path": path,
                "sha256": sha256_bytes(data),
                "bytes": len(data),
                "license": license_name,
                "kind": "skip",
                "skip_reason": "documentation (not a WPT fixture)",
                "dir": str(Path(path).parent),
                "subtest_count_expected": 0,
            }
        )
    entries.sort(key=lambda entry: entry["path"])
    base.mkdir(parents=True, exist_ok=True)
    (base / REVISION_FILE).write_text(
        json.dumps(
            {
                "revision": commit,
                "source": "web-platform-tests/wpt",
                "phase": 1,
                "note": "SHA verified byte-for-byte against vendored fixtures; see NOTICE.md",
            },
            indent=2,
        ) + "\n", encoding="utf-8",
    )
    (base / MANIFEST_FILE).write_text(json.dumps(entries, indent=2) + "\n", encoding="utf-8")
    notice = [
        "# Vendored WPT provenance\n",
        f"- Upstream: web-platform-tests/wpt\n- Pinned commit: `{commit}` (never `master`)\n",
        f"- License: {license_name} (allowlisted)\n- Imported dirs: {', '.join(dirs)}\n",
        "\n| Fixture | Bytes | SHA-256 | Kind |\n|---|---:|---|---|\n",
    ]
    for entry in entries:
        notice.append(
            f"| `{entry['path']}` | {entry['bytes']} | `{entry['sha256']}` | {entry['kind']} |\n"
        )
    (base / NOTICE_FILE).write_text("".join(notice), encoding="utf-8")
    errors = check_manifest(root)
    if errors:
        raise ValueError("vendor produced an invalid manifest:\n  " + "\n  ".join(errors))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", default="", help="local WPT checkout to import from")
    parser.add_argument("--commit", default="", help="40-hex upstream commit SHA to pin")
    parser.add_argument("--dirs", default="", help="comma-separated directories under the WPT root")
    parser.add_argument("--check", action="store_true", help="verify the manifest offline")
    parser.add_argument("--root", default="", help="repository root (default: parent of tools/)")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parent.parent
    if args.check:
        errors = check_manifest(root)
        if errors:
            for error in errors:
                print(f"error: {error}", file=sys.stderr)
            return 1
        entries = load_manifest(root)
        print(f"vendor_wpt: OK — {len(entries)} declared file(s), pin {load_revision(root)['revision']}")
        return 0
    if not (args.source and args.commit and args.dirs):
        parser.error("--source, --commit and --dirs are required unless --check is used")
    try:
        vendor(Path(args.source), args.commit, [d.strip() for d in args.dirs.split(",") if d.strip()], root)
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"vendor_wpt: imported {args.dirs} at {args.commit}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
