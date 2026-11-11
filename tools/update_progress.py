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
    docs/status/progress.toml      manifest with its generated [stats]/[gate] blocks

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
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path
from xml.sax.saxutils import escape as xml_escape

STATUS_DIR = Path("docs/status")
BADGE_FILES = {
    "tests": "badge-tests.svg",
    "conformance": "badge-conformance.svg",
    "crates": "badge-crates.svg",
    "unsafe": "badge-unsafe.svg",
}
GATE_SUITES = {
    "tokenizer": "Tokenizer Total",
    "tree": "Tree Construction Total",
    "serializer": "HTML Serializer Total",
    "css_syntax": "CSS Syntax Total",
}
FONT = "DejaVu Sans, Verdana, sans-serif"


def run(root: Path, args: list[str], timeout: int = 600) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=root,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )


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
    pattern = re.compile(r"#!\s*\[\s*forbid\s*\(\s*unsafe_code\s*\)\s*\]")
    crates = []
    for cargo in sorted((root / "crates").glob("*/Cargo.toml")):
        for root_module in ("lib.rs", "main.rs"):
            module = cargo.parent / "src" / root_module
            if not module.is_file():
                continue
            text = module.read_text(encoding="utf-8", errors="ignore")
            if pattern.search(text):
                crates.append(cargo.parent.name)
            break
    return crates


def count_tests(root: Path, cache_path: Path, head: str, manifest_tests: int, no_count: bool) -> int:
    """Counts workspace tests, caching by HEAD; `--no-count` reuses the manifest value."""
    if no_count:
        return manifest_tests
    if cache_path.is_file():
        try:
            cache = json.loads(cache_path.read_text(encoding="utf-8"))
            if cache.get("head") == head and isinstance(cache.get("tests"), int):
                return int(cache["tests"])
        except (OSError, ValueError):
            pass
    result = run(root, ["cargo", "test", "--workspace", "--", "--list"], timeout=900)
    count = sum(1 for line in result.stdout.splitlines() if line.rstrip().endswith(": test"))
    if count == 0:
        print("warning: `cargo test --workspace -- --list` returned no tests; using manifest value", file=sys.stderr)
        return manifest_tests
    cache_path.parent.mkdir(parents=True, exist_ok=True)
    cache_path.write_text(json.dumps({"head": head, "tests": count}, indent=2) + "\n", encoding="utf-8")
    return count


def run_gate(root: Path, manifest_gate: dict[str, str]) -> dict[str, str]:
    """Runs `test_harness --summary` and parses the 4 suites + PASS/FAIL."""
    result = run(root, ["cargo", "run", "-q", "-p", "test_harness", "--", "--summary"], timeout=900)
    output = result.stdout + result.stderr
    gate = {"status": "FAIL", "verified_commit": ""}
    for key, prefix in GATE_SUITES.items():
        match = re.search(rf"{re.escape(prefix)}: (\d+) / (\d+)", output)
        if match:
            gate[key] = f"{match.group(1)} / {match.group(2)}"
        else:
            gate[key] = manifest_gate.get(key, "n/a")
    if result.returncode == 0 and "Conformance gate: PASS" in output:
        gate["status"] = "PASS"
    return gate


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
    filled = max(0, min(width, round(percent * width / 100)))
    return "█" * filled + "░" * (width - filled)


def overall_percent(phases: list[dict]) -> int:
    if not phases:
        return 0
    return round(sum(int(p.get("percent", 0)) for p in phases) / len(phases))


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
    return text.replace("|", "\\|")


def render_markdown(data: dict) -> str:
    stats, gate, phases = data["stats"], data["gate"], data["phases"]
    overall = overall_percent(phases)
    lines: list[str] = []
    lines.append("# Megi — Progress\n")
    lines.append("<!-- Generated by tools/update_progress.py — do not edit by hand. -->\n")
    lines.append(
        "![Tests](docs/status/badge-tests.svg) "
        "![Conformance](docs/status/badge-conformance.svg) "
        "![Crates](docs/status/badge-crates.svg) "
        "![unsafe](docs/status/badge-unsafe.svg)\n"
    )
    lines.append(
        f"**Snapshot** — commit `{stats['head']}` ({stats['date']}) · "
        f"overall **{overall}%** · **{comma(stats['tests'])}** tests · "
        f"conformance **{gate['status']}** · **{stats['crates']}** crates · "
        f"**{comma(stats['rust_lines'])}** lines of Rust\n"
    )
    lines.append(
        "> This page is generated from `docs/status/progress.toml` plus measured repository "
        "data (git HEAD/date, `cargo test --workspace -- --list`, `test_harness --summary`, "
        "crate list and `#![forbid(unsafe_code)]` scan). No external services are involved.\n"
    )
    lines.append(f"[Open the progress map SVG](docs/status/progress.svg)\n")

    lines.append("## Progress map\n")
    lines.append("| Phase | What | Progress | % |")
    lines.append("|---|---|---|---|")
    for phase in phases:
        percent = int(phase.get("percent", 0))
        lines.append(
            f"| **{phase['id']}** | {md_escape(phase.get('title', ''))} | "
            f"`{bar(percent)}` | {percent}% |"
        )
    lines.append(f"| | **Overall** | `{bar(overall)}` | **{overall}%** |\n")

    lines.append("## What is done and what remains\n")
    for phase in phases:
        percent = int(phase.get("percent", 0))
        lines.append(f"### {phase['id']} — {phase.get('title', '')} ({percent}%)\n")
        items = phase.get("items", [])
        pending = phase.get("pending", [])
        lines.append("**Done**\n")
        for item in items:
            lines.append(f"- {item}")
        if pending:
            lines.append("\n**Pending**\n")
            for item in pending:
                lines.append(f"- {item}")
        lines.append("")

    lines.append("## Repository statistics (measured)\n")
    lines.append("| Metric | Value |")
    lines.append("|---|---|")
    lines.append(f"| Commit | `{stats['head']}` ({stats['date']}) |")
    lines.append(f"| Crates in the workspace | {stats['crates']} |")
    lines.append(f"| Lines of Rust (tracked `.rs`) | {stats['rust_lines']} |")
    lines.append(f"| Tests (`cargo test --workspace -- --list`) | {stats['tests']} |")
    lines.append(
        f"| Crates with `#![forbid(unsafe_code)]` | {stats['unsafe_forbidden']} / {stats['crates']} |"
    )
    lines.append(f"| Conformance gate | **{gate['status']}** |\n")

    lines.append("## Conformance gate (4 suites)\n")
    lines.append("| Suite | Result |")
    lines.append("|---|---|")
    lines.append(f"| HTML tokenizer | {gate.get('tokenizer', 'n/a')} |")
    lines.append(f"| HTML tree construction | {gate.get('tree', 'n/a')} |")
    lines.append(f"| HTML serializer | {gate.get('serializer', 'n/a')} |")
    lines.append(f"| CSS Syntax (vendored WPT) | {gate.get('css_syntax', 'n/a')} |")
    lines.append(
        f"\nGate status: **{gate['status']}** (known failures allowlisted; see `tests/README` "
        "and `crates/test_harness`).\n"
    )

    lines.append("## Recent rounds\n")
    for number, title in data["rounds"]:
        lines.append(f"- **Round {number}** — {title}")
    lines.append("")

    lines.append("## Next up\n")
    for item in data["next_up"]:
        lines.append(f"- {item}")
    lines.append("")

    lines.append("## Cross-cutting areas\n")
    lines.append("| Area | What | Estimate | Notes |")
    lines.append("|---|---|---|---|")
    for area in data["areas"]:
        lines.append(
            f"| {area.get('id', '')} | {area.get('title', '')} | "
            f"{int(area.get('percent', 0))}% | {area.get('notes', '')} |"
        )
    lines.append("")

    lines.append("## Regenerating this page\n")
    lines.append("```bash")
    lines.append("python3 tools/update_progress.py --write            # full (counts tests, runs gate)")
    lines.append("python3 tools/update_progress.py --write --no-gate  # skip the conformance gate")
    lines.append("python3 tools/update_progress.py --check --no-gate --no-count  # CI freshness")
    lines.append("```\n")
    lines.append(
        f"_Last manifest update by: {data['last_updated_by']}. Generated artifacts: "
        f"`PROGRESS.md`, `docs/status/progress.svg`, `docs/status/badge-*.svg`._"
    )
    return "\n".join(lines).rstrip() + "\n"


def render_progress_svg(data: dict) -> str:
    stats, gate, phases = data["stats"], data["gate"], data["phases"]
    overall = overall_percent(phases)
    width = 900
    top = 132
    row = 54
    footer = 92
    height = top + row * len(phases) + footer
    parts: list[str] = []
    parts.append(
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">'
    )
    parts.append(f"<title id='title'>Megi Browser Engine progress map</title>")
    parts.append(
        f"<desc id='desc'>Overall {overall}% complete; commit {xml_escape(stats['head'])}; "
        f"{stats['crates']} crates; {stats['tests']} tests; conformance {gate['status']}</desc>"
    )
    parts.append(f'<rect width="{width}" height="{height}" fill="#ffffff"/>')
    parts.append(
        f'<rect x="0.5" y="0.5" width="{width - 1}" height="{height - 1}" fill="none" stroke="#d0d7de"/>'
    )
    parts.append(
        f'<text x="28" y="46" font-family="{FONT}" font-size="26" font-weight="700" '
        f'fill="#1f2328">Megi Browser Engine — progress</text>'
    )
    parts.append(
        f'<text x="28" y="70" font-family="{FONT}" font-size="13" fill="#57606a">'
        f'{xml_escape(data.get("subtitle", ""))}</text>'
    )
    parts.append(
        f'<text x="{width - 28}" y="60" text-anchor="end" font-family="{FONT}" '
        f'font-size="34" font-weight="700" fill="#1f2328">{overall}%</text>'
    )
    parts.append(
        f'<text x="{width - 28}" y="80" text-anchor="end" font-family="{FONT}" '
        f'font-size="12" fill="#57606a">overall completion</text>'
    )
    # Overall bar.
    parts.append(
        f'<rect x="28" y="92" width="{width - 56}" height="10" rx="5" fill="#d0d7de"/>'
    )
    parts.append(
        f'<rect x="28" y="92" width="{round((width - 56) * overall / 100)}" height="10" '
        f'rx="5" fill="{phase_color(overall)}"/>'
    )
    for index, phase in enumerate(phases):
        percent = int(phase.get("percent", 0))
        y = top + index * row
        color = phase_color(percent)
        items = len(phase.get("items", []))
        pending = len(phase.get("pending", []))
        parts.append(
            f'<text x="28" y="{y}" font-family="{FONT}" font-size="13" font-weight="700" '
            f'fill="#1f2328">{xml_escape(phase.get("id", ""))}</text>'
        )
        parts.append(
            f'<text x="66" y="{y}" font-family="{FONT}" font-size="13" fill="#1f2328">'
            f'{xml_escape(phase.get("title", ""))}</text>'
        )
        parts.append(
            f'<text x="{width - 28}" y="{y}" text-anchor="end" font-family="{FONT}" '
            f'font-size="12" fill="#57606a">{items} done · {pending} pending</text>'
        )
        parts.append(
            f'<rect x="28" y="{y + 14}" width="{width - 210}" height="16" rx="8" fill="#eaeef2"/>'
        )
        parts.append(
            f'<rect x="28" y="{y + 14}" width="{round((width - 210) * percent / 100)}" '
            f'height="16" rx="8" fill="{color}"/>'
        )
        parts.append(
            f'<text x="{width - 28}" y="{y + 27}" text-anchor="end" font-family="{FONT}" '
            f'font-size="13" font-weight="700" fill="{color}">{percent}%</text>'
        )
    footer_y = top + row * len(phases) + 8
    parts.append(
        f'<line x1="28" y1="{footer_y}" x2="{width - 28}" y2="{footer_y}" stroke="#d0d7de"/>'
    )
    parts.append(
        f'<text x="28" y="{footer_y + 24}" font-family="{FONT}" font-size="12" fill="#57606a">'
        f'commit {xml_escape(stats["head"])} ({xml_escape(stats["date"])}) · '
        f'{stats["crates"]} crates · {comma(stats["tests"])} tests · '
        f'conformance {xml_escape(gate["status"])} · '
        f'{comma(stats["rust_lines"])} lines of Rust</text>'
    )
    parts.append(
        f'<text x="28" y="{footer_y + 44}" font-family="{FONT}" font-size="11" fill="#8c959f">'
        f'Generated by tools/update_progress.py from docs/status/progress.toml — do not edit.</text>'
    )
    parts.append(
        f'<text x="28" y="{footer_y + 62}" font-family="{FONT}" font-size="11" fill="#8c959f">'
        f'Legend: green ≥95% · blue ≥85% · amber ≥70% · red &lt;70%</text>'
    )
    parts.append("</svg>")
    return "\n".join(parts).rstrip() + "\n"


def render_badge(label: str, value: str, color: str) -> str:
    label_width = max(38, 8 + len(label) * 7)
    value_width = max(30, 10 + len(value) * 7)
    width = label_width + value_width
    height = 20
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" role="img" aria-label="{xml_escape(label)}: '
        f'{xml_escape(value)}">\n'
        f'  <title>{xml_escape(label)}: {xml_escape(value)}</title>\n'
        f'  <rect width="{label_width}" height="{height}" fill="#555555"/>\n'
        f'  <rect x="{label_width}" width="{value_width}" height="{height}" fill="{color}"/>\n'
        f'  <g fill="#ffffff" text-anchor="middle" font-family="{FONT}" font-size="11">\n'
        f'    <text x="{label_width / 2:.0f}" y="14">{xml_escape(label)}</text>\n'
        f'    <text x="{label_width + value_width / 2:.0f}" y="14">'
        f'{xml_escape(value)}</text>\n'
        f"  </g>\n"
        f"</svg>\n"
    )


def render_badges(data: dict) -> dict[str, str]:
    stats, gate = data["stats"], data["gate"]
    conformance = "PASS" if gate["status"] == "PASS" else "FAIL"
    conformance_color = "#2ea043" if gate["status"] == "PASS" else "#da3633"
    unsafe_text = f"{stats['unsafe_forbidden']}/{stats['crates']}"
    unsafe_color = "#2ea043" if stats["unsafe_forbidden"] == stats["crates"] else "#d29922"
    return {
        "tests": render_badge("tests", comma(stats["tests"]), "#1f6feb"),
        "conformance": render_badge("conformance", f"{conformance} · 4 suites", conformance_color),
        "crates": render_badge("crates", str(stats["crates"]), "#1f6feb"),
        "unsafe": render_badge("#![forbid(unsafe)]", unsafe_text, unsafe_color),
    }


def update_manifest(text: str, stats: dict, gate: dict[str, str]) -> str:
    """Rewrites only the generated `[stats]` and `[gate]` TOML blocks."""
    stats_block = [
        "[stats]",
        f"tests = {stats['tests']}",
        f'tests_commit = "{stats["head"]}"',
        f'tests_date = "{stats["date"]}"',
        f"crates = {stats['crates']}",
        f"rust_lines = {stats['rust_lines']}",
        f"unsafe_forbidden_crates = {stats['unsafe_forbidden']}",
        "",
    ]
    gate_block = [
        "[gate]",
        f'status = "{gate["status"]}"',
        f'verified_commit = "{stats["head"]}"',
        f'tokenizer = "{gate.get("tokenizer", "n/a")}"',
        f'tree = "{gate.get("tree", "n/a")}"',
        f'serializer = "{gate.get("serializer", "n/a")}"',
        f'css_syntax = "{gate.get("css_syntax", "n/a")}"',
        "",
    ]
    lines = text.splitlines()
    output: list[str] = []
    index = 0
    while index < len(lines):
        stripped = lines[index].strip()
        if stripped == "[stats]":
            output.extend(stats_block)
            index += 1
            while index < len(lines) and not lines[index].strip().startswith("["):
                index += 1
            continue
        if stripped == "[gate]":
            output.extend(gate_block)
            index += 1
            while index < len(lines) and not lines[index].strip().startswith("["):
                index += 1
            continue
        output.append(lines[index])
        index += 1
    return "\n".join(output).rstrip() + "\n"


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
    manifest_gate = manifest.get("gate", {})
    tests = count_tests(
        root,
        root / "target" / "progress-stats.json",
        head,
        int(manifest_stats.get("tests", 0)),
        no_count,
    )
    gate = run_gate(root, manifest_gate) if not no_gate else dict(manifest_gate)
    unsafe = unsafe_forbidden_crates(root)
    stats = {
        "head": head,
        "date": date,
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
    }

    artifacts: dict[str, bytes] = {}
    artifacts["PROGRESS.md"] = render_markdown(data).encode("utf-8")
    artifacts[str(STATUS_DIR / "progress.svg")] = render_progress_svg(data).encode("utf-8")
    badges = render_badges(data)
    for key, filename in BADGE_FILES.items():
        artifacts[str(STATUS_DIR / filename)] = badges[key].encode("utf-8")
    artifacts[str(manifest_path.relative_to(root))] = update_manifest(
        manifest_path.read_text(encoding="utf-8"), stats, gate
    ).encode("utf-8")
    data["stats"]["rust_lines"] = comma(stats["rust_lines"])  # only for logging
    return artifacts


def print_stats(root: Path, artifacts: dict[str, bytes]) -> None:
    manifest = tomllib.loads((root / STATUS_DIR / "progress.toml").read_text(encoding="utf-8"))
    print(
        f"progress: commit {git_short_head(root)} ({git_commit_date(root)}) · "
        f"tests {manifest['stats']['tests']} · gate {manifest['gate']['status']} · "
        f"crates {manifest['stats']['crates']} · "
        f"unsafe-forbidden {manifest['stats']['unsafe_forbidden_crates']}/{manifest['stats']['crates']} · "
        f"Rust lines {manifest['stats']['rust_lines']}"
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
    artifacts = build_artifacts(root, no_count=args.no_count, no_gate=args.no_gate)

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
    print_stats(root, artifacts)
    if written:
        print(f"wrote {len(written)} file(s): {', '.join(written)}")
    else:
        print("artifacts already up to date (nothing written)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
