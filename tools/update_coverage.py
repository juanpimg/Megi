#!/usr/bin/env python3
"""Generate Megi's capability-coverage status from a verifiable manifest.

The manifest `docs/status/coverage.toml` declares *known* capabilities. A capability
counts as implemented only when its `evidence` can be checked in this repository:

    gate:<suite>                     suite in [gate] of progress.toml, gate PASS, counts equal
    test:<path>::<fn>                the source file exists and contains `fn <fn>`
    fixture:<path>                   the golden/fixture file exists
    none                             only for partial/missing

`done` without verifiable evidence is an error, not a silent estimate. Adding a new
capability grows the denominator, so the percentage can legitimately go down — that
is the point of the measure (see docs/status/COVERAGE.md).

Usage:
    python3 tools/update_coverage.py --write
    python3 tools/update_coverage.py --check
"""

from __future__ import annotations

import argparse
import html
import json
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    print("Python 3.11+ required (tomllib).", file=sys.stderr)
    sys.exit(1)

STATUS_DIR = Path("docs/status")
MANIFEST = STATUS_DIR / "coverage.toml"
PROGRESS_MANIFEST = STATUS_DIR / "progress.toml"
ALLOWED_STATES = {"done", "partial", "missing"}
AREA_LABELS = {
    "F1": "HTML, DOM y serialización",
    "F2": "CSS, cascada y selectores",
    "F3": "Layout (BFC/IFC, grid, flex, tablas)",
    "F4": "Paint, raster, fuentes y SVG",
    "F5": "Red y fetch",
    "F6": "JavaScript y APIs DOM",
    "F7": "Shell de escritorio",
    "X": "Transversal (bench, fuzz, CI)",
}
FONT = "DejaVu Sans, Verdana, sans-serif"
ISSUE_BASE = "https://github.com/juanpimg/Megi/issues/"


def run(root: Path, args: list[str], timeout: int = 60) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=root, capture_output=True, text=True, timeout=timeout, check=False)


def git_short_head(root: Path) -> str:
    return run(root, ["git", "rev-parse", "--short", "HEAD"]).stdout.strip() or "uncommitted"


def git_commit_date(root: Path) -> str:
    return run(root, ["git", "log", "-1", "--format=%cs"]).stdout.strip() or "unknown"


def load_manifest(path: Path) -> dict:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        sys.exit(f"error: manifest not found: {path} (run from the repo root)")
    except tomllib.TOMLDecodeError as error:
        sys.exit(f"error: invalid TOML in {path}: {error}")


def read_gate(root: Path) -> dict[str, str]:
    progress = load_manifest(root / PROGRESS_MANIFEST)
    return dict(progress.get("gate", {}))


def _strip_rust_comments_and_strings(source: str) -> str:
    """Blank out Rust line/block comments and string/char literals, preserving offsets."""
    out = list(source)
    n = len(source)
    i = 0
    in_comment = None  # "//" or "/*"
    quote = None
    while i < n:
        if in_comment == "//":
            if source[i] == "\n":
                in_comment = None
            else:
                out[i] = " "
            i += 1
            continue
        if in_comment == "/*":
            if source.startswith("*/", i):
                out[i] = out[i + 1] = " "
                i += 2
                in_comment = None
            else:
                if source[i] != "\n":
                    out[i] = " "
                i += 1
            continue
        if quote:
            if source[i] == "\\":
                out[i] = " "
                if i + 1 < n and source[i + 1] != "\n":
                    out[i + 1] = " "
                i += 2
                continue
            if source[i] == quote:
                out[i] = " "
                quote = None
            else:
                if source[i] != "\n":
                    out[i] = " "
            i += 1
            continue
        if source.startswith("//", i):
            out[i] = out[i + 1] = " "
            in_comment = "//"
            i += 2
        elif source.startswith("/*", i):
            out[i] = out[i + 1] = " "
            in_comment = "/*"
            i += 2
        elif source[i] == "r" and i + 1 < n and source[i + 1] == '"':
            # Raw string r"..." (rarely used); blank until the closing quote.
            out[i] = out[i + 1] = " "
            j = i + 2
            while j < n:
                if source[j] == '"':
                    out[j] = " "
                    j += 1
                    break
                if source[j] != "\n":
                    out[j] = " "
                j += 1
            i = j
        elif source[i] == "r" and (m_hash := re.match(r"r(#+)\"", source[i:])):
            # Raw string r##"..."##: skip exactly the matching number of hashes.
            hashes = m_hash.group(1)
            closing = '"' + hashes
            start_quote = i + 1 + len(hashes)
            for j in range(i, start_quote + 1):
                out[j] = " "
            j = start_quote + 1
            while j < n:
                if source.startswith(closing, j):
                    for k in range(j, j + len(closing)):
                        out[k] = " "
                    j += len(closing)
                    break
                if source[j] != "\n":
                    out[j] = " "
                j += 1
            i = j
        elif source[i] == '"':
            # Only double-quoted strings; `'` is used for Rust lifetimes/chars and must not
            # open a string state (that would swallow the rest of the file).
            out[i] = " "
            quote = '"'
            i += 1
        else:
            i += 1
    return "".join(out)


def verify_evidence(cap: dict, root: Path, gate: dict[str, str]) -> tuple[bool, str]:
    """Return (verified, reason). `done` requires evidence that machines can check."""
    state = str(cap.get("state", ""))
    evidence = str(cap.get("evidence", "") or "none")
    if state not in ALLOWED_STATES:
        return False, f"estado desconocido: {state!r}"
    if state != "done":
        if evidence not in ("", "none"):
            return False, f"state={state} debe usar evidence=none"
        return True, "no requiere evidencia"
    if evidence in ("", "none"):
        return False, "state=done requiere evidencia verificable (no 'none')"

    kind, _, detail = evidence.partition(":")
    if kind == "gate":
        if str(gate.get("status", "")).upper() != "PASS":
            return False, "el gate de conformidad no está PASS"
        counts = str(gate.get(detail, ""))
        match = re.fullmatch(r"(\d+)\s*/\s*(\d+)", counts)
        if not match:
            return False, f"suite de gate desconocida o sin resultado: {detail!r}"
        if match.group(1) != match.group(2):
            return False, f"suite {detail} tiene fallos: {counts}"
        return True, f"gate {detail} {counts} PASS"

    if kind == "test":
        path_text, _, fn = detail.partition("::")
        path = root / path_text
        if not path.is_file():
            return False, f"no existe el fichero de test {path_text}"
        if not fn:
            return False, "evidence test: falta ::nombre_del_test"
        source = path.read_text(encoding="utf-8", errors="ignore")
        # Strip comments and string literals so a `fn name` mentioned in prose cannot
        # verify a capability (#1391).
        code = _strip_rust_comments_and_strings(source)
        pattern = re.compile(r"fn\s+" + re.escape(fn) + r"\b")
        match = pattern.search(code)
        if not match:
            return False, f"no aparece fn {fn} en {path_text}"
        # A real test needs `#[test]`/`#[tokio::test]`, unless the file lives under a
        # `tests/` directory (integration-test files need no attribute).
        in_tests_dir = "tests" in path.parts
        if not in_tests_dir:
            prefix = code[: match.start()]
            if not re.search(r"#\[(?:tokio::)?test\]\s*(?:#\[[^\]]*\]\s*)*$", prefix):
                return False, f"{fn} no está marcado con #[test] en {path_text}"
        return True, f"test {path_text}::{fn}"

    if kind == "fixture":
        path = root / detail
        if not path.is_file():
            return False, f"no existe el fixture {detail}"
        return True, f"fixture {detail} existe"

    return False, f"tipo de evidencia desconocido: {kind!r}"


def _percent(implemented: int, known: int) -> int:
    if known <= 0:
        return 0
    if implemented >= known:
        return 100
    value = round(implemented * 100 / known)
    return min(99, value)


def compute_stats(caps: list[dict], root: Path, gate: dict[str, str]) -> dict:
    """Enrich each capability with verification and compute per-area and overall counts."""
    enriched: list[dict] = []
    for raw in caps:
        item = dict(raw)
        ok, reason = verify_evidence(raw, root, gate)
        counted = str(item.get("state")) == "done" and ok
        item.update({"verified": ok, "reason": reason, "counted": counted})
        enriched.append(item)

    areas: list[dict] = []
    index: dict[str, dict] = {}
    for item in enriched:
        area_id = str(item.get("area", "?"))
        if area_id not in index:
            area = {
                "id": area_id,
                "implemented": 0,
                "partial": 0,
                "known": 0,
            }
            index[area_id] = area
            areas.append(area)
        area = index[area_id]
        area["known"] += 1
        if item["counted"]:
            area["implemented"] += 1
        elif item.get("state") == "partial":
            area["partial"] += 1
    for area in areas:
        area["percent"] = _percent(area["implemented"], area["known"])

    implemented = sum(1 for item in enriched if item["counted"])
    partial = sum(1 for item in enriched if item.get("state") == "partial")
    known = len(enriched)
    missing = [
        {
            key: item.get(key, "")
            for key in ("id", "area", "name", "source", "state", "evidence", "issue", "reason")
        }
        for item in enriched
        if not item["counted"]
    ]
    unverified = [str(item.get("id")) for item in enriched if item.get("state") == "done" and not item["verified"]]
    invalid_states = [str(item.get("id")) for item in enriched if item.get("state") not in ALLOWED_STATES]

    return {
        "areas": areas,
        "overall": {
            "implemented": implemented,
            "partial": partial,
            "known": known,
            "percent": _percent(implemented, known),
            "unverified": unverified,
            "invalid_states": invalid_states,
        },
        "capabilities": enriched,
        "missing": missing,
    }


def bar(percent: int, width: int = 20) -> str:
    """Full only at 100 %; 99 % stays visibly short of the last block."""
    filled = int(percent * width // 100)
    if percent < 100:
        filled = min(filled, width - 1)
    return "█" * max(0, filled) + "░" * max(0, width - filled)


def phase_color(percent: int) -> str:
    if percent >= 95:
        return "#2ea043"
    if percent >= 85:
        return "#1f6feb"
    if percent >= 70:
        return "#d29922"
    return "#da3633"


def render_svg(stats: dict) -> str:
    overall = stats["overall"]
    areas = stats["areas"]
    width = 900
    top = 150
    row = 52
    footer = 96
    height = top + row * len(areas) + footer
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">',
        "<title id='title'>Megi — cobertura de capacidades</title>",
        "<desc id='desc'>Capacidades implementadas sobre capacidades conocidas, "
        "con evidencia verificable en el repositorio.</desc>",
        f'<rect width="{width}" height="{height}" fill="#ffffff"/>',
        f'<rect x="0.5" y="0.5" width="{width - 1}" height="{height - 1}" fill="none" stroke="#d0d7de"/>',
        f'<text x="28" y="46" font-family="{FONT}" font-size="26" font-weight="700" '
        f'fill="#1f2328">Megi — cobertura de capacidades</text>',
        f'<text x="28" y="68" font-family="{FONT}" font-size="13" fill="#57606a">'
        f'Implementadas / conocidas · inventario verificable (estilo AnyPS5)</text>',
        f'<text x="{width - 28}" y="58" text-anchor="end" font-family="{FONT}" '
        f'font-size="34" font-weight="700" fill="#1f2328">{overall["percent"]}%</text>',
        f'<text x="{width - 28}" y="78" text-anchor="end" font-family="{FONT}" '
        f'font-size="12" fill="#57606a">{overall["implemented"]}/{overall["known"]} capacidades</text>',
        f'<rect x="28" y="92" width="{width - 56}" height="10" rx="5" fill="#d0d7de"/>',
        f'<rect x="28" y="92" width="{round((width - 56) * overall["percent"] / 100)}" height="10" '
        f'rx="5" fill="{phase_color(overall["percent"])}"/>',
    ]
    for index, area in enumerate(areas):
        y = top + index * row
        percent = area["percent"]
        color = phase_color(percent)
        label = html.escape(AREA_LABELS.get(area["id"], area["id"]))
        parts.extend(
            [
                f'<text x="28" y="{y}" font-family="{FONT}" font-size="13" font-weight="700" '
                f'fill="#1f2328">{html.escape(area["id"])}</text>',
                f'<text x="66" y="{y}" font-family="{FONT}" font-size="13" fill="#1f2328">{label}</text>',
                f'<text x="{width - 28}" y="{y}" text-anchor="end" font-family="{FONT}" '
                f'font-size="12" fill="#57606a">'
                f'{area["implemented"]}/{area["known"]}'
                + (f' · {area["partial"]} parcial' if area["partial"] else "")
                + "</text>",
                f'<rect x="28" y="{y + 14}" width="{width - 210}" height="16" rx="8" fill="#eaeef2"/>',
                f'<rect x="28" y="{y + 14}" width="{round((width - 210) * percent / 100)}" '
                f'height="16" rx="8" fill="{color}"/>',
                f'<text x="{width - 28}" y="{y + 27}" text-anchor="end" font-family="{FONT}" '
                f'font-size="13" font-weight="700" fill="{color}">{percent}%</text>',
            ]
        )
    separator = top + row * len(areas) + 8
    parts.extend(
        [
            f'<line x1="28" y1="{separator}" x2="{width - 28}" y2="{separator}" stroke="#d0d7de"/>',
            f'<text x="28" y="{separator + 24}" font-family="{FONT}" font-size="12" fill="#57606a">'
            f'{overall["implemented"]}/{overall["known"]} · {overall["partial"]} parciales · '
            f'{len(stats["missing"])} pendientes. Si añadimos capacidades conocidas, el % baja: es honestidad.</text>',
            f'<text x="28" y="{separator + 46}" font-family="{FONT}" font-size="11" fill="#8c959f">'
            f'Generado por tools/update_coverage.py desde docs/status/coverage.toml — no editar a mano.</text>',
            f'<text x="28" y="{separator + 64}" font-family="{FONT}" font-size="11" fill="#8c959f">'
            f'Legend: verde ≥95% · azul ≥85% · ámbar ≥70% · rojo &lt;70%</text>',
            "</svg>",
        ]
    )
    return "\n".join(parts).rstrip() + "\n"


def render_html(stats: dict, manifest: dict) -> str:
    overall = stats["overall"]
    areas = stats["areas"]
    rows = []
    for area in areas:
        label = html.escape(AREA_LABELS.get(area["id"], area["id"]))
        color = phase_color(area["percent"])
        rows.append(
            f"<tr><td><b>{html.escape(area['id'])}</b> {label}</td>"
            f"<td>{area['implemented']}</td><td>{area['known']}</td>"
            f"<td style='color:{color}'><b>{area['percent']}%</b></td>"
            f"<td><code>{bar(area['percent'])}</code></td></tr>"
        )
    missing_rows = []
    for item in stats["missing"]:
        name = html.escape(str(item.get("name", item.get("id", ""))))
        issue = item.get("issue", 0)
        link = f'<a href="{ISSUE_BASE}{issue}">#{issue}</a>' if issue else "—"
        state = html.escape(str(item.get("state", "")))
        reason = html.escape(str(item.get("reason", "")))
        missing_rows.append(
            f"<tr><td>{html.escape(str(item.get('area', '')))}</td>"
            f"<td><code>{html.escape(str(item.get('id', '')))}</code></td>"
            f"<td>{name}</td><td>{state}</td><td>{link}</td><td>{reason}</td></tr>"
        )
    return f"""<!DOCTYPE html>
<html lang="es"><head><meta charset="utf-8"><title>Megi — cobertura de capacidades</title>
<style>
body{{font:14px/1.5 -apple-system,Segoe UI,Helvetica,Arial,sans-serif;color:#1f2328;max-width:1080px;margin:2rem auto;padding:0 1rem}}
h1{{font-size:28px;margin-bottom:0}} h2{{font-size:20px;margin-top:2rem}}
table{{border-collapse:collapse;width:100%;margin:1rem 0}} th,td{{border:1px solid #d0d7de;padding:6px 10px;text-align:left}}
th{{background:#f6f8fa}} code{{font-family:ui-monospace,monospace}}
.meta{{color:#57606a}} .big{{font-size:44px;font-weight:700;color:{phase_color(overall['percent'])}}}
.note{{background:#fff8c5;border-left:4px solid #d29922;padding:10px 14px;margin:1rem 0}}
</style></head><body>
<h1>Megi — cobertura de capacidades</h1>
<p class="meta">Generado desde <code>docs/status/coverage.toml</code> por <code>tools/update_coverage.py</code>.
Actualizado por: {html.escape(str(manifest.get('last_updated_by', 'unknown')))}</p>
<p><span class="big">{overall['percent']}%</span> &nbsp; <b>{overall['implemented']}/{overall['known']}</b>
capacidades implementadas · {overall['partial']} parciales · {len(stats['missing'])} pendientes</p>
<p><img src="coverage.svg" alt="Gráfica de cobertura" style="max-width:100%"></p>
<div class="note">El denominador son las capacidades <b>declaradas</b> en el inventario. Una capacidad
solo cuenta como implementada si su evidencia (gate, test o fixture) es verificable. Añadir capacidades
conocidas aumenta el denominador y puede bajar el porcentaje: significa que estamos midiendo mejor,
no que el código haya empeorado. Los parciales cuentan como no implementados.</p>
<h2>Por área</h2>
<table><tr><th>Área</th><th>Impl.</th><th>Conocidas</th><th>%</th><th>Barra</th></tr>
{''.join(rows)}</table>
<h2>Aún faltan (o no son verificables)</h2>
<table><tr><th>Área</th><th>Id</th><th>Capacidad</th><th>Estado</th><th>Issue</th><th>Detalle</th></tr>
{''.join(missing_rows)}</table>
<p class="meta">Datos crudos: <a href="coverage.json">coverage.json</a> ·
Reglas: <a href="COVERAGE.md">COVERAGE.md</a> ·
Comprobación visual en webs reales: <a href="real-sites.html">real-sites.html</a></p>
</body></html>
"""


def render_badge(percent: int) -> str:
    value = f"{percent}%"
    label_width = 92
    value_width = max(40, 10 + len(value) * 7)
    color = phase_color(percent)
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{label_width + value_width}" height="20" '
        f'viewBox="0 0 {label_width + value_width} 20" role="img" aria-label="coverage: {value}">\n'
        f"  <title>coverage: {value}</title>\n"
        f'  <rect width="{label_width}" height="20" fill="#555555"/>\n'
        f'  <rect x="{label_width}" width="{value_width}" height="20" fill="{color}"/>\n'
        f'  <g fill="#ffffff" text-anchor="middle" font-family="{FONT}" font-size="11">\n'
        f'    <text x="{label_width / 2:.0f}" y="14">coverage</text>\n'
        f'    <text x="{label_width + value_width / 2:.0f}" y="14">{value}</text>\n'
        f"  </g>\n</svg>\n"
    )


def build_artifacts(root: Path) -> dict[str, bytes]:
    manifest = load_manifest(root / MANIFEST)
    gate = read_gate(root)
    stats = compute_stats(manifest.get("capability", []), root, gate)
    if stats["overall"]["unverified"]:
        sys.exit(
            "error: capacidades done sin evidencia verificable: "
            + ", ".join(stats["overall"]["unverified"])
        )
    if stats["overall"]["invalid_states"]:
        sys.exit(
            "error: estados desconocidos (use done/partial/missing): "
            + ", ".join(stats["overall"]["invalid_states"])
        )
    payload = {
        "schema": 1,
        "overall": stats["overall"],
        "areas": stats["areas"],
        "missing": stats["missing"],
    }
    return {
        str(MANIFEST.with_name("coverage.json")): (
            json.dumps(payload, indent=2, ensure_ascii=False) + "\n"
        ).encode("utf-8"),
        str(MANIFEST.with_name("coverage.svg")): render_svg(stats).encode("utf-8"),
        str(MANIFEST.with_name("coverage.html")): render_html(stats, manifest).encode("utf-8"),
        str(MANIFEST.with_name("badge-coverage.svg")): render_badge(
            stats["overall"]["percent"]
        ).encode("utf-8"),
    }


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Generate Megi's capability coverage (stdlib only).")
    parser.add_argument("--write", action="store_true", help="regenerate artifacts (default)")
    parser.add_argument("--check", action="store_true", help="verify artifacts are up to date")
    parser.add_argument("--root", default=None, help="repository root")
    args = parser.parse_args(argv)
    if args.write and args.check:
        parser.error("--write and --check are mutually exclusive")

    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parent.parent
    artifacts = build_artifacts(root)

    if args.check:
        stale = []
        for relative, content in sorted(artifacts.items()):
            path = root / relative
            if not path.is_file() or path.read_bytes() != content:
                stale.append(relative)
        if stale:
            print("coverage status is stale: " + ", ".join(stale))
            print("Run `python3 tools/update_coverage.py --write` and commit the generated files.")
            return 1
        print("coverage status is up to date")
        return 0

    written = []
    for relative, content in sorted(artifacts.items()):
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        if not path.is_file() or path.read_bytes() != content:
            path.write_bytes(content)
            written.append(relative)
    overall = artifacts[str(MANIFEST.with_name("coverage.json"))].decode()
    import json

    percent = json.loads(overall)["overall"]
    print(
        f"coverage: {percent['implemented']}/{percent['known']} = {percent['percent']}% "
        f"({percent['partial']} partial)"
    )
    if written:
        print("wrote " + ", ".join(written))
    else:
        print("artifacts already up to date")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
