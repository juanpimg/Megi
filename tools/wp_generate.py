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
from tools.wpt_gate import load_report as load_wpt_report  # noqa: E402

SOURCES = ("wpt", "js262", "site", "fuzz", "audit", "depth", "user")
QUEUE = Path("docs/work-packages/queue")
USER_LABELS = {"crash", "broken_site", "broken-site"}
# Spec §4.3 calls Test262 packages WP-JS-<key>; other sources use their own name.
ID_PREFIXES = {"js262": "JS"}
WPT_REPORT = "docs/status/wpt.json"           # real report written by tools/wpt_report.py
WPT_LEGACY_REPORT = "tests/wpt/results/latest.json"
# The gate is RED for an unallowlisted failing subtest and GREEN once it passes: spec §4.3 rule 2.
WPT_GATE_VERIFY = (
    "python3 tools/wpt_gate.py --check --report docs/status/wpt.json "
    "--baseline tests/wpt/baseline.json --allowlist tests/wpt/allowlist.toml"
)


def slug(value: str) -> str:
    text = re.sub(r"[^a-z0-9]+", "-", str(value).lower()).strip("-")
    return (text or "item")[:60].rstrip("-")


def _wp(source: str, key: str, *, file: str, expected: str, actual: str,
        spec: str, acceptance: str, verify_cmd: str, metric: str,
        milestone: str, impact: int = 1, frequency: int = 1) -> WorkPackage:
    prefix = ID_PREFIXES.get(source, source.upper())
    return validate_wp({
        "id": f"WP-{prefix}-{slug(key)}",
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


def _artifact(root: Path, relative: str):
    path = root / relative
    if not path.is_file():
        return None
    return path


def _note(issues: list[str] | None, message: str) -> None:
    if issues is not None:
        issues.append(message)


def _json(root: Path, relative: str, issues: list[str] | None = None):
    """Read JSON, reporting malformed artifacts instead of raising."""
    path = _artifact(root, relative)
    if path is None:
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        _note(issues, f"{relative}: {exc}")
        return None


def _entries(data, key: str | None, relative: str,
             issues: list[str] | None = None) -> list[dict]:
    """Return only dict entries from a JSON list, whatever the artifact shape."""
    if data is None:  # no artifact at all is not an error
        return []
    if isinstance(data, dict):
        values = data.get(key, [])
    elif key is None and isinstance(data, list):
        values = data
    else:
        _note(issues, f"{relative}: expected a JSON object containing '{key}'")
        return []
    if not isinstance(values, list):
        _note(issues, f"{relative}: '{key}' must be a list of objects")
        return []
    return [item for item in values if isinstance(item, dict)]


def _text(value) -> str:
    if isinstance(value, (list, tuple)):
        return "; ".join(str(item)[:200] for item in value)
    return str(value) if value is not None else ""


def _safe_wp(issues: list[str] | None, **fields) -> WorkPackage | None:
    """Skip an entry that cannot satisfy the schema; never abort the source."""
    try:
        return _wp(**fields)
    except (ValueError, TypeError, AttributeError) as exc:
        _note(issues, f"invalid entry skipped: {exc}")
        return None


def _dedup_by_id(wps: list[WorkPackage]) -> list[WorkPackage]:
    """Keep the first WP per ID; stable so dry-run output is reproducible."""
    seen: set[str] = set()
    unique: list[WorkPackage] = []
    for wp in wps:
        if wp.id not in seen:
            seen.add(wp.id)
            unique.append(wp)
    return unique


def generate_wpt(root: Path, issues: list[str] | None = None) -> list[WorkPackage]:
    """Feed WPs from the real `megi-wpt-report/1` and the legacy failure feed."""
    out: list[WorkPackage] = []
    relative = WPT_LEGACY_REPORT
    for failure in _entries(_json(root, relative, issues), "failures", relative, issues):
        test = str(failure.get("test", "unknown"))
        subtest = str(failure.get("subtest", ""))
        wp = _safe_wp(
            issues, source="wpt", key=f"{test}-{subtest}",
            file=str(failure.get("file", f"tests/wpt/{test}")),
            expected=str(failure.get("expected", "PASS")),
            actual=str(failure.get("actual", "FAIL")),
            spec="spec v2 §2 — WPT allowlist = 0",
            acceptance=f"{test} {subtest} pasa sin allowlist",
            verify_cmd=WPT_GATE_VERIFY, metric="wpt_passed",
            milestone=str(failure.get("milestone", "M0")),
            impact=5, frequency=4,
        )
        if wp is not None:
            out.append(wp)
    report_path = _artifact(root, WPT_REPORT)
    if report_path:
        try:
            report = load_wpt_report(report_path)  # validates epoch, keys and counts
        except (ValueError, TypeError, OSError) as exc:
            _note(issues, f"{WPT_REPORT}: invalid report: {exc}")
        else:
            for result in report["results"]:
                status = str(result["status"])
                if status == "PASS":
                    continue
                key = str(result["key"])
                subtest = str(result["subtest"])
                wp = _safe_wp(
                    issues, source="wpt", key=f"{key[:12]}-{subtest}",
                    file=str(result["test"]), expected="PASS",
                    actual=f"{status} — {subtest} (key {key[:12]})",
                    spec="spec v2 §2 — WPT allowlist = 0",
                    acceptance=f"{subtest} (key {key[:12]}) pasa sin allowlist",
                    verify_cmd=WPT_GATE_VERIFY, metric="wpt_passed",
                    milestone="M0", impact=5, frequency=4,
                )
                if wp is not None:
                    out.append(wp)
    return _dedup_by_id(out)


def generate_js262(root: Path, issues: list[str] | None = None) -> list[WorkPackage]:
    relative = "tests/test262/results/latest.json"
    data = _json(root, relative, issues)
    out = []
    for failure in _entries(data, "failures", relative, issues):
        test = str(failure.get("test", "unknown"))
        wp = _safe_wp(
            issues,
            source="js262", key=test,
            file=str(failure.get("file", f"tests/test262/{test}")),
            expected=str(failure.get("expected", "PASS")),
            actual=str(failure.get("actual", "FAIL")),
            spec="spec v2 §2 — Test262 allowlist = 0",
            acceptance=f"{test} pasa en el subconjunto declarado",
            verify_cmd="cargo test -p megi-js", metric="js262_passed",
            milestone=str(failure.get("milestone", "M8")),
            impact=4, frequency=3,
        )
        if wp is not None:
            out.append(wp)
    return out


def generate_site(root: Path, issues: list[str] | None = None) -> list[WorkPackage]:
    relative = "docs/status/real-sites.json"
    data = _json(root, relative, issues)
    out = []
    for site in _entries(data, "results", relative, issues):
        if str(site.get("result", "")).upper() == "PASS":
            continue
        sid = str(site.get("id", "unknown"))
        actual = _text(site.get("warnings")) or str(site.get("result", "DEGRADADO"))
        wp = _safe_wp(
            issues,
            source="site", key=sid,
            file=f"tests/fixtures/real-sites/{sid}.html",
            expected="PASS sin warnings degradantes",
            actual=actual,
            spec="spec v2 §2 — 100/100 sitios PASS, sin console_errors",
            acceptance=f"{site.get('name', sid)} sin errores de consola y < 2,5 s",
            verify_cmd=f"cargo run --release -p megi-render -- tests/fixtures/real-sites/{sid}.html --json",
            metric="sites_passed",
            milestone=str(site.get("milestone", "M9")),
            impact=5, frequency=5,
        )
        if wp is not None:
            out.append(wp)
    return out


def generate_fuzz(root: Path, issues: list[str] | None = None) -> list[WorkPackage]:
    folder = root / "fuzz/artifacts"
    if not folder.is_dir():
        return []
    out = []
    for path in sorted(folder.glob("*.json")):
        relative = f"fuzz/artifacts/{path.name}"
        data = _json(root, relative, issues)
        for crash in _entries(data, "crashes", relative, issues):
            target = str(crash.get("target", "unknown"))
            cid = str(crash.get("id", path.stem))
            wp = _safe_wp(
                issues,
                source="fuzz", key=f"{target}-{cid}",
                file=str(crash.get("file", relative)),
                expected="sin crashes en campaña de 1 M ejecuciones",
                actual=str(crash.get("note", "crash report artifact")),
                spec="spec v2 §2 — 0 crashes en fuzz",
                acceptance=f"{target}: minimizar, arreglar y añadir regresión",
                verify_cmd="cargo fuzz run", metric="fuzz_crashes",
                milestone=str(crash.get("milestone", "M3")),
                impact=5, frequency=2,
            )
            if wp is not None:
                out.append(wp)
    return out


def generate_audit(root: Path, issues: list[str] | None = None) -> list[WorkPackage]:
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
        wp = _safe_wp(
            issues,
            source="audit", key=file, file=file,
            expected="fila revisada y marcada covered",
            actual="pending en el ledger de auditoría",
            spec="spec v2 §5 — cada archivo leído antes de marcarlo cubierto",
            acceptance=f"leer {file} y registrar evidencia",
            verify_cmd="python3 tools/audit_coverage.py --check --strict",
            metric="audit_pending", milestone="M0", impact=2, frequency=2,
        )
        if wp is not None:
            out.append(wp)
    return out


def generate_depth(root: Path, issues: list[str] | None = None) -> list[WorkPackage]:
    relative = "docs/work-packages/depth-inventory.toml"
    path = _artifact(root, relative)
    if not path:
        return []
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        _note(issues, f"{relative}: {exc}")
        return []
    out = []
    for entry in _entries(data, "entry", relative, issues):
        wp = _safe_wp(
            issues,
            source="depth", key=str(entry.get("id", "unknown")),
            file=str(entry.get("file", "docs/status/progress.toml")),
            expected=str(entry.get("expected", "criterio verificado")),
            actual=str(entry.get("gap", "criterio pendiente")),
            spec=str(entry.get("spec", "spec v2 §4.1")),
            acceptance=str(entry.get("acceptance", "criterio cerrado con evidencia")),
            verify_cmd=str(entry.get("verify_cmd", "")),
            metric="depth_closed",
            milestone=str(entry.get("milestone", "M0")),
            impact=entry.get("impact", 1), frequency=entry.get("frequency", 1),
        )
        if wp is not None:
            out.append(wp)
    return out


def generate_user(root: Path, issues: list[str] | None = None) -> list[WorkPackage]:
    relative = "docs/work-packages/user-reports.json"
    data = None
    if _artifact(root, relative) is not None:
        data = _json(root, relative, issues)
    else:
        try:
            result = subprocess.run(
                ["gh", "issue", "list", "--state", "open", "--json", "number,title,body,labels"],
                capture_output=True, text=True, timeout=30, check=False,
            )
            if result.returncode == 0:
                data = json.loads(result.stdout)
            else:
                _note(issues, f"gh issue list exited with {result.returncode}")
        except (OSError, ValueError, subprocess.SubprocessError) as exc:
            _note(issues, f"gh issue list: {exc}")
    out = []
    for report in _entries(data, None, "github issues", issues):
        labels_value = report.get("labels") or []
        if not isinstance(labels_value, (list, tuple, set, dict)):
            labels_value = [labels_value]
        labels = {
            str(label.get("name", "")).lower() if isinstance(label, dict) else str(label).lower()
            for label in labels_value
        }
        if not labels & USER_LABELS:
            continue
        wp = _safe_wp(
            issues,
            source="user", key=str(report.get("number", "unknown")),
            file=relative,
            expected="reproducido y corregido, con issue cerrada",
            actual=str(report.get("body", "informe de usuario sin verificar")),
            spec="spec v2 §4.3 — WP-USER de errores de usuario",
            acceptance=str(report.get("title", "repro y arreglo con test")),
            verify_cmd="python3 tools/real_site_check.py",
            metric="user_issues", milestone="M9", impact=3, frequency=3,
        )
        if wp is not None:
            out.append(wp)
    return out


GENERATORS = {
    "wpt": generate_wpt, "js262": generate_js262, "site": generate_site,
    "fuzz": generate_fuzz, "audit": generate_audit, "depth": generate_depth,
    "user": generate_user,
}


def generate(source: str, dry_run: bool = True, root: Path | None = None) -> list[WorkPackage]:
    return _run_source(source, dry_run=dry_run, root=root)


def _run_source(source: str, dry_run: bool, root: Path | None,
                errors: list[str] | None = None) -> list[WorkPackage]:
    if source not in SOURCES:
        raise ValueError(f"unknown WP source: {source}")
    root = root or Path(__file__).resolve().parent.parent
    source_errors: list[str] = []
    try:
        wps = _dedup_by_id(GENERATORS[source](root, source_errors))
    except Exception as exc:  # a malformed artifact must never abort a source
        source_errors.append(f"unexpected generator failure: {exc}")
        wps = []
    sink = errors if errors is not None else []
    for message in source_errors:
        sink.append(f"{source}: {message}")
        print(f"{source}: ERROR {message}")
    if not dry_run:
        queue = root / QUEUE
        queue.mkdir(parents=True, exist_ok=True)
        for wp in wps:
            destination = queue / f"{wp.id}.toml"
            if destination.exists():
                continue
            fields = vars(wp).copy()
            # JSON basic strings are valid TOML basic strings and escape newlines,
            # quotes and backslashes (site warnings contain real newlines).
            body = "".join(
                f"{key} = {value if isinstance(value, int) else json.dumps(str(value), ensure_ascii=False)}\n"
                for key, value in fields.items()
            )
            destination.write_text(body, encoding="utf-8")
    return wps


def generate_all(root: Path, dry_run: bool = True) -> tuple[list[WorkPackage], list[str]]:
    """Run all seven sources; one broken artifact must not stop the others."""
    wps: list[WorkPackage] = []
    errors: list[str] = []
    seen: set[str] = set()
    for source in SOURCES:
        produced = _run_source(source, dry_run=dry_run, root=root, errors=errors)
        unique = []
        for wp in produced:
            if wp.id in seen:
                print(f"  duplicate {wp.id} skipped")
                continue
            seen.add(wp.id)
            unique.append(wp)
        wps.extend(unique)
        print(f"{source}: {len(unique)} work packages")
        for wp in unique:
            print(f"  {wp.id} [{wp.profile}] P{wp.priority} → {wp.metric}")
    return wps, errors


def main(argv: list[str] | None = None) -> int:
    import argparse
    parser = argparse.ArgumentParser(description="Generate work packages")
    parser.add_argument("source", choices=SOURCES + ("all",), nargs="?", default="all")
    parser.add_argument("--all", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--root", type=Path, default=None)
    args = parser.parse_args(argv)
    selected = "all" if args.all else args.source
    root = args.root or Path(__file__).resolve().parent.parent
    if selected == "all":
        wps, errors = generate_all(root, dry_run=args.dry_run)
        print(f"total: {len(wps)}")
        return 1 if errors else 0
    wps = generate(selected, dry_run=args.dry_run, root=root)
    print(f"{selected}: {len(wps)} work packages")
    for wp in wps:
        print(f"  {wp.id} [{wp.profile}] P{wp.priority} → {wp.metric}")
    print(f"total: {len(wps)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
