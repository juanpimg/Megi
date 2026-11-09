#!/usr/bin/env python3
"""
Megi Engine — html5lib-tests & WPT Fixture Inspector and JSON Exporter.

Inspects, summarizes, displays individual test cases, and exports `.dat`
tree-construction files into structured JSON fixtures so `test_harness` can
consume both `.test` (JSON) and `.dat` / `.json` tree-construction suites.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Dict, List

REPO_ROOT = Path(__file__).resolve().parent.parent
HTML5LIB_DIR = REPO_ROOT / "tests" / "html5lib-tests"
TOKENIZER_DIR = HTML5LIB_DIR / "tokenizer"
TREE_DAT_DIR = HTML5LIB_DIR / "tree-construction"
TREE_JSON_DIR = HTML5LIB_DIR / "tree-construction-json"

KNOWN_DAT_HEADINGS = {
    "description",
    "data",
    "errors",
    "new-errors",
    "errors-new",
    "document-fragment",
    "script-off",
    "script-on",
    "document",
}


def parse_dat_file(dat_path: Path) -> List[Dict[str, Any]]:
    """
    Exact line-by-line state machine matching WPT `html/syntax/parsing/resources/test.js` `parseDat()`.
    Handles multi-line text nodes containing blank lines (`\\n\\n`) inside `#data` or `#document`.
    """
    raw = dat_path.read_text(encoding="utf-8", errors="surrogateescape")
    lines = raw.split("\n")
    if lines and lines[-1] == "":
        lines.pop()

    test_Wait_list: List[Dict[str, str]] = []
    current_data: Dict[str, str] = {}
    key: str = ""

    for line in lines:
        if line.startswith("#") and line[1:].strip() in KNOWN_DAT_HEADINGS:
            heading = line[1:].strip()
            if (heading == "data" and key != "description") or heading == "description":
                if key:
                    if current_data.get(key, "").endswith("\n"):
                        current_data[key] = current_data[key][:-1]
                    test_Wait_list.append(current_data)
                    current_data = {}
            key = heading
            current_data[key] = ""
        elif key:
            current_data[key] += line + "\n"

    if key:
        test_Wait_list.append(current_data)

    structured: List[Dict[str, Any]] = []
    for idx, td in enumerate(test_Wait_list):
        norm: Dict[str, str] = {}
        for k, v in td.items():
            norm[k] = v[:-1] if v.endswith("\n") else v

        errors_list = [e for e in norm.get("errors", "").split("\n") if e]
        new_errors_list = [e for e in norm.get("new-errors", norm.get("errors-new", "")).split("\n") if e]
        fragment = norm.get("document-fragment")
        script_mode = "both"
        if "script-off" in norm:
            script_mode = "script-off"
        elif "script-on" in norm:
            script_mode = "script-on"

        structured.append(
            {
                "index": idx,
                "description": norm.get("description"),
                "data": norm.get("data", ""),
                "errors": errors_list,
                "new_errors": new_errors_list,
                "document_fragment": fragment,
                "script_mode": script_mode,
                "document": norm.get("document", ""),
            }
        )
    return structured


def load_tokenizer_file(test_path: Path) -> List[Dict[str, Any]]:
    data = json.loads(test_path.read_text(encoding="utf-8"))
    if "tests" in data:
        return data["tests"]
    if "xmlViolationTests" in data:
        return data["xmlViolationTests"]
    return []


def cmd_summary(args: argparse.Namespace) -> int:
    print("=== Tokenizer JSON Suites (tests/html5lib-tests/tokenizer/*.test) ===")
    tok_files = sorted(TOKENIZER_DIR.glob("*.test")) if TOKENIZER_DIR.exists() else []
    tok_total = 0
    for tf in tok_files:
        cases = load_tokenizer_file(tf)
        tok_total += len(cases)
        print(f"  {tf.name:<34} {len(cases):5d} tests")
    print(f"Total tokenizer test files: {len(tok_files)} ({tok_total} test objects)\n")

    print("=== Tree Construction Suites (tests/html5lib-tests/tree-construction/*.dat) ===")
    dat_files = sorted(TREE_DAT_DIR.glob("*.dat")) if TREE_DAT_DIR.exists() else []
    dat_total = 0
    frag_total = 0
    for df in dat_files:
        cases = parse_dat_file(df)
        frags = sum(1 for c in cases if c["document_fragment"] is not None)
        dat_total += len(cases)
        frag_total += frags
        print(f"  {df.name:<44} {len(cases):4d} tests (fragments: {frags:2d})")
    print(f"Total tree-construction .dat files: {len(dat_files)} ({dat_total} tests, {frag_total} fragment cases)")
    return 0


def cmd_show(args: argparse.Namespace) -> int:
    target = Path(args.file)
    if not target.exists():
        for d in (TOKENIZER_DIR, TREE_DAT_DIR, TREE_JSON_DIR):
            cand = d / args.file
            if cand.exists():
                target = cand
                break
    if not target.exists():
        print(f"Error: fixture file '{args.file}' not found.", file=sys.stderr)
        return 1

    if target.suffix == ".test":
        cases = load_tokenizer_file(target)
        if args.index < 0 or args.index >= len(cases):
            print(f"Error: index {args.index} out of range (0..{len(cases) - 1}).", file=sys.stderr)
            return 1
        print(json.dumps(cases[args.index], indent=2, ensure_ascii=False))
    elif target.suffix == ".dat":
        cases = parse_dat_file(target)
        if args.index < 0 or args.index >= len(cases):
            print(f"Error: index {args.index} out of range (0..{len(cases) - 1}).", file=sys.stderr)
            return 1
        print(json.dumps(cases[args.index], indent=2, ensure_ascii=False))
    elif target.suffix == ".json":
        data = json.loads(target.read_text(encoding="utf-8"))
        cases = data.get("tests", [])
        if args.index < 0 or args.index >= len(cases):
            print(f"Error: index {args.index} out of range (0..{len(cases) - 1}).", file=sys.stderr)
            return 1
        print(json.dumps(cases[args.index], indent=2, ensure_ascii=False))
    else:
        print(f"Unsupported fixture extension: {target.suffix}", file=sys.stderr)
        return 1
    return 0


def sanitize_surrogates(s: Optional[str]) -> Optional[str]:
    if s is None:
        return None
    # Replace any raw surrogate code points (from surrogateescape) with \uFFFD or escape sequence
    out = []
    for ch in s:
        cp = ord(ch)
        if 0xD800 <= cp <= 0xDFFF:
            out.append(f"\\u{cp:04X}")
        else:
            out.append(ch)
    return "".join(out)


def cmd_export_json(args: argparse.Namespace) -> int:
    if not TREE_DAT_DIR.exists():
        print(f"Error: {TREE_DAT_DIR} does not exist.", file=sys.stderr)
        return 1
    TREE_JSON_DIR.mkdir(parents=True, exist_ok=True)
    dat_files = sorted(TREE_DAT_DIR.glob("*.dat"))
    total_cases = 0
    for df in dat_files:
        cases = parse_dat_file(df)
        for c in cases:
            c["data"] = sanitize_surrogates(c["data"])
            c["document"] = sanitize_surrogates(c["document"])
            c["description"] = sanitize_surrogates(c["description"])
        total_cases += len(cases)
        out_path = TREE_JSON_DIR / f"{df.stem}.json"
        payload = {
            "source_file": df.name,
            "tests": cases,
        }
        out_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(
        f"[OK] Exported {len(dat_files)} .dat files ({total_cases} tests) -> "
        f"{TREE_JSON_DIR.relative_to(REPO_ROOT)}/*.json"
    )
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="test_inspector.py",
        description="Inspect and convert html5lib-tests and WPT HTML parsing fixtures.",
    )
    sub = parser.add_subparsers(dest="subcommand", required=True)

    p_sum = sub.add_parser("summary", help="Print summary table of all tokenizer and tree-construction test files.")
    p_sum.set_defaults(func=cmd_summary)

    p_show = sub.add_parser("show", help="Display a single test case by file name and 0-based index.")
    p_show.add_argument("file", help="Fixture filename (e.g. test1.test or doctype01.dat).")
    p_show.add_argument("index", type=int, help="0-based test index.")
    p_show.set_defaults(func=cmd_show)

    p_exp = sub.add_parser(
        "export-json",
        help="Convert tree-construction/*.dat files into JSON fixtures in tree-construction-json/*.json.",
    )
    p_exp.set_defaults(func=cmd_export_json)

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
