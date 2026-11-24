"""Seven WP generators fed by real artifacts (spec §4.3).

A generator never invents work: with no artifact it returns an empty list. Each
work package carries a `profile` derived from its verification command so the
two build machines (one with little disk) can pick suitable work.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools.wp_schema import WorkPackage, validate_wp  # noqa: E402

SOURCES = ("wpt", "js262", "site", "fuzz", "audit", "depth", "user")
QUEUE = Path("docs/work-packages/queue")
USER_LABELS = {"crash", "broken_site"}


def slug(value: str) -> str:
    text = re.sub(r"[^a-z0-9]+", "-", str(value).lower()).strip("-")
    return (text or "item")[:60].rstrip("-")


def _artifact(root: Path, relative: str):
    path = root / relative
    if not path.is_file():
        return None
    return path


def _wp(source: str, key: str, *, file: str, expected: str, actual: str,
        spec: str, acceptance: str, verify_cmd: str, metric: str,
        milestone: str, impact: int = 1, frequency: int = 1) -> WorkPackage:
    return validate_wp({
        "id": f"WP-{source.upper()}-{slug(key)}",
        "source": source,
        "file": file,
        "expected": expected,
        "actual": actual or "not verified",
        "spec": spec,
        "acceptance": acceptance,
        "verify_cmd": verify_cmd,
        "metric": metric,
        "milestone": milestone,
        "impact": impact,
        "frequency": frequency,
    })


def _json(root: Path, relative: str):
    path = _artifact(root, relative)
    return json.loads(path.read_text(encoding="utf-8")) if path else None


def generate_wpt(root: Path) -> list[WorkPackage]:
    data = _json(root, "tests/wpt/results/latest.json")
    if not data:
        return []
    verify = "cargo run --release -p megi-test-harness -- --summary"
    out = []
    for failure in data.get("failures", []):
        test = str(failure.get("test", "unknown"))
        subtest = str(failure.get("subtest", ""))
        out.append(_wp(
            "wpt", f"{test}-{subtest}",
            file=str(failure.get("file", f"tests/wpt/{test}")),
            expected=str(failure.get("expected", "PASS")),
            actual=str(failure.get("actual", "FAIL")),
            spec="spec v2 §2 — WPT allowlist = 0",
            acceptance=f"{test} {subtest} pasa sin allowlist",
            verify_cmd=verify, metric="wpt_passed",
            milestone=str(failure.get("milestone", "M0")),
            impact=5, frequency=4,
        ))
    return out


def generate_js262(root: Path) -> list[WorkPackage]:
    data = _json(root, "tests/test262/results/latest.json")
    if not data:
        return []
    out = []
    for failure in data.get("failures", []):
        test = str(failure.get("test", "unknown"))
        out.append(_wp(
            "js262", test, file=str(failure.get("file", f"tests/test262/{test}")),
            expected=str(failure.get("expected", "PASS")),
            actual=str(failure.get("actual", "FAIL")),
            spec="spec v2 §2 — Test262 allowlist = 0",
            acceptance=f"{test} pasa en el subconjunto declarado",
            verify_cmd="cargo test -p megi-js", metric="js262_passed",
            milestone=str(failure.get("milestone", "M8")),
            impact=4, frequency=3,
        ))
    return out


def generate_site(root: Path) -> list[WorkPackage]:
    data = _json(root, "docs/status/real-sites.json")
    if not data:
        return []
    out = []
    for site in data.get("results", []):
        if str(site.get("result", "")).upper() == "PASS":
            continue
        warnings = site.get("warnings") or []
        sid = str(site.get("id", "unknown"))
        out.append(_wp(
            "site", sid,
            file=f"tests/fixtures/real-sites/{sid}.html",
            expected="PASS sin warnings degradantes",
            actual="; ".join(str(w)[:200] for w in warnings) or str(site.get("result", "DEGRADADO")),
            spec="spec v2 §2 — 100/100 sitios PASS, sin console_errors",
            acceptance=f"{site.get('name', sid)} sin errores de consola y < 2,5 s",
            verify_cmd=f"cargo run --release -p megi-render -- tests/fixtures/real-sites/{sid}.html --json",
            metric="sites_passed",
            milestone=str(site.get("milestone", "M9")),
            impact=5, frequency=5,
        ))
    return out


def generate_fuzz(root: Path) -> list[WorkPackage]:
    folder = root / "fuzz/artifacts"
    if not folder.is_dir():
        return []
    out = []
    for path in sorted(folder.glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        for crash in data.get("crashes", []):
            target = str(crash.get("target", "unknown"))
            cid = str(crash.get("id", path.stem))
            out.append(_wp(
                "fuzz", f"{target}-{cid}",
                file=str(crash.get("file", f"fuzz/artifacts/{path.name}")),
                expected="sin crashes en campaña de 1 M ejecuciones",
                actual=str(crash.get("note", "crash report artifact")),
                spec="spec v2 §2 — 0 crashes en fuzz",
                acceptance=f"{target}: minimizar, arreglar y añadir regresión",
                verify_cmd="cargo fuzz run", metric="fuzz_crashes",
                milestone=str(crash.get("milestone", "M3")),
                impact=5, frequency=2,
            ))
    return out


def generate_audit(root: Path) -> list[WorkPackage]:
    path = _artifact(root, "docs/audit/coverage.tsv")
    if not path:
        return []
    rows = [line.split("\t") for line in path.read_text(encoding="utf-8").splitlines()]
    if not rows:
        return []
    header = rows[0]
    try:
        status_i = header.index("status")
    except ValueError:
        return []
    out = []
    for cols in rows[1:]:
        if len(cols) <= status_i or cols[status_i] != "pending":
            continue
        file = cols[0]
        out.append(_wp(
            "audit", file, file=file,
            expected="fila revisada y marcada covered",
            actual="pending en el ledger de auditoría",
            spec="spec v2 §5 — cada archivo leído antes de marcarlo cubierto",
            acceptance=f"leer {file} y registrar evidencia",
            verify_cmd="python3 tools/audit_coverage.py --check --strict",
            metric="audit_pending", milestone="M0", impact=2, frequency=2,
        ))
    return out


def generate_depth(root: Path) -> list[WorkPackage]:
    path = _artifact(root, "docs/work-packages/depth-inventory.toml")
    if not path:
        return []
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    out = []
    for entry in data.get("entry", []):
        verify = str(entry["verify_cmd"])
        out.append(_wp(
            "depth", str(entry["id"]), file=str(entry.get("file", "docs/status/progress.toml")),
            expected=str(entry.get("expected", "criterio verificado")),
            actual=str(entry.get("gap", "criterio pendiente")),
            spec=str(entry.get("spec", "spec v2 §4.1")),
            acceptance=str(entry.get("acceptance", "criterio cerrado con evidencia")),
            verify_cmd=verify, metric="depth_closed",
            milestone=str(entry.get("milestone", "M0")),
            impact=int(entry.get("impact", 1)), frequency=int(entry.get("frequency", 1)),
        ))
    return out


def generate_user(root: Path) -> list[WorkPackage]:
    path = _artifact(root, "docs/work-packages/user-reports.json")
    if path:
        reports = json.loads(path.read_text(encoding="utf-8"))
    else:
        try:
            result = subprocess.run(
                ["gh", "issue", "list", "--state", "open", "--json", "number,title,body,labels"],
                capture_output=True, text=True, timeout=30, check=False,
            )
            reports = json.loads(result.stdout) if result.returncode == 0 else []
        except (OSError, ValueError, subprocess.SubprocessError):
            reports = []
    out = []
    for report in reports:
        labels = {str(label).lower() for label in report.get("labels", [])}
        if not labels & USER_LABELS:
            continue
        number = str(report["number"])
        out.append(_wp(
            "user", number, file=f"docs/work-packages/user-reports.json",
            expected="reproducido y corregido, con issue cerrada",
            actual=str(report.get("body", "informe de usuario sin verificar")),
            spec="spec v2 §4.3 — WP-USER de errores de usuario",
            acceptance=str(report.get("title", "repro y arreglo con test")),
            verify_cmd="python3 tools/real_site_check.py",
            metric="user_issues", milestone="M9", impact=3, frequency=3,
        ))
    return out


GENERATORS = {
    "wpt": generate_wpt, "js262": generate_js262, "site": generate_site,
    "fuzz": generate_fuzz, "audit": generate_audit, "depth": generate_depth,
    "user": generate_user,
}


def generate(source: str, dry_run: bool = True, root: Path | None = None) -> list[WorkPackage]:
    if source not in SOURCES:
        raise ValueError(f"unknown WP source: {source}")
    root = root or Path(__file__).resolve().parent.parent
    wps = GENERATORS[source](root)
    if not dry_run:
        queue = root / QUEUE
        queue.mkdir(parents=True, exist_ok=True)
        for wp in wps:
            destination = queue / f"{wp.id}.toml"
            if destination.exists():
                continue
            fields = vars(wp).copy()
            body = "".join(f'{key} = "{str(value).replace(chr(92), chr(92)*2).replace(chr(34), chr(92)+chr(34))}"\n'
                           for key, value in fields.items())
            destination.write_text(body, encoding="utf-8")
    return wps


def main(argv: list[str] | None = None) -> int:
    import argparse
    parser = argparse.ArgumentParser(description="Generate work packages")
    parser.add_argument("source", choices=SOURCES + ("all",), nargs="?", default="all")
    parser.add_argument("--all", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--root", type=Path, default=None)
    args = parser.parse_args(argv)
    selected = "all" if args.all else args.source
    sources = SOURCES if selected == "all" else (selected,)
    total = 0
    for source in sources:
        wps = generate(source, dry_run=args.dry_run, root=args.root)
        total += len(wps)
        print(f"{source}: {len(wps)} work packages")
        for wp in wps:
            print(f"  {wp.id} [{wp.profile}] P{wp.priority} → {wp.metric}")
    print(f"total: {total}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
