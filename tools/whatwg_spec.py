#!/usr/bin/env python3
"""
Megi Engine — WHATWG & W3C Specification Fetcher, Cache Manager & Normative Extractor.

Fetches, caches, indexes, and extracts formal algorithm sections and error tables
from the official WHATWG HTML Living Standard (https://html.spec.whatwg.org/multipage/),
WHATWG DOM Standard (https://dom.spec.whatwg.org/), and W3C CSS Syntax Module Level 3
(https://drafts.csswg.org/css-syntax-3/).
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.request
from pathlib import Path
from typing import Dict, List, Optional, Tuple

try:
    import lxml.html
except ImportError:
    print("Missing dependency: lxml. Install with: pip install lxml", file=sys.stderr)
    sys.exit(1)

REPO_ROOT = Path(__file__).resolve().parent.parent
CACHE_DIR = REPO_ROOT / ".cache" / "specs"

SPEC_URLS: Dict[str, str] = {
    "parsing.html": "https://html.spec.whatwg.org/multipage/parsing.html",
    "syntax.html": "https://html.spec.whatwg.org/multipage/syntax.html",
    "dom.html": "https://html.spec.whatwg.org/multipage/dom.html",
    "entities.json": "https://html.spec.whatwg.org/entities.json",
    "whatwg-dom.html": "https://dom.spec.whatwg.org/",
    "css-syntax-3.html": "https://drafts.csswg.org/css-syntax-3/",
}

DEFAULT_FETCH_TARGETS = [
    "parsing.html",
    "syntax.html",
    "dom.html",
    "entities.json",
]


MAX_SPEC_BYTES = 50 * 1024 * 1024  # 50 MiB cap

def ensure_cached(filename: str, force: bool = False) -> Path:
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    target = CACHE_DIR / filename
    if target.exists() and target.stat().st_size > 0 and not force:
        return target
    url = SPEC_URLS.get(filename)
    if not url:
        raise ValueError(f"Unknown spec target: {filename}")
    req = urllib.request.Request(
        url,
        headers={"User-Agent": "MegiBrowserEngine-SpecTool/0.1 (+https://github.com/juanpimg/Megi)"},
    )
    tmp = target.with_suffix(target.suffix + ".tmp")
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = resp.read(MAX_SPEC_BYTES + 1)
        if len(data) > MAX_SPEC_BYTES:
            raise ValueError(f"Spec download exceeds {MAX_SPEC_BYTES} bytes")
        tmp.write_bytes(data)
        tmp.replace(target)
    finally:
        if tmp.exists():
            tmp.unlink(missing_ok=True)
    return target


def cmd_fetch(args: argparse.Namespace) -> int:
    targets = list(SPEC_URLS.keys()) if args.all else DEFAULT_FETCH_TARGETS
    for name in targets:
        path = ensure_cached(name, force=args.force)
        size_kb = path.stat().st_size / 1024.0
        print(f"[OK] Cached {name:<18} -> {path.relative_to(REPO_ROOT)} ({size_kb:.1f} KiB)")
    return 0


def parse_headings(html_path: Path) -> List[Tuple[str, str, str, lxml.html.HtmlElement]]:
    """Returns list of (secno, anchor_id, title_text, heading_element)."""
    raw = html_path.read_text(encoding="utf-8", errors="replace")
    doc = lxml.html.fromstring(raw)
    results = []
    for h in doc.xpath("//h2 | //h3 | //h4 | //h5 | //h6"):
        anchor_id = h.get("id") or ""
        secno_nodes = h.xpath('.//span[contains(@class, "secno")]')
        secno = secno_nodes[0].text_content().strip() if secno_nodes else ""
        full_text = " ".join(h.text_content().split())
        if secno and full_text.startswith(secno):
            title = full_text[len(secno) :].strip()
        else:
            title = full_text
        # strip trailing pilcrow if present
        title = title.rstrip("¶").strip()
        results.append((secno, anchor_id, title, h))
    return results


def cmd_toc(args: argparse.Namespace) -> int:
    page = args.page if args.page.endswith(".html") else f"{args.page}.html"
    path = ensure_cached(page)
    headings = parse_headings(path)
    filter_q = (args.filter or "").lower()
    count = 0
    for secno, anchor_id, title, _ in headings:
        line = f"{secno:<12} #{anchor_id:<52} {title}"
        if filter_q and filter_q not in line.lower():
            continue
        print(line)
        count += 1
    print(f"\nTotal sections listed: {count}")
    return 0


def extract_section_text(heading_el: lxml.html.HtmlElement) -> str:
    tag_level = int(heading_el.tag[1])
    lines = []
    header_text = " ".join(heading_el.text_content().split()).rstrip("¶").strip()
    anchor = heading_el.get("id", "")
    lines.append(f"# {header_text} (#{anchor})\n")

    curr = heading_el.getnext()
    while curr is not None:
        if isinstance(curr.tag, str) and re.match(r"^h[2-6]$", curr.tag):
            curr_level = int(curr.tag[1])
            if curr_level <= tag_level:
                break
        if isinstance(curr.tag, str):
            text = " ".join(curr.text_content().split())
            if text:
                if curr.tag in ("ol", "ul"):
                    for idx, li in enumerate(curr.xpath(".//li"), start=1):
                        li_text = " ".join(li.text_content().split())
                        lines.append(f"  {idx}. {li_text}")
                elif curr.tag == "dl":
                    for child in curr.iterdescendants():
                        if not isinstance(child.tag, str):
                            continue
                        c_text = " ".join(child.text_content().split())
                        if child.tag == "dt":
                            lines.append(f"\n  * {c_text}")
                        elif child.tag == "dd":
                            lines.append(f"      -> {c_text}")
                else:
                    lines.append(text)
        curr = curr.getnext()
    return "\n".join(lines)


def cmd_section(args: argparse.Namespace) -> int:
    page = args.page if args.page.endswith(".html") else f"{args.page}.html"
    path = ensure_cached(page)
    headings = parse_headings(path)
    query = args.query.strip().lstrip("#").lower()

    matched: Optional[lxml.html.HtmlElement] = None
    # 1. Exact match on secno or anchor_id
    for secno, anchor_id, title, h in headings:
        if secno.lower() == query or anchor_id.lower() == query:
            matched = h
            break
    # 2. Substring match on anchor_id or title
    if matched is None:
        for secno, anchor_id, title, h in headings:
            if query in anchor_id.lower() or query in title.lower():
                matched = h
                break

    if matched is None:
        print(f"Error: No section matching '{args.query}' found in {page}.", file=sys.stderr)
        return 1

    print(extract_section_text(matched))
    return 0


def cmd_errors(args: argparse.Namespace) -> int:
    path = ensure_cached("parsing.html")
    raw = path.read_text(encoding="utf-8", errors="replace")
    doc = lxml.html.fromstring(raw)
    codes = []
    for dt in doc.xpath('//dfn[starts-with(@id, "parse-error-")]'):
        code = dt.text_content().strip()
        anchor = dt.get("id", "")
        codes.append((code, anchor))
    if not codes:
        # Fallback regex for id="parse-error-..."
        for m in re.finditer(r'id=["\']?(parse-error-([a-z0-9-]+))["\'\s>]', raw):
            codes.append((m.group(2), m.group(1)))

    seen = set()
    unique_codes = []
    for code, anchor in codes:
        if code not in seen:
            seen.add(code)
            unique_codes.append((code, anchor))

    for idx, (code, anchor) in enumerate(unique_codes, start=1):
        print(f"{idx:2d}. {code:<62} (#{anchor})")
    print(f"\nTotal normative parse error codes: {len(unique_codes)}")
    return 0


def cmd_entities(args: argparse.Namespace) -> int:
    path = ensure_cached("entities.json")
    data = json.loads(path.read_text(encoding="utf-8"))
    q = (args.query or "").strip()
    count = 0
    for name, info in data.items():
        if q and q.lower() not in name.lower():
            continue
        cps = info.get("codepoints", [])
        chars = repr(info.get("characters", ""))
        if args.limit and count >= args.limit:
            break
        print(f"{name:<32} codepoints={cps!s:<20} characters={chars}")
        count += 1
    print(f"\nDisplayed {count} / {len(data)} total named character reference entries.")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="whatwg_spec.py",
        description="WHATWG HTML/DOM & W3C CSS Specification Fetcher and Normative Extractor.",
    )
    sub = parser.add_subparsers(dest="subcommand", required=True)

    p_fetch = sub.add_parser("fetch", help="Download and cache WHATWG/W3C specification files.")
    p_fetch.add_argument("--all", action="store_true", help="Fetch all specs including DOM and CSS Syntax 3.")
    p_fetch.add_argument("--force", action="store_true", help="Re-download even if cached.")
    p_fetch.set_defaults(func=cmd_fetch)

    p_toc = sub.add_parser("toc", help="List table of contents (section numbers, anchors, titles).")
    p_toc.add_argument("--page", default="parsing.html", help="Spec page name (default: parsing.html).")
    p_toc.add_argument("--filter", default=None, help="Substring filter on section number/anchor/title.")
    p_toc.set_defaults(func=cmd_toc)

    p_sec = sub.add_parser("section", help="Extract normative algorithm text for a section number or anchor.")
    p_sec.add_argument("query", help="Section number (e.g. 13.2.5.1), anchor (e.g. #data-state), or title query.")
    p_sec.add_argument("--page", default="parsing.html", help="Spec page name (default: parsing.html).")
    p_sec.set_defaults(func=cmd_section)

    p_err = sub.add_parser("errors", help="List all 52 normative WHATWG HTML parse error codes from §13.2.2.")
    p_err.set_defaults(func=cmd_errors)

    p_ent = sub.add_parser("entities", help="Inspect cached WHATWG entities.json named character references.")
    p_ent.add_argument("--query", default=None, help="Filter entity names by substring.")
    p_ent.add_argument("--limit", type=int, default=25, help="Max entries to print (0 for all).")
    p_ent.set_defaults(func=cmd_entities)

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
