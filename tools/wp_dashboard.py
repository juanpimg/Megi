"""Work-package dashboard with counts taken from real artifacts (spec v2 §4.3).

Closed work packages are counted from git commit subjects containing a WP id;
queued work is counted from the generated TOML files. No number is estimated.
"""

from __future__ import annotations

import argparse
import html
import re
import subprocess
import sys
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools.wp_schema import validate_wp  # noqa: E402

DEFAULT_ROOT = Path(__file__).resolve().parent.parent
OUTPUT = Path("docs/status/work-packages.html")
WP_ID = re.compile(r"\bWP-[A-Z0-9]+-[A-Za-z0-9._-]+\b")


def _git(root: Path, *args: str) -> str:
    result = subprocess.run(["git", "-C", str(root), *args],
                            capture_output=True, text=True, check=False)
    return result.stdout if result.returncode == 0 else ""


def collect(root: Path) -> dict:
    subjects = _git(root, "log", "--format=%s").splitlines()
    mentioned = {wid for subject in subjects for wid in WP_ID.findall(subject)}
    path = root / "docs/work-packages/closed.toml"
    records = {}
    if path.is_file():
        with path.open("rb") as handle:
            records = tomllib.load(handle)
    # A commit subject alone does not close a WP: it must have been closed with
    # a recorded metric movement and a commit that mentions its ID.
    closed_ids = sorted(
        wid for wid, fields in records.items()
        if wid in mentioned and str(fields.get("metric", "")).strip()
    )
    queue = root / "docs/work-packages/queue"
    wps = []
    if queue.is_dir():
        for path in sorted(queue.glob("*.toml")):
            with path.open("rb") as handle:
                wps.append(validate_wp(tomllib.load(handle)))
    head = _git(root, "rev-parse", "--short", "HEAD").strip()
    return {
        "closed": len(closed_ids),
        "closed_ids": sorted(closed_ids),
        "queued": len(wps),
        "light": sum(1 for wp in wps if wp.profile == "light"),
        "heavy": sum(1 for wp in wps if wp.profile == "heavy"),
        "head": head,
    }


def render(data: dict) -> str:
    rows = [
        ("Commits con WP-ID (cerrados)", data["closed"]),
        ("Work packages en cola", data["queued"]),
        ("Perfil light (poco disco)", data["light"]),
        ("Perfil heavy (/build completo)", data["heavy"]),
    ]
    lines = [
        "<!doctype html>",
        '<html lang="es"><head><meta charset="utf-8">',
        "<title>Megi — work packages</title></head><body>",
        "<h1>Megi — work packages</h1>",
        "<p>Contado desde mensajes de commit y archivos de la cola; sin estimaciones.</p>",
        "<table><thead><tr><th>Métrica</th><th>Valor</th></tr></thead><tbody>",
    ]
    lines += [f"<tr><td>{html.escape(label)}</td><td>{value}</td></tr>" for label, value in rows]
    lines.append("</tbody></table>")
    lines.append("<h2>Work packages cerrados</h2><ul>")
    lines += [f"<li>{html.escape(wid)}</li>" for wid in data["closed_ids"]] or ["<li>(none)</li>"]
    lines.append("</ul>")
    lines.append("<p><small>Generado por tools/wp_dashboard.py desde closed.toml y la cola real.</small></p>")
    lines.append("</body></html>")
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Generate the work-package dashboard")
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    args = parser.parse_args(argv)
    content = render(collect(args.root)).encode("utf-8")
    path = args.root / OUTPUT
    if args.check:
        if not path.is_file() or path.read_bytes() != content:
            print(f"stale: {OUTPUT} — run python3 tools/wp_dashboard.py --write")
            return 1
        print("work-package dashboard is up to date")
        return 0
    if args.write or not args.check:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
        print(f"wrote {OUTPUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
