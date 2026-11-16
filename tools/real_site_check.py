#!/usr/bin/env python3
"""Render a curated list of real websites and produce a visual report.

Default mode renders versioned HTML snapshots (deterministic, no network). Pass --live to
fetch the real URLs through Megi's own `render` pipeline (TLS, CSS, images, JS included).

    python3 tools/real_site_check.py            # snapshots
    python3 tools/real_site_check.py --live     # real network
"""

from __future__ import annotations

import argparse
import html
import json
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    print("Python 3.11+ required (tomllib).", file=sys.stderr)
    sys.exit(1)

STATUS_DIR = Path("docs/status")
SITES_MANIFEST = STATUS_DIR / "real_sites.toml"
SNAPSHOT_DIR = Path("tests/fixtures/real-sites")
PNG_DIR = STATUS_DIR / "real-sites"
ISSUE_BASE = "https://github.com/juanpimg/Megi/issues/"


def load_sites(root: Path) -> list[dict]:
    path = root / SITES_MANIFEST
    try:
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        sys.exit(f"error: manifest not found: {path}")
    return list(manifest.get("site", []))


def classify(stats: dict, site: dict | None = None) -> str:
    """Classify one render result. Blank is a stronger signal than degraded."""
    try:
        items = int(stats.get("display_items", 0))
    except (TypeError, ValueError):
        items = 0
    if items <= 0:
        return "PÁGINA EN BLANCO"
    try:
        status = int(stats.get("status") or 200)
    except (TypeError, ValueError):
        status = 200
    warnings = stats.get("warnings") or []
    degraded = (
        bool(warnings)
        or status >= 400
        or int(stats.get("images_failed", 0) or 0) > 0
        or int(stats.get("scripts_failed", 0) or 0) > 0
    )
    if site and "min_items" in site and items < int(site["min_items"]):
        degraded = True
    return "DEGRADADO" if degraded else "PASS"


def _json_from(stdout: str) -> dict:
    for line in reversed(stdout.splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return {}


def render_one(root: Path, site: dict, live: bool) -> dict:
    site_id = str(site["id"])
    if live:
        source = str(site["url"])
    else:
        source = str(root / SNAPSHOT_DIR / str(site.get("snapshot", f"{site_id}.html")))
    png_rel = PNG_DIR / f"{site_id}.png"
    png_path = root / png_rel
    png_path.parent.mkdir(parents=True, exist_ok=True)
    command = [
        "cargo",
        "run",
        "-q",
        "-p",
        "render",
        "--",
        source,
        str(png_path),
        "--width",
        "1024",
        "--height",
        "768",
        "--json",
    ]
    try:
        process = subprocess.run(
            command, cwd=root, capture_output=True, text=True, timeout=600, check=False
        )
    except subprocess.TimeoutExpired:
        return _result(site, "ERROR", {}, "timeout al renderizar", png_rel, source)
    if process.returncode != 0:
        detail = (process.stderr or process.stdout).strip().splitlines()
        message = detail[-1] if detail else "error al renderizar"
        return _result(site, "ERROR", {}, message, png_rel, source)
    stats = _json_from(process.stdout)
    return _result(site, classify(stats, site), stats, "", png_rel, source)


def _result(site: dict, result: str, stats: dict, error: str, png_rel: Path, source: str) -> dict:
    return {
        "id": site["id"],
        "name": site.get("name", site["id"]),
        "note": site.get("note", ""),
        "source": source,
        "mode": "live" if source.startswith(("http://", "https://")) else "snapshot",
        "result": result,
        "display_items": stats.get("display_items", 0),
        "status": stats.get("status"),
        "warnings": list(stats.get("warnings") or []),
        "images_failed": stats.get("images_failed", 0),
        "time_ms": round(float((stats.get("timings_us") or {}).get("total", 0)) / 1000, 1),
        "issue": site.get("issue", 0),
        "error": error,
        "png": png_rel.as_posix(),
    }


def render_html(results: list[dict], mode: str = "snapshot") -> str:
    counts = {key: sum(1 for item in results if item["result"] == key) for key in ("PASS", "DEGRADADO", "PÁGINA EN BLANCO", "ERROR")}
    rows = []
    for item in results:
        result = item["result"]
        color = "#2ea043" if result == "PASS" else ("#d29922" if result == "DEGRADADO" else "#da3633")
        issue = item.get("issue", 0)
        issue_link = f'<a href="{ISSUE_BASE}{issue}">#{issue}</a>' if issue else "—"
        warnings = "<br>".join(html.escape(str(w)) for w in item["warnings"][:4]) or "—"
        note = html.escape(str(item.get("note", "")))
        rows.append(
            f"<tr><td><b>{html.escape(str(item['name']))}</b><br><span class='meta'>{note}</span></td>"
            f"<td style='color:{color}'><b>{result}</b></td>"
            f"<td>{item.get('display_items', 0)}</td><td>{item.get('time_ms', 0)} ms</td>"
            f"<td>{warnings}</td><td>{issue_link}</td>"
            f"<td><a href='{html.escape(str(item['png']))}'><img src='{html.escape(str(item['png']))}' "
            f"alt='captura de {html.escape(str(item['name']))}' style='width:220px;border:1px solid #d0d7de'></a></td></tr>"
        )
    summary = " · ".join(f"{key}: {value}" for key, value in counts.items())
    return f"""<!DOCTYPE html>
<html lang="es"><head><meta charset="utf-8"><title>Megi — render en webs reales</title>
<style>
body{{font:14px/1.5 -apple-system,Segoe UI,Helvetica,Arial,sans-serif;color:#1f2328;max-width:1200px;margin:2rem auto;padding:0 1rem}}
h1{{font-size:26px}} table{{border-collapse:collapse;width:100%}} th,td{{border:1px solid #d0d7de;padding:8px;text-align:left;vertical-align:top}}
th{{background:#f6f8fa}} .meta{{color:#57606a;font-size:12px}}
.note{{background:#fff8c5;border-left:4px solid #d29922;padding:10px 14px;margin:1rem 0}}
</style></head><body>
<h1>Megi — prueba real de render</h1>
<p class="meta">Modo: <b>{html.escape(mode)}</b> · {summary} ·
generado por <code>tools/real_site_check.py</code> desde <code>docs/status/real_sites.toml</code>.</p>
<div class="note">Esto es una comprobación de render de extremo a extremo, no una métrica de
conformidad. Una página en blanco (<code>display_items = 0</code>) o avisos de imagen/layout se
marcan como fallo aunque el pipeline haya terminado sin error. Las capturas son PNG reales
producidos por <code>render</code>. Para medir cobertura de capacidades, ver
<a href="coverage.html">coverage.html</a>.</p>
<table><tr><th>Sitio</th><th>Resultado</th><th>display_items</th><th>Tiempo</th><th>Avisos</th><th>Issue</th><th>Captura</th></tr>
{''.join(rows)}</table>
<p class="meta">Datos: <a href="real-sites.json">real-sites.json</a></p>
</body></html>
"""


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Real-site render check for Megi.")
    parser.add_argument("--live", action="store_true", help="fetch real URLs instead of snapshots")
    parser.add_argument("--root", default=None)
    args = parser.parse_args(argv)
    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parent.parent

    results = [render_one(root, site, args.live) for site in load_sites(root)]
    payload = {
        "mode": "live" if args.live else "snapshot",
        "results": results,
    }
    json_path = root / STATUS_DIR / "real-sites.json"
    html_path = root / STATUS_DIR / "real-sites.html"
    json_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    html_path.write_text(render_html(results, payload["mode"]), encoding="utf-8")
    for item in results:
        print(f"{item['result']:>18}  {item['name']}  ({item['display_items']} items)")
    print(f"wrote {json_path.relative_to(root)} and {html_path.relative_to(root)}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
