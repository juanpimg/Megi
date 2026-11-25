#!/usr/bin/env python3
"""Generate Megi's progress status from real repository data.

Produces, deterministically, from `docs/status/progress.toml` plus measured repository
statistics:

    PROGRESS.md                    project progress page (badges, map, stats, history)
    docs/status/progress.svg       visual progress map (light theme, 900 px wide)
    docs/status/badge-tests.svg    shields-like badges
    docs/status/badge-conformance.svg
    docs/status/badge-crates.svg
    docs/status/badge-unsafe.svg
    docs/status/badge-wpt.svg
    docs/status/progress.toml      manifest with its generated [stats]/[gate]/[wpt] blocks

Everything is Python 3 standard library (3.11+ for `tomllib`); no network, no third-party
services, no external fonts. The script is idempotent: running it twice writes byte-identical
files. Timestamps are **not** taken from the clock — the snapshot uses the date of the last
git commit (`git log -1 --format=%cs`), so regenerating an unchanged tree is a no-op.

Usage
-----
    python3 tools/update_progress.py --write            # regenerate everything
    python3 tools/update_progress.py --write --no-gate  # skip the conformance gate
    python3 tools/update_progress.py --write --no-count # reuse the cached test count
    python3 tools/update_progress.py --check --no-gate --no-count   # CI freshness check

The snapshot commit/date are inherently self-referential: the generated files cannot embed
the hash/date of the commit that will contain them. `--check` therefore **normalizes the
snapshot fields** (commit hash and commit date in the Markdown snapshot line/table, the SVG
`<desc>`/footer and the manifest `tests_commit`/`tests_date`/`verified_commit` entries) to
`<commit>`/`<date>` placeholders on **both** sides before comparing, so committing the
generated files keeps the freshness check green. Everything else (phases, items, stats,
LOC, test count, gate, history) is still compared byte for byte, so a code/test/manifest
change made without regenerating still fails. `--write` always writes the real HEAD/date.

`--check` renders in memory and compares bytes (after snapshot normalization); it exits 0 when the committed files are up
to date and 1 with a short diff otherwise. `--no-count` reuses `[stats].tests` from the
manifest (cache: `target/progress-stats.json`); `--no-gate` reuses `[gate]` from the
manifest. CI uses `--check --no-gate --no-count` (the conformance job already runs the gate).

Regenerating after changing the manifest:
    python3 tools/update_progress.py --write

Modularity (AGENTS.md §4): measurement lives in `tools/progress_stats.py` and artifact
rendering in `tools/progress_render.py`; this file is the CLI/orchestrator. Every public
function of both modules is re-exported here so callers and tests keep working unchanged.
"""

from __future__ import annotations

import argparse
import re
import sys
try:
    import tomllib
except ModuleNotFoundError:
    print("Python 3.11+ required (tomllib). Install Python 3.11 or newer.", file=sys.stderr)
    sys.exit(1)
from pathlib import Path

# Both entry points (``python3 tools/update_progress.py`` and ``import tools.update_progress``)
# must find the sibling WPT helpers without adding a package dependency.
TOOLS_DIR = Path(__file__).resolve().parent
if str(TOOLS_DIR) not in sys.path:
    sys.path.insert(0, str(TOOLS_DIR))
import wpt_gate  # noqa: E402
import wpt_report  # noqa: E402
# Re-exports: the public API of the generator stays in this module for existing callers.
from progress_render import (  # noqa: E402
    FONT,
    render_badge,
    render_badges,
    render_markdown,
    render_progress_svg,
    update_manifest,
)
from progress_stats import (  # noqa: E402
    STATUS_DIR,
    bar,
    comma,
    count_crates,
    count_rust_lines,
    count_tests,
    criteria_percent,
    git_commit_date,
    git_short_head,
    md_escape,
    milestone_counts,
    overall_percent,
    parse_rounds,
    phase_color,
    read_coverage,
    render_wpt_headline,
    run,
    unsafe_forbidden_crates,
    wpt_fields,
    xml_escape,
)

BADGE_FILES = {
    "tests": "badge-tests.svg",
    "conformance": "badge-conformance.svg",
    "crates": "badge-crates.svg",
    "unsafe": "badge-unsafe.svg",
    "wpt": "badge-wpt.svg",
}
GATE_SUITES = {
    "tokenizer": "Tokenizer Total",
    "tree": "Tree Construction Total",
    "serializer": "HTML Serializer Total",
    "css_syntax": "CSS Syntax Total",
}


def run_gate(root: Path, manifest_gate: dict[str, str]) -> dict[str, str]:
    """Runs `test_harness --summary` and parses the 4 suites + PASS/FAIL."""
    result = run(root, ["cargo", "run", "-q", "-p", "megi-test-harness", "--", "--summary"], timeout=900)
    output = result.stdout + result.stderr
    gate = {"status": "FAIL", "verified_commit": ""}
    # If any suite count cannot be parsed from the fresh output, the gate cannot be trusted:
    # do not copy counts forward from the manifest and force FAIL (#1392).
    all_parsed = True
    for key, prefix in GATE_SUITES.items():
        match = re.search(rf"{re.escape(prefix)}: (\d+) / (\d+)", output)
        if match:
            gate[key] = f"{match.group(1)} / {match.group(2)}"
        else:
            gate[key] = "n/a"
            all_parsed = False
    if all_parsed and result.returncode == 0 and "Conformance gate: PASS" in output:
        gate["status"] = "PASS"
    return gate


SNAPSHOT_PATTERNS = [
    # Markdown snapshot line and stats-table row (commit + date).
    (re.compile(r"(?m)^(\*\*Snapshot\*\* — commit )`[0-9a-f]{7,40}` \([0-9-]+\)"), r"\1`<commit>` (<date>)"),
    (re.compile(r"(?m)^(\| Commit \| )`[0-9a-f]{7,40}` \([0-9-]+\)( \|)"), r"\1`<commit>` (<date>)\2"),
    # Manifest generated fields.
    (re.compile(r'tests_commit = "[0-9a-f]{7,40}"'), 'tests_commit = "<commit>"'),
    (re.compile(r'tests_date = "[0-9-]+"'), 'tests_date = "<date>"'),
    (re.compile(r'verified_commit = "[0-9a-f]{7,40}"'), 'verified_commit = "<commit>"'),
    # SVG <desc> and footer.
    (re.compile(r"commit [0-9a-f]{7,40};"), "commit <commit>;"),
    (re.compile(r"commit [0-9a-f]{7,40} \([0-9-]+\)"), "commit <commit> (<date>)"),
]


def normalize_snapshot(text: str) -> str:
    """Replaces the self-referential snapshot commit/date with placeholders (`--check`)."""
    for pattern, replacement in SNAPSHOT_PATTERNS:
        text = pattern.sub(replacement, text)
    return text


def build_artifacts(root: Path, no_count: bool, no_gate: bool) -> dict[str, bytes]:
    manifest_path = root / STATUS_DIR / "progress.toml"
    try:
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        sys.exit(f"error: manifest not found: {manifest_path} (run from the repo root)")
    except tomllib.TOMLDecodeError as error:
        sys.exit(f"error: invalid TOML in {manifest_path}: {error}")

    head = git_short_head(root)
    date = git_commit_date(root)
    if head == "uncommitted" or date == "unknown":
        print("warning: git metadata unavailable; using placeholders", file=sys.stderr)

    manifest_stats = manifest.get("stats", {})
    try:
        manifest_tests = int(manifest_stats.get("tests", 0))
    except (TypeError, ValueError):
        sys.exit(
            f"error: invalid [stats].tests in {manifest_path}: "
            f"{manifest_stats.get('tests')!r}"
        )
    manifest_gate = manifest.get("gate", {})
    tests, tests_measured = count_tests(
        root,
        root / "target" / "progress-stats.json",
        head,
        manifest_tests,
        no_count,
    )
    gate = run_gate(root, manifest_gate) if not no_gate else dict(manifest_gate)
    # Same rule as the conformance gate: measure on --write, reuse the manifest on --no-gate.
    if no_gate:
        # Cheap and offline: derive [wpt] from the committed report rather than trusting a
        # possibly stale manifest block (the P/T must always match docs/status/wpt.json).
        try:
            wpt = wpt_fields(wpt_report.load_report(root / STATUS_DIR / "wpt.json"))
        except (OSError, ValueError) as failure:
            sys.exit(f"error: WPT report: {failure}")
    else:
        try:
            report = wpt_report.build_report(root)
            error = wpt_gate.check_gate(
                report,
                wpt_gate.load_baseline(root / "tests/wpt/baseline.json"),
                wpt_gate.load_allowlist(root / "tests/wpt/allowlist.toml"),
            )
        except (OSError, ValueError) as failure:
            sys.exit(f"error: WPT report/gate: {failure}")
        if error:
            sys.exit(f"error: WPT gate: {error}")
        wpt_report.write_report(root, report)
        wpt = wpt_fields(report)
    unsafe = unsafe_forbidden_crates(root)
    # Provenance: only claim the current commit for the test count / conformance gate when
    # they were actually measured here; with --no-count/--no-gate (or a failed count) keep
    # the previous stamps instead of pretending the cached result belongs to HEAD.
    if tests_measured:
        tests_commit = head
        tests_date = date
    else:
        tests_commit = manifest_stats.get("tests_commit", head)
        tests_date = manifest_stats.get("tests_date", date)
    if no_gate:
        verified_commit = manifest_gate.get("verified_commit", head)
    else:
        verified_commit = head
    # Stamp the gate with the commit that actually measured it so `update_coverage.py` can
    # reject stale gate evidence (#1392). `run_gate` leaves this empty; fill it here.
    gate["verified_commit"] = verified_commit
    stats = {
        "head": head,
        "date": date,
        "tests_commit": tests_commit,
        "tests_date": tests_date,
        "crates": count_crates(root),
        "rust_lines": count_rust_lines(root),
        "tests": tests,
        "unsafe_forbidden": len(unsafe),
    }
    data = {
        "stats": stats,
        "gate": gate,
        "phases": manifest.get("phase", []),
        "areas": manifest.get("area", []),
        "next_up": manifest.get("next_up", []),
        "rounds": parse_rounds(root / "CHANGELOG.md"),
        "last_updated_by": manifest.get("last_updated_by", "unknown"),
        "subtitle": manifest.get("subtitle", ""),
        "coverage": read_coverage(root),
        "wpt": wpt,
    }

    artifacts: dict[str, bytes] = {}
    artifacts["PROGRESS.md"] = render_markdown(data).encode("utf-8")
    artifacts[str(STATUS_DIR / "progress.svg")] = render_progress_svg(data).encode("utf-8")
    badges = render_badges(data)
    for key, filename in BADGE_FILES.items():
        if key in badges:
            artifacts[str(STATUS_DIR / filename)] = badges[key].encode("utf-8")
    artifacts[str(manifest_path.relative_to(root))] = update_manifest(
        manifest_path.read_text(encoding="utf-8"), stats, gate, wpt
    ).encode("utf-8")
    data["stats"]["rust_lines"] = comma(stats["rust_lines"])  # only for logging
    return artifacts, stats, gate


def print_stats(root: Path, artifacts: dict[str, bytes], stats: dict, gate: dict) -> None:
    print(
        f"progress: commit {git_short_head(root)} ({git_commit_date(root)}) · "
        f"tests {stats['tests']} · gate {gate['status']} · "
        f"crates {stats['crates']} · "
        f"unsafe-forbidden {stats['unsafe_forbidden']}/{stats['crates']} · "
        f"Rust lines {stats['rust_lines']}"
    )


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Generate Megi's progress status page (stdlib only).")
    parser.add_argument("--write", action="store_true", help="regenerate artifacts (default)")
    parser.add_argument("--check", action="store_true", help="verify artifacts are up to date; exit 1 if stale")
    parser.add_argument("--no-count", action="store_true", help="reuse the manifest test count instead of running cargo")
    parser.add_argument("--no-gate", action="store_true", help="reuse the manifest conformance gate instead of running it")
    parser.add_argument("--root", default=None, help="repository root (default: parent of tools/)")
    args = parser.parse_args(argv)
    if args.write and args.check:
        parser.error("--write and --check are mutually exclusive")

    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parent.parent
    artifacts, stats, gate = build_artifacts(root, no_count=args.no_count, no_gate=args.no_gate)

    if args.check:
        stale: list[str] = []
        for relative, content in sorted(artifacts.items()):
            path = root / relative
            current = path.read_bytes() if path.is_file() else b""
            current_text = normalize_snapshot(current.decode("utf-8", "replace"))
            new_text = normalize_snapshot(content.decode("utf-8", "replace"))
            if current_text != new_text:
                stale.append(relative)
                if current:
                    current_lines = current_text.splitlines()
                    new_lines = new_text.splitlines()
                    for number, (old, new) in enumerate(zip(current_lines, new_lines), start=1):
                        if old != new:
                            print(f"stale: {relative}: first difference at line {number}:")
                            print(f"  - {old}")
                            print(f"  + {new}")
                            break
                    else:
                        print(f"stale: {relative}: file length differs")
                else:
                    print(f"stale: {relative}: missing")
        if stale:
            print("\nRun `python3 tools/update_progress.py --write` and commit the generated files.")
            return 1
        print("progress status is up to date")
        return 0

    written = []
    for relative, content in sorted(artifacts.items()):
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        if not path.is_file() or path.read_bytes() != content:
            path.write_bytes(content)
            written.append(relative)
    print_stats(root, artifacts, stats, gate)
    if written:
        print(f"wrote {len(written)} file(s): {', '.join(written)}")
    else:
        print("artifacts already up to date (nothing written)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
