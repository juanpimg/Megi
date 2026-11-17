#!/usr/bin/env python3
"""Offline Markdown link checker for the repository documentation (#725).

The docs reference many specs, tools and sibling files. Nothing verified those
references, so a renamed file or a reworded heading rotted silently. `lychee`
would also check external URLs, but that requires network access in CI and is
noisy for third-party spec URLs; this script deliberately checks only what the
repository controls:

* every relative link target exists (file or directory),
* every same-repo `#fragment` matches a heading anchor of the target document
  (GitHub slug rules, approximate but stable),
* external `http(s)://` links are skipped (they belong to a scheduled job),
* `mailto:` and bare anchors are skipped.

Exit code 0 means every checked link resolved; 1 lists the offenders.
"""

from __future__ import annotations

import re
import sys
import unicodedata
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Documents whose links are checked. Everything under `docs/` plus the
# repository-root Markdown files.
ROOT_DOCS = ["README.md", "CONTRIBUTING.md", "SECURITY.md", "CHANGELOG.md"]

INLINE_LINK = re.compile(r"(?<!\!)\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
REFERENCE_LINK = re.compile(r"^\s*\[[^\]]+\]:\s*(\S+)", re.MULTILINE)
HEADING = re.compile(r"^(#{1,6})\s+(.*)$", re.MULTILINE)
FENCE = re.compile(r"^```.*?^```", re.MULTILINE | re.DOTALL)
INLINE_CODE = re.compile(r"`[^`\n]*`")


def slugify(text: str) -> str:
    """Approximate GitHub heading anchor: lowercase, drop punctuation, spaces→'-'."""
    text = text.strip()
    # Strip inline code/links/explicit anchor tags from the heading text.
    text = re.sub(r"<a[^>]*>.*?</a>", "", text)
    text = INLINE_CODE.sub(lambda m: m.group(0).strip("`"), text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = unicodedata.normalize("NFKD", text)
    kept = []
    for ch in text.lower():
        if ch.isalnum() or ch in "-_ ":
            kept.append(ch)
    slug = "".join(kept).strip().replace(" ", "-")
    return slug


def headings_of(path: Path) -> set[str]:
    try:
        body = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return set()
    body = FENCE.sub("", body)
    return {slugify(m.group(2)) for m in HEADING.finditer(body)}


def links_of(path: Path) -> list[str]:
    try:
        body = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return []
    body = FENCE.sub("", body)
    body = INLINE_CODE.sub("", body)
    found = [m.group(1) for m in INLINE_LINK.finditer(body)]
    found.extend(m.group(1) for m in REFERENCE_LINK.finditer(body))
    return found


def documents() -> list[Path]:
    docs = [REPO_ROOT / name for name in ROOT_DOCS if (REPO_ROOT / name).is_file()]
    docs_dir = REPO_ROOT / "docs"
    if docs_dir.is_dir():
        docs.extend(sorted(p for p in docs_dir.rglob("*.md") if p.is_file()))
    return docs


def check() -> list[str]:
    problems: list[str] = []
    anchor_cache: dict[Path, set[str]] = {}

    for doc in documents():
        for raw in links_of(doc):
            target = raw.strip()
            if not target or target.startswith(("http://", "https://", "mailto:", "tel:")):
                continue
            if target.startswith("<") and target.endswith(">"):
                target = target[1:-1]
            path_part, _, fragment = target.partition("#")
            if not path_part:
                # Same-document anchor.
                resolved = doc
            else:
                resolved = (doc.parent / path_part.split("?")[0]).resolve()
                if not resolved.exists():
                    problems.append(f"{doc.relative_to(REPO_ROOT)}: missing target {target!r}")
                    continue
            if not fragment:
                continue
            if not resolved.is_file():
                continue
            if resolved.suffix.lower() != ".md":
                continue
            if resolved not in anchor_cache:
                anchor_cache[resolved] = headings_of(resolved)
            if fragment.lower() not in anchor_cache[resolved]:
                problems.append(
                    f"{doc.relative_to(REPO_ROOT)}: missing anchor #{fragment} "
                    f"in {resolved.relative_to(REPO_ROOT)}"
                )
    return problems


def main() -> int:
    problems = check()
    if not problems:
        print(f"docs links: OK ({len(documents())} files checked)")
        return 0
    print(f"docs links: {len(problems)} problem(s)")
    for problem in problems:
        print(f"  - {problem}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
