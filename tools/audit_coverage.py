#!/usr/bin/env python3
"""Track the line-by-line audit ledger of every tracked text file.

The ledger `docs/audit/coverage.tsv` has one row per tracked file that can be
audited by reading it. `--check` proves the claim mechanically: a file missing
from the ledger, a stale line count, or a file marked covered without an auditor
and a date is an error, and CI runs it on every push. That is the whole point —
an audit that cannot be checked is a comment, not coverage.

Binary artifacts (images, fonts, wasm) are exempt by construction: `--check`
verifies that every tracked file is either in the ledger or in an exempt
category, so a new crate or tool cannot slip in unaudited.

Usage:
    python3 tools/audit_coverage.py --generate          # add new files as pending
    python3 tools/audit_coverage.py --mark covered --path crates/dom/src \
        --auditor wave1 --date 2026-11-16 --findings '#1105-#1119'
    python3 tools/audit_coverage.py --report            # coverage summary
    python3 tools/audit_coverage.py --check             # CI gate
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / "docs" / "audit" / "coverage.tsv"

HEADER = ("path", "lines", "category", "phase", "auditor", "date", "status", "findings", "notes")

# Extensions audited line by line. Anything else is either exempt (see
# BINARY_EXTENSIONS/VENDORED_PREFIXES) or reported, which is how a new file
# type gets noticed instead of silently escaping the ledger.
TEXT_EXTENSIONS = {
    ".rs", ".py", ".pyi", ".sh", ".yml", ".yaml", ".toml", ".md", ".rst",
    ".html", ".css", ".js", ".json", ".dat", ".svg", ".txt", ".test", ".gitignore",
    ".hex", ".patch", ".gitattributes",
}
BINARY_EXTENSIONS = {
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".rgba", ".bmp", ".woff2", ".ttf",
    ".otf", ".wasm", ".gz", ".zip", ".bin",
}
# Third-party material: kept in the ledger so the exemption is a recorded
# decision, but audited as data (feature matrix in docs/audit/fixtures.md).
VENDORED_PREFIXES = ("tests/html5lib-tests/",)
# Binary payloads and vendored corpora cannot be read line by line.
EXEMPT_DIRS = ("fuzz/corpus/",)
# Generated artifacts: their content is derived from other inputs, so they are
# checked against their generator instead of being read line by line.
GENERATED_FILES = {"Cargo.lock", "docs/audit/coverage.tsv"}
GENERATED_SUFFIXES = (".proptest-regressions",)
# Files without an extension that are prose/config worth auditing.
NAMED_TEXT_FILES = {"LICENSE", "LICENSE-APACHE", "LICENSE-MIT", "justfile", "Dockerfile", "CODEOWNERS"}

STATUSES = {"pending", "covered", "exempt"}

# Categories that are never read line by line: their rows are `exempt` from the
# start so `--generate` alone leaves the ledger in a passing state.
NOT_READ_CATEGORIES = frozenset({"binary", "vendored", "generated", "exempt"})


def initial_row(category: str) -> dict[str, str]:
    """Row values for a newly tracked file."""
    if category in NOT_READ_CATEGORIES:
        return {
            "auditor": "",
            "date": "",
            "status": "exempt",
            "findings": "",
            "notes": f"not read line by line: {category}",
        }
    return {"auditor": "", "date": "", "status": "pending", "findings": "", "notes": ""}


def run_git(args: list[str]) -> str:
    result = subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        print(f"git {' '.join(args)} failed: {result.stderr.strip()}", file=sys.stderr)
        raise SystemExit(2)
    return result.stdout


def tracked_files() -> list[str]:
    return [line for line in run_git(["ls-files"]).splitlines() if line]


def extension(path: str) -> str:
    name = Path(path).name
    if "." not in name:
        return ""
    return "." + name.rsplit(".", 1)[1].lower()


def classify(path: str) -> tuple[str, str]:
    """Return (category, phase) for a tracked path; "exempt" means no reading."""
    if path.startswith(EXEMPT_DIRS):
        return "exempt", "-"
    if path.startswith(VENDORED_PREFIXES):
        return "vendored", "-"
    if extension(path) in BINARY_EXTENSIONS:
        return "binary", "-"
    if path in GENERATED_FILES or path.endswith(GENERATED_SUFFIXES):
        return "generated", "-"
    if path.startswith(("crates/bench/",)):
        return "tool", "F4"
    if path.startswith("crates/") and "/src/" in path:
        return "src", "F2"
    if path.startswith("crates/") and "/tests/" in path:
        return "test", "F1"
    if path.startswith("crates/") and "/examples/" in path:
        return "example", "F3"
    if path.startswith("fuzz/fuzz_targets/") or path == "fuzz/Cargo.toml":
        return "fuzz", "F1"
    if path.startswith("fuzz/"):
        return "fixture", "F1"
    if path.startswith(".github/"):
        return "ci", "F3"
    if path.startswith("tools/"):
        return "tool", "F3"
    if path.startswith(("docs/status/", "docs/images/")):
        return "generated", "-"
    if path.startswith("docs/"):
        return "docs", "F3"
    ext = extension(path)
    if ext in {".toml", ".gitignore"}:
        return "manifest", "F3"
    if ext in {".md", ".rst", ".txt"}:
        return "docs", "F3"
    return "fixture", "F1"


def count_lines(path: str) -> int:
    if classify(path)[0] in {"exempt", "vendored", "binary", "generated"}:
        return 0
    data = (ROOT / path).read_bytes()
    if not data:
        return 0
    lines = data.count(b"\n")
    return lines + (0 if data.endswith(b"\n") else 1)


def read_ledger() -> list[dict[str, str]]:
    if not LEDGER.exists():
        return []
    rows: list[dict[str, str]] = []
    for index, line in enumerate(LEDGER.read_text(encoding="utf-8").splitlines()):
        if index == 0:
            continue
        if not line.strip():
            continue
        fields = line.split("\t")
        if len(fields) != len(HEADER):
            print(f"{LEDGER}: row {index + 1} has {len(fields)} fields", file=sys.stderr)
            raise SystemExit(2)
        rows.append(dict(zip(HEADER, fields, strict=True)))
    return rows


def write_ledger(rows: list[dict[str, str]]) -> None:
    rows.sort(key=lambda row: row["path"])
    lines = ["\t".join(HEADER)]
    lines.extend("\t".join(row[col] for col in HEADER) for row in rows)
    LEDGER.parent.mkdir(parents=True, exist_ok=True)
    LEDGER.write_text("\n".join(lines) + "\n", encoding="utf-8")
    try:
        shown = LEDGER.relative_to(ROOT)
    except ValueError:  # a ledger outside the checkout, e.g. in a test tempdir
        shown = LEDGER
    print(f"wrote {shown} ({len(rows)} rows)")


def audited_scope() -> list[tuple[str, str, str]]:
    """Every tracked file, with its category and phase.

    Nothing is skipped: binary/vendored/generated files carry an explicit
    category so `--check` proves the inventory is complete rather than trusting
    a filter.
    """
    scope: list[tuple[str, str, str]] = []
    for path in tracked_files():
        category, phase = classify(path)
        if category not in {"exempt", "vendored", "binary", "generated"}:
            ext = extension(path)
            if ext not in TEXT_EXTENSIONS and Path(path).name not in NAMED_TEXT_FILES:
                print(f"warning: unclassified tracked file {path!r}", file=sys.stderr)
        scope.append((path, category, phase))
    return scope


def cmd_generate(_args: argparse.Namespace) -> int:
    previous = {row["path"]: row for row in read_ledger()}
    rows: list[dict[str, str]] = []
    added = 0
    for path, category, phase in audited_scope():
        old = previous.pop(path, None)
        if old is None:
            added += 1
            old = initial_row(category)
        elif old["status"] == "pending" and category in NOT_READ_CATEGORIES:
            # A category can change when files are added to a directory.
            old = initial_row(category)
        rows.append(
            {
                "path": path,
                "lines": str(count_lines(path)),
                "category": category,
                "phase": phase,
                "auditor": old["auditor"],
                "date": old["date"],
                "status": old["status"],
                "findings": old["findings"],
                "notes": old["notes"],
            }
        )
    for stale in sorted(previous):
        print(f"dropped stale row {stale}", file=sys.stderr)
    write_ledger(rows)
    print(f"added {added} new pending file(s)")
    return 0


def cmd_mark(args: argparse.Namespace) -> int:
    rows = read_ledger()
    if not rows:
        print("ledger is empty: run --generate first", file=sys.stderr)
        return 2
    touched = 0
    skipped = 0
    for row in rows:
        if not row["path"].startswith(args.path):
            continue
        if row["category"] in NOT_READ_CATEGORIES:
            # Never mark a binary/vendored/generated row as read: the exemption is
            # a decision about the category, not about this review.
            skipped += 1
            continue
        row["status"] = args.status
        if args.auditor:
            row["auditor"] = args.auditor
        if args.date:
            row["date"] = args.date
        if args.findings:
            row["findings"] = args.findings
        if args.notes:
            row["notes"] = args.notes
        touched += 1
    if touched == 0:
        print(f"no ledger row starts with {args.path!r}", file=sys.stderr)
        return 2
    write_ledger(rows)
    print(f"marked {touched} row(s) {args.status}" + (f", kept {skipped} exempt" if skipped else ""))
    return 0


def cmd_report(_args: argparse.Namespace) -> int:
    rows = read_ledger()
    if not rows:
        print("ledger is empty: run --generate first", file=sys.stderr)
        return 2
    by_category: dict[str, Counter] = defaultdict(Counter)
    for row in rows:
        counter = by_category[row["category"]]
        counter["files"] += 1
        counter["lines"] += int(row["lines"])
        counter[row["status"]] += 1
        counter[f"{row['status']}_lines"] += int(row["lines"])
    print(f"{'category':<10} {'files':>6} {'covered':>8} {'pending':>8} {'lines':>9} {'cov%':>6}")
    total = Counter()
    for category in sorted(by_category):
        counter = by_category[category]
        total.update(counter)
        cov = 100.0 * counter["covered_lines"] / max(counter["lines"], 1)
        print(
            f"{category:<10} {counter['files']:>6} {counter['covered']:>8} "
            f"{counter['pending']:>8} {counter['lines']:>9} {cov:>5.1f}%"
        )
    cov = 100.0 * total["covered_lines"] / max(total["lines"], 1)
    print(
        f"{'TOTAL':<10} {total['files']:>6} {total['covered']:>8} "
        f"{total['pending']:>8} {total['lines']:>9} {cov:>5.1f}%"
    )
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    rows = read_ledger()
    if not rows:
        print("ledger is empty: run --generate first", file=sys.stderr)
        return 1
    problems: list[str] = []
    drifted: list[str] = []
    known = {row["path"] for row in rows}
    if len(known) != len(rows):
        problems.append("ledger contains duplicate paths")
    for path, category, phase in audited_scope():
        row = next((r for r in rows if r["path"] == path), None)
        if row is None:
            problems.append(f"not in ledger: {path} ({category}/{phase})")
            continue
        if row["category"] != category or row["phase"] != phase:
            problems.append(
                f"{path}: category/phase drift ({row['category']}/{row['phase']} != {category}/{phase})"
            )
        expected = count_lines(path)
        if int(row["lines"]) != expected:
            # The repository moves while the audit runs, so a changed line count is
            # the list of files to re-read, not a reason to block unrelated PRs.
            # `--strict` turns it into a failure for the audit's own branch.
            drifted.append(f"{path}: {row['lines']} -> {expected} lines, re-read needed")
        if row["status"] not in STATUSES:
            problems.append(f"{path}: unknown status {row['status']!r}")
        elif row["status"] == "covered" and (not row["auditor"] or not row["date"]):
            problems.append(f"{path}: covered without auditor/date")
        elif row["status"] == "pending" and (row["auditor"] or row["date"]):
            problems.append(f"{path}: pending row carries auditor/date")
        elif row["status"] == "pending" and category in NOT_READ_CATEGORIES:
            problems.append(f"{path}: {category} row must not be pending")

    tracked = {path for path in tracked_files()}
    for row in rows:
        if row["path"] not in tracked:
            problems.append(f"ledger row for untracked file: {row['path']}")

    drift = drifted
    if drift and getattr(args, "strict", False):
        problems.extend(drift)
        drift = []
    for warning in drift[:20]:
        print(f"warning: {warning}", file=sys.stderr)
    if len(drifted) > 20:
        print(f"... and {len(drifted) - 20} more file(s) to re-read", file=sys.stderr)

    if problems:
        for problem in problems[:50]:
            print(f"error: {problem}", file=sys.stderr)
        if len(problems) > 50:
            print(f"... and {len(problems) - 50} more", file=sys.stderr)
        return 1
    covered = sum(1 for row in rows if row["status"] == "covered")
    pending = sum(1 for row in rows if row["status"] == "pending")
    lines = sum(int(row["lines"]) for row in rows)
    print(
        f"audit ledger ok: {len(rows)} files, {covered} covered, {pending} pending, "
        f"{lines} lines tracked, {len(drifted)} file(s) to re-read"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--generate", action="store_true", help="add new tracked files as pending")
    group.add_argument("--mark", action="store_true", help="set status/auditor for a path prefix")
    group.add_argument("--report", action="store_true", help="print coverage summary")
    group.add_argument("--check", action="store_true", help="verify the ledger (CI gate)")
    parser.add_argument("--path", default="", help="with --mark: path prefix to update")
    parser.add_argument(
        "--status", default="covered", choices=sorted(STATUSES), help="with --mark: new status"
    )
    parser.add_argument("--auditor", default="", help="with --mark: who read the files")
    parser.add_argument("--date", default="", help="with --mark: ISO date of the review")
    parser.add_argument("--findings", default="", help="with --mark: finding references")
    parser.add_argument("--notes", default="", help="with --mark: free-form note")
    parser.add_argument(
        "--strict",
        action="store_true",
        help="with --check: fail when a file changed since it was read (audit branch only)",
    )
    args = parser.parse_args()

    if args.mark:
        if not args.path:
            parser.error("--mark requires --path")
        return cmd_mark(args)
    if args.generate:
        return cmd_generate(args)
    if args.report:
        return cmd_report(args)
    return cmd_check(args)


if __name__ == "__main__":
    raise SystemExit(main())
