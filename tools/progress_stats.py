#!/usr/bin/env python3
"""Repository measurement and small statistics helpers for the progress generator.

This module was split out of ``tools/update_progress.py`` to keep that CLI/
orchestrator under the AGENTS.md §4 file-size cap. It holds only pure, stdlib-only
helpers: git/cargo measurement, manifest parsing and the elementary formatting
functions shared with the renderers. It deliberately has no imports from
``progress_render`` or ``update_progress`` so the DAG stays one-directional:

    progress_stats  <-  progress_render  <-  update_progress

Every function is re-exported from ``tools/update_progress.py`` so existing
callers (``from tools.update_progress import bar``, tests, CI) keep working.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from xml.sax.saxutils import escape as _xml_escape

STATUS_DIR = Path("docs/status")


def xml_escape(text: str) -> str:
    return _xml_escape(str(text), {"\"": "&quot;", "'": "&apos;"})


def run(root: Path, args: list[str], timeout: int = 600) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            args,
            cwd=root,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired:
        print(f"warning: command timed out after {timeout}s: {' '.join(args)}", file=sys.stderr)
        return subprocess.CompletedProcess(args, 1, "", "timeout")


def git_short_head(root: Path) -> str:
    result = run(root, ["git", "rev-parse", "--short", "HEAD"], 30)
    return result.stdout.strip() or "uncommitted"


def git_commit_date(root: Path) -> str:
    result = run(root, ["git", "log", "-1", "--format=%cs"], 30)
    return result.stdout.strip() or "unknown"


def count_crates(root: Path) -> int:
    return sum(1 for path in sorted((root / "crates").glob("*/Cargo.toml")) if path.is_file())


def count_rust_lines(root: Path) -> int:
    result = run(root, ["git", "ls-files", "*.rs"], 60)
    paths = [root / line for line in result.stdout.splitlines() if line.strip()]
    if not paths:
        paths = sorted(root.glob("crates/**/*.rs"))
    total = 0
    for path in paths:
        try:
            total += len(path.read_text(encoding="utf-8", errors="ignore").splitlines())
        except OSError:
            continue
    return total


def unsafe_forbidden_crates(root: Path) -> list[str]:
    """Returns the crates whose root module forbids unsafe code.

    Binary-only crates (`bench`) carry the attribute in `main.rs`, so both roots are
    checked; the first file that exists wins.
    """
    # Accept forbid(unsafe_code) alone or as one term among several (e.g.
    # forbid(unsafe_code, warnings)) (#601).
    pattern = re.compile(r"#!\s*\[\s*forbid\s*\([^)]*unsafe_code[^)]*\)\s*\]")
    crates = []
    for cargo in sorted((root / "crates").glob("*/Cargo.toml")):
        forbidden = False
        for root_module in ("lib.rs", "main.rs"):
            module = cargo.parent / "src" / root_module
            if not module.is_file():
                continue
            text = module.read_text(encoding="utf-8", errors="ignore")
            if pattern.search(text):
                forbidden = True
        if forbidden:
            crates.append(cargo.parent.name)
    return crates


def count_tests(
    root: Path, cache_path: Path, head: str, manifest_tests: int, no_count: bool
) -> tuple[int, bool]:
    """Counts workspace tests, caching by HEAD.

    Returns `(count, measured)`: `measured` is `False` when the value comes from the
    manifest (`--no-count`) or from the fallback after a failed `cargo test --list`, so the
    caller can avoid claiming the count was verified at `HEAD`.
    """
    if no_count:
        return manifest_tests, False
    if head == "uncommitted":
        # Never cache under the sentinel head.
        result = run(root, ["cargo", "test", "--workspace", "--", "--list"], timeout=900)
        count = sum(1 for line in result.stdout.splitlines() if line.rstrip().endswith(": test"))
        if count == 0:
            print("warning: `cargo test --workspace -- --list` returned no tests; using manifest value", file=sys.stderr)
            return manifest_tests, False
        return count, True
    # A cached count is only valid for a clean tree at this HEAD (#1394): an
    # uncommitted test addition/removal would otherwise be stamped with a count
    # that never existed on any pushed commit.
    tree_dirty = bool(run(root, ["git", "status", "--porcelain"]).stdout.strip())
    if cache_path.is_file() and not tree_dirty:
        try:
            cache = json.loads(cache_path.read_text(encoding="utf-8"))
            if cache.get("head") == head and isinstance(cache.get("tests"), int):
                return int(cache["tests"]), True
        except (OSError, ValueError):
            pass
    result = run(root, ["cargo", "test", "--workspace", "--", "--list"], timeout=900)
    count = sum(1 for line in result.stdout.splitlines() if line.rstrip().endswith(": test"))
    if count == 0:
        print("warning: `cargo test --workspace -- --list` returned no tests; using manifest value", file=sys.stderr)
        return manifest_tests, False
    if not tree_dirty:
        cache_path.parent.mkdir(parents=True, exist_ok=True)
        cache_path.write_text(json.dumps({"head": head, "tests": count}, indent=2) + "\n", encoding="utf-8")
    return count, True


def parse_rounds(changelog: Path, limit: int = 6) -> list[tuple[str, str]]:
    rounds: list[tuple[str, str]] = []
    if not changelog.is_file():
        return rounds
    for line in changelog.read_text(encoding="utf-8", errors="ignore").splitlines():
        match = re.match(r"^### Round (\d+) — (.+?)(?: \(\d{4}-\d{2}-\d{2}\))?\s*$", line)
        if match:
            rounds.append((match.group(1), match.group(2).strip()))
        if len(rounds) >= limit:
            break
    return rounds


def bar(percent: int, width: int = 20) -> str:
    """Renders the bar, which only fills completely at 100 %.

    Rounding used to give 99 % a full 20/20 bar, making an incomplete phase look done; floor
    keeps 98-99 % visibly short of the last block.
    """
    filled = int(percent * width // 100)
    if percent < 100:
        filled = min(filled, width - 1)
    filled = max(0, min(width, filled))
    return "█" * filled + "░" * (width - filled)


# #2441: a bare sentence moved into `items` is documentation, not verification.
# Accept a citation that can be inspected (a command or artifact in backticks) or an
# explicit gate result; both are checkable when the progress manifest is reviewed.
EVIDENCE_RE = re.compile(
    r"`[^`]+`"
    r"|\b(?:PASS|OK|verified|EXIT\s*=\s*0|sha256|md5|byte-idéntico)\b"
    r"|\b\d+/\d+\b"
)


def as_criterion_list(value: object) -> list:
    """Return a criterion list, rejecting malformed TOML values as empty."""
    return value if isinstance(value, list) else []


def has_evidence(item: object) -> bool:
    """True when a criterion records a checkable citation or gate result."""
    return isinstance(item, str) and bool(EVIDENCE_RE.search(item))


def verified_criteria(phase: dict) -> list:
    """Return only the criteria carrying checkable evidence, not bare prose."""
    return [item for item in as_criterion_list(phase.get("items")) if has_evidence(item)]


def criterion_counts(phase: dict) -> tuple[int, int]:
    """(verified criteria, all criteria) for one phase."""
    items = as_criterion_list(phase.get("items"))
    pending = as_criterion_list(phase.get("pending"))
    return len(verified_criteria(phase)), len(items) + len(pending)


def overall_counts(phases: list[dict]) -> tuple[int, int]:
    """(verified criteria, all criteria) across all phases."""
    counts = [criterion_counts(phase) for phase in phases]
    return sum(met for met, _total in counts), sum(total for _met, total in counts)


def criteria_percent(phase: dict) -> int:
    """Percentage of a milestone's acceptance criteria that are verified.

    Only criteria with a checkable citation or gate result count as verified;
    documented-only text stays in the denominator so the bar cannot claim progress.
    """
    met, total = criterion_counts(phase)
    return 100 * met // total if total else 0


def overall_percent(phases: list[dict]) -> int:
    """Verified acceptance criteria / all criteria across M0–M12 (no estimates)."""
    met, total = overall_counts(phases)
    return 100 * met // total if total else 0


def milestone_counts(phases: list[dict]) -> tuple[int, int]:
    """(milestones closed, milestones in the roadmap)."""
    closed = sum(
        1
        for phase in phases
        if not as_criterion_list(phase.get("pending"))
        and as_criterion_list(phase.get("items"))
        and len(verified_criteria(phase)) == len(as_criterion_list(phase.get("items")))
    )
    return closed, len(phases)


def phase_color(percent: int) -> str:
    if percent >= 95:
        return "#2ea043"
    if percent >= 85:
        return "#1f6feb"
    if percent >= 70:
        return "#d29922"
    return "#da3633"


def comma(value: int) -> str:
    return f"{value:,}"


def md_escape(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ").replace("\r", " ")


def read_coverage(root: Path) -> dict | None:
    """Read the verifiable capability coverage produced by tools/update_coverage.py.

    Returns None when the artifact is missing or malformed; callers then fall back to the
    legacy phase average and label it as such instead of inventing a coverage number.
    """
    path = root / STATUS_DIR / "coverage.json"
    if not path.is_file():
        return None
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
        overall = payload["overall"]
        percent = int(overall["percent"])
        implemented = int(overall["implemented"])
        known = int(overall["known"])
        if known < 0 or implemented < 0 or implemented > known or not 0 <= percent <= 100:
            raise ValueError("inconsistent coverage numbers")
        expected = 0 if known == 0 else (100 if implemented >= known else min(99, round(implemented * 100 / known)))
        if percent != expected:
            raise ValueError("coverage percent does not match implemented/known")
        return {
            "percent": percent,
            "implemented": implemented,
            "known": known,
            "partial": int(overall.get("partial", 0)),
        }
    except (OSError, ValueError, KeyError, TypeError):
        return None


def render_wpt_headline(report: dict, epoch: dict | None = None) -> str:
    """Plan §4.5 headline: honest P/T + dirs + full non-zero distribution."""
    epoch = epoch or report.get("epoch", {})
    revision = str(epoch.get("wpt_revision", "unknown"))[:7]
    counts = report["counts"]
    run_info = report.get("run_info") or {}
    dirs = (run_info.get("dirs") or []) or sorted(
        {str(Path(r["test"]).parent) for r in report["results"]}
    )
    details = [
        f"{counts[field]} {label}"
        for field, label in (("fail", "FAIL"), ("timeout", "TIMEOUT"),
                             ("notrun", "NOTRUN"), ("error", "harness_error"))
        if counts.get(field, 0)
    ]
    headline = f"WPT {revision} — {counts['pass']}/{report['total']} subtests pasados"
    headline += f" (dirs: {len(dirs)}"
    if details:
        headline += "; " + ", ".join(details)
    return headline + ")"


def wpt_fields(report: dict) -> dict:
    """The manifest/[wpt] projection of a generated WPT report."""
    counts = report["counts"]
    run_info = report.get("run_info") or {}
    return {
        "revision": report["epoch"]["wpt_revision"],
        "manifest_sha256": report["epoch"]["manifest_sha256"],
        **counts,
        "passed": counts["pass"],
        "total": report["total"],
        "dirs": len(run_info.get("dirs") or []),
        "allowlist": 0,
        "distribution": "; ".join(
            f"{label}={counts[field]}" for field, label in (
                ("pass", "PASS"), ("fail", "FAIL"), ("timeout", "TIMEOUT"),
                ("notrun", "NOTRUN"), ("error", "harness_error"),
            )
        ),
        "headline": render_wpt_headline(report, report["epoch"]),
        "status": "PASS",
    }
