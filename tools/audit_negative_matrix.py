#!/usr/bin/env python3
"""Generate and verify the auditable negative-test matrix (T1/T2/T3/T5/T9/F1)."""
from __future__ import annotations

import argparse
import datetime as dt
import os
import platform
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "docs/audit/negative-matrix.toml"
SUITES = ROOT / "crates/browser/tests"
SYSTEMS = ("linux", "macos", "windows")
THREATS = ("T1", "T2", "T3", "T5", "T9", "F1")
TMPDIR = "/var/tmp"
COMMAND_TEMPLATE = "cargo test -p megi-browser --test {binary} {test_name} --locked"


@dataclass(frozen=True)
class Scenario:
    id: str
    threat: str
    binary: str
    test_name: str


# Every entry is an existing negative test in crates/browser/tests; no placeholders.
SCENARIOS = (
    Scenario("NEG-T1-01", "T1", "ipc_semantic_tests", "httponly_never_seeded_to_renderer"),
    Scenario("NEG-T1-02", "T1", "ipc_semantic_tests", "cookie_from_other_site_is_rejected"),
    Scenario("NEG-T1-03", "T1", "ipc_semantic_tests", "renderer_cannot_return_cookies_over_ipc"),
    Scenario("NEG-T2-01", "T2", "ipc_semantic_tests", "renderer_forged_httponly_cookie_is_rejected"),
    Scenario("NEG-T2-02", "T2", "ipc_semantic_tests", "renderer_forged_secure_cookie_is_rejected"),
    Scenario("NEG-T3-01", "T3", "file_grant_tests", "dotdot_escape_is_rejected"),
    Scenario("NEG-T3-02", "T3", "file_grant_tests", "symlink_escape_is_rejected"),
    Scenario("NEG-T3-03", "T3", "file_grant_tests", "absolute_path_is_rejected"),
    Scenario("NEG-T5-01", "T5", "watchdog_tests", "unresponsive_renderer_times_out_below_six_seconds"),
    Scenario("NEG-T9-01", "T9", "spawn_hardening_tests", "renderer_environment_is_cleared_and_cwd_is_scratch"),
    Scenario("NEG-T9-02", "T9", "spawn_hardening_tests", "renderer_child_leads_its_own_process_group"),
    Scenario("NEG-F1-01", "F1", "ipc_validation_tests", "unknown_field_rejected"),
)

THREAT_INFO = {
    "T1": "HttpOnly/cross-site cookie leak",
    "T2": "Cookie injection from renderer",
    "T3": "File-grant escape",
    "T5": "Unresponsive renderer watchdog",
    "T9": "Renderer spawn hardening",
    "F1": "IPC validation (unknown field)",
}


def current_os() -> str:
    name = platform.system().lower()
    return {"darwin": "macos", "linux": "linux", "windows": "windows"}[name]


def verify_cmd(scenario: Scenario) -> str:
    return COMMAND_TEMPLATE.format(binary=scenario.binary, test_name=scenario.test_name)


def verify_scenarios() -> str | None:
    """Return an error string if the matrix would cite a non-existing test."""
    if len(SCENARIOS) != 12 or len({s.id for s in SCENARIOS}) != 12:
        return "scenario catalogue must contain 12 unique ids"
    if {s.threat for s in SCENARIOS} != set(THREATS):
        return "scenario catalogue must cover T1/T2/T3/T5/T9/F1"
    for scenario in SCENARIOS:
        suite = SUITES / f"{scenario.binary}.rs"
        if not suite.is_file():
            return f"missing test suite: {suite}"
        if f"fn {scenario.test_name}(" not in suite.read_text(encoding="utf-8"):
            return f"invented test: {scenario.binary}::{scenario.test_name}"
    return None


def run_scenario(scenario: Scenario) -> tuple[int, str]:
    env = dict(os.environ, TMPDIR=TMPDIR)  # CARGO_TARGET_DIR is inherited, never forced
    proc = subprocess.run(
        [
            "cargo", "test", "-p", "megi-browser", "--test", scenario.binary,
            scenario.test_name, "--locked",
        ],
        cwd=ROOT,
        env=env,
        capture_output=True,
        text=True,
    )
    lines = [line.strip() for line in (proc.stdout + proc.stderr).splitlines() if line.strip()]
    evidence = next(
        (line for line in reversed(lines) if "test result:" in line),
        lines[-1] if lines else "",
    )
    return proc.returncode, evidence


def quote(value: object) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    return '"' + str(value).replace("\\", "\\\\").replace('"', '\\"') + '"'


def render(data: dict) -> str:
    lines: list[str] = []
    for key in ("schema", "generated_by", "generated_at", "current_os", "command_template", "note"):
        lines.append(f"{key} = {quote(data[key])}")
    lines.append(
        "runner_env = { "
        f"TMPDIR = {quote(data['runner_env']['TMPDIR'])}, "
        f"CARGO_TARGET_DIR = {quote(data['runner_env']['CARGO_TARGET_DIR'])} }}"
    )
    lines.extend(("", "[threats]"))
    lines.extend(f"{key} = {quote(value)}" for key, value in data["threats"].items())
    for system in SYSTEMS:
        info = data["os"][system]
        lines.extend(("", f"[os.{system}]"))
        for key in ("status", "runner_available", "slot_count"):
            lines.append(f"{key} = {quote(info[key])}")
        for slot in info["slots"]:
            lines.extend(("", f"[[os.{system}.slots]]"))
            lines.extend(f"{key} = {quote(value)}" for key, value in slot.items())
    return "\n".join(lines) + "\n"


def build_data() -> dict:
    error = verify_scenarios()
    if error:
        raise RuntimeError(error)
    current = current_os()
    date = dt.date.today().isoformat()
    results = {scenario.id: run_scenario(scenario) for scenario in SCENARIOS}
    data = {
        "schema": 1,
        "generated_by": "tools/audit_negative_matrix.py",
        "generated_at": date,
        "current_os": current,
        "command_template": COMMAND_TEMPLATE,
        "note": "M3#5 stays pending: only the local OS has runners; other slots are unsupported.",
        "runner_env": {"TMPDIR": TMPDIR, "CARGO_TARGET_DIR": "inherited"},
        "threats": THREAT_INFO,
        "os": {},
    }
    for system in SYSTEMS:
        available = system == current
        slots = []
        for scenario in SCENARIOS:
            slot = {
                "id": scenario.id,
                "threat": scenario.threat,
                "binary": scenario.binary,
                "test_name": scenario.test_name,
                "status": "unsupported",
                "verify_cmd": verify_cmd(scenario),
            }
            if available:
                code, evidence = results[scenario.id]
                passed = code == 0 and "test result: ok" in evidence
                slot["status"] = "pass" if passed else "fail"
                slot["exit_code"] = code
                slot["evidence"] = evidence
                slot["date"] = date
            else:
                slot["reason"] = (
                    f"No local {system} runner in this environment; "
                    "M3#5 remains pending for this OS"
                )
            slots.append(slot)
        data["os"][system] = {
            "status": (
                "pass" if available and all(s["status"] == "pass" for s in slots)
                else "fail" if available else "unsupported"
            ),
            "runner_available": available,
            "slot_count": len(slots),
            "slots": slots,
        }
    return data


def cmd_write(path: Path) -> int:
    data = build_data()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(render(data), encoding="utf-8")
    for slot in data["os"][data["current_os"]]["slots"]:
        print(f"[{slot['status'].upper()}] {slot['id']} {slot['evidence']}")
    failures = [s["id"] for s in data["os"][data["current_os"]]["slots"] if s["status"] != "pass"]
    print(f"WROTE {path} date={data['generated_at']} failures={len(failures)}")
    return 1 if failures else 0


def cmd_check(path: Path) -> int:
    problems: list[str] = []
    error = verify_scenarios()
    if error:
        problems.append(error)
    if not path.is_file():
        print(f"FAIL: missing matrix: {path}")
        return 1
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    current = current_os()
    if data.get("current_os") != current:
        problems.append(f"current_os mismatch: {data.get('current_os')} != {current}")
    if not data.get("generated_at"):
        problems.append("missing generated_at")
    if data.get("command_template") != COMMAND_TEMPLATE:
        problems.append("command_template drift")
    if data.get("runner_env", {}).get("TMPDIR") != TMPDIR:
        problems.append("runner_env.TMPDIR drift")
    catalogue = {s.id: s for s in SCENARIOS}
    ids_by_os: dict[str, set[str]] = {}
    for system in SYSTEMS:
        info = data.get("os", {}).get(system, {})
        slots = info.get("slots", [])
        if len(slots) != 12:
            problems.append(f"{system}: has {len(slots)} slots; expected 12")
        ids = {slot.get("id") for slot in slots}
        if len(ids) != len(slots):
            problems.append(f"{system}: duplicate slot ids")
        ids_by_os[system] = ids
        for slot in slots:
            sid = slot.get("id", "<unknown>")
            scenario = catalogue.get(sid)
            if scenario is None:
                problems.append(f"{system}/{sid}: unknown scenario id")
                continue
            if (slot.get("binary"), slot.get("test_name")) != (scenario.binary, scenario.test_name):
                problems.append(f"{system}/{sid}: binary/test_name drift")
            if slot.get("verify_cmd") != verify_cmd(scenario):
                problems.append(f"{system}/{sid}: verify_cmd drift")
            if slot.get("status") == "pass":
                if slot.get("exit_code") != 0 or not slot.get("evidence"):
                    problems.append(f"{system}/{sid}: pass without executed-command evidence")
            elif slot.get("status") == "fail":
                problems.append(f"{system}/{sid}: recorded failing test")
            elif slot.get("status") == "unsupported":
                if system == current or not slot.get("reason"):
                    problems.append(f"{system}/{sid}: unsupported needs reason and no local runner")
            else:
                problems.append(f"{system}/{sid}: bad status {slot.get('status')!r}")
    if any(ids_by_os.get(system) != ids_by_os.get("linux") for system in SYSTEMS[1:]):
        problems.append("slot ids differ between OS sections")
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}")
        return 1
    print("PASS: negative matrix has 12/12 verified slots per OS (linux pass, macos/windows unsupported)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true", help="run tests and regenerate the matrix")
    mode.add_argument("--check", action="store_true", help="validate the published matrix")
    parser.add_argument("--path", type=Path, default=DEFAULT_OUTPUT, help="matrix path (defaults to docs/audit/negative-matrix.toml)")
    args = parser.parse_args()
    return cmd_write(args.path) if args.write else cmd_check(args.path)


if __name__ == "__main__":
    sys.exit(main())
