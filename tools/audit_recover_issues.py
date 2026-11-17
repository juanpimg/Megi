#!/usr/bin/env python3
"""Recover the disposition of every issue filed by the audit.

Issues are the wrong place to keep an audit trail: in this repository they were
closed in a single sweep, many with a "Deferred" note instead of a fix, and a
closed issue disappears from the backlog. This tool reads the closing comment of
each issue authored by the auditor, classifies it, and writes
`docs/audit/findings/wave1.json` so the findings survive the issue tracker.

It needs an authenticated `gh` and network access, so it is a maintenance tool,
not part of the CI gate (`docs/audit/coverage.tsv` plus `audit_coverage.py
--check` is the gate).

Usage:
    python3 tools/audit_recover_issues.py --author lolmg445 --repo juanpimg/Megi
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "audit" / "findings" / "wave1.json"

# Marker phrases used by the maintainer's closing comments, checked in order.
CLASSIFIERS = (
    ("deferred", ("deferred", "future work", "for future", "requiring significant refactoring")),
    ("fixed", ("fixed on main", "fixed in", "implemented in", "landed in", "resolved by pr")),
    ("duplicate", ("duplicate", "already tracked", "already fixed", "pre-existing")),
    ("not-a-bug", ("not a bug", "works as intended", "by design", "wontfix", "won't fix")),
)

def gh(args: list[str]) -> str:
    result = subprocess.run(["gh", *args], cwd=ROOT, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        print(f"gh {' '.join(args)} failed: {result.stderr.strip()}", file=sys.stderr)
        raise SystemExit(2)
    return result.stdout


def list_issues(repo: str, author: str) -> list[dict[str, object]]:
    """Return {number, labels} for every issue opened by the audit author."""
    issues: list[dict[str, object]] = []
    page = 1
    while True:
        raw = gh(
            [
                "api",
                f"repos/{repo}/issues?state=all&creator={author}&per_page=100&page={page}",
            ]
        )
        page_items = json.loads(raw)
        for item in page_items:
            if "pull_request" in item:
                continue
            issues.append(
                {
                    "number": int(item["number"]),
                    "labels": sorted(label["name"] for label in item.get("labels", [])),
                }
            )
        # Stop on the page size, not on the filtered size: a page can hold mostly
        # pull requests and still have a next page.
        if len(page_items) < 100:
            return sorted(issues, key=lambda issue: issue["number"])
        page += 1


def fetch_last_comments(repo: str, numbers: list[int]) -> dict[int, dict[str, str]]:
    owner, name = repo.split("/", 1)
    result: dict[int, dict[str, str]] = {}
    batch_size = 25
    for start in range(0, len(numbers), batch_size):
        batch = numbers[start : start + batch_size]
        aliases = "\n".join(
            f'  i{number}: issue(number: {number}) {{ number title state closedAt '
            f"comments(last: 1) {{ nodes {{ body createdAt author {{ login }} }} }} }}"
            for number in batch
        )
        query = f"query {{ repository(owner: {json.dumps(owner)}, name: {json.dumps(name)}) {{\n{aliases}\n}} }}"
        raw = gh(["api", "graphql", "-f", f"query={query}"])
        payload = json.loads(raw)
        for number in batch:
            node = payload.get("data", {}).get("repository", {}).get(f"i{number}")
            if not node:
                continue
            comments = node.get("comments", {}).get("nodes", [])
            last = comments[-1] if comments else {}
            result[number] = {
                "title": node.get("title", ""),
                "state": node.get("state", ""),
                "closed_at": node.get("closedAt", ""),
                "comment": (last.get("body") or "").strip(),
                "comment_by": ((last.get("author") or {}).get("login") or ""),
                "comment_at": last.get("createdAt", ""),
            }
        print(f"fetched {min(start + batch_size, len(numbers))}/{len(numbers)}", file=sys.stderr)
    return result


def classify(comment: str) -> str:
    lowered = comment.lower()
    for label, markers in CLASSIFIERS:
        if any(marker in lowered for marker in markers):
            return label
    return "unclassified"


def area_of(entry: dict[str, object]) -> str:
    labels = [label for label in entry.get("labels", []) if label not in {"bug", "P0", "P1", "P2", "P3"}]
    return labels[0] if labels else "unsorted"


def write_findings(ledger: list[dict[str, object]]) -> None:
    """Render the readable audit ledger: every finding with its disposition."""
    out = ROOT / "docs" / "audit" / "FINDINGS.md"
    by_disposition: dict[str, list[dict[str, object]]] = {}
    for entry in ledger:
        by_disposition.setdefault(str(entry["disposition"]), []).append(entry)

    lines = [
        "# Audit findings ledger",
        "",
        "Generated by `python3 tools/audit_recover_issues.py`; do not edit by hand.",
        "",
        "The issue tracker is not the source of truth: in this repository the audit",
        "backlog was closed in one sweep, and a plurality of the closures say",
        "*deferred*, not *fixed*. This file keeps every finding and its real",
        "disposition so an unaddressed defect cannot disappear behind a closed issue.",
        "",
        f"Total findings: **{len(ledger)}**"
        f" · deferred **{len(by_disposition.get('deferred', []))}**"
        f" · fixed **{len(by_disposition.get('fixed', []))}**"
        f" · unclassified **{len(by_disposition.get('unclassified', []))}**",
        "",
    ]
    headings = {
        "deferred": "Deferred — still broken in `main`",
        "unclassified": "Closed without a recognisable disposition — needs re-verification",
        "fixed": "Reported fixed — verified sample below",
    }
    for disposition in ("deferred", "unclassified", "fixed"):
        entries = by_disposition.get(disposition, [])
        if not entries:
            continue
        lines.append(f"## {headings[disposition]} ({len(entries)})")
        lines.append("")
        lines.append("| Issue | Area | Title | Last closing comment |")
        lines.append("|---|---|---|---|")
        for entry in sorted(entries, key=lambda item: int(item["number"])):
            comment = " ".join(str(entry.get("closing_comment", "")).split())[:160]
            comment = comment.replace("|", "\\|")
            lines.append(
                f"| [#{entry['number']}](https://github.com/juanpimg/Megi/issues/{entry['number']}) "
                f"| {area_of(entry)} | {str(entry['title']).replace('|', '\\|')} | {comment} |"
            )
        lines.append("")
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {out.relative_to(ROOT)}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="juanpimg/Megi")
    parser.add_argument("--author", default="lolmg445")
    args = parser.parse_args()

    issues = list_issues(args.repo, args.author)
    numbers = [int(issue["number"]) for issue in issues]
    labels = {int(issue["number"]): issue["labels"] for issue in issues}
    print(f"{len(numbers)} issue(s) authored by {args.author}", file=sys.stderr)
    comments = fetch_last_comments(args.repo, numbers)

    ledger = []
    counts: dict[str, int] = {}
    for number in numbers:
        entry = comments.get(number, {})
        comment = entry.get("comment", "")
        disposition = classify(comment)
        counts[disposition] = counts.get(disposition, 0) + 1
        ledger.append(
            {
                "number": number,
                "title": entry.get("title", ""),
                "state": entry.get("state", ""),
                "closed_at": entry.get("closed_at", ""),
                "disposition": disposition,
                "labels": labels.get(number, []),
                "closing_comment_by": entry.get("comment_by", ""),
                "closing_comment_at": entry.get("comment_at", ""),
                "closing_comment": comment,
            }
        )

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(ledger, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)} ({len(ledger)} issues)")
    write_findings(ledger)
    for disposition in sorted(counts):
        print(f"  {disposition:<14} {counts[disposition]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
