"""Work-package schema for the Megi agent work queue (spec §4.3)."""

from __future__ import annotations
import re
from dataclasses import dataclass

ID_PATTERN = re.compile(r"^WP-[A-Z0-9]+-[A-Za-z0-9._-]+$")
REQUIRED_FIELDS = ("id", "source", "file", "expected", "actual", "spec",
                   "acceptance", "verify_cmd", "metric", "milestone")
HEAVY_COMMAND = re.compile(r"\b(cargo|wpt|test262|fuzz)\b", re.IGNORECASE)
ALLOWED_PROFILES = {"light", "heavy"}
ALLOWED_METRICS = {
    "wpt_passed", "js262_passed", "sites_passed", "fuzz_crashes",
    "audit_pending", "depth_closed", "user_issues",
}


def classify_profile(verify_cmd: str) -> str:
    """`heavy` needs a full build or a heavy harness; `light` runs on a small disk."""
    return "heavy" if HEAVY_COMMAND.search(verify_cmd) else "light"

@dataclass(frozen=True)
class WorkPackage:
    id: str
    source: str
    file: str
    expected: str
    actual: str
    spec: str
    acceptance: str
    verify_cmd: str
    metric: str
    milestone: str
    impact: int = 1
    frequency: int = 1
    profile: str = "light"

    @property
    def priority(self) -> int:
        return self.impact * self.frequency

def validate_wp(data: dict) -> WorkPackage:
    for field in REQUIRED_FIELDS:
        if field not in data:
            raise ValueError(f"missing required field: {field}")
        if not isinstance(data[field], str) or not data[field].strip():
            raise ValueError(f"field must be a non-empty string: {field}")
    if not ID_PATTERN.match(data["id"]):
        raise ValueError("field has invalid format: id (expected WP-<SOURCE>-<key>)")
    if data["metric"] not in ALLOWED_METRICS:
        raise ValueError(f"field has unsupported value: metric (expected one of {sorted(ALLOWED_METRICS)})")
    profile = str(data.get("profile") or "").strip().lower()
    if not profile:
        profile = classify_profile(data["verify_cmd"])
    elif profile not in ALLOWED_PROFILES:
        raise ValueError(f"field has unsupported value: profile (expected one of {sorted(ALLOWED_PROFILES)})")
    return WorkPackage(
        id=data["id"], source=data["source"], file=data["file"],
        expected=data["expected"], actual=data["actual"], spec=data["spec"],
        acceptance=data["acceptance"], verify_cmd=data["verify_cmd"],
        metric=data["metric"], milestone=data["milestone"],
        impact=int(data.get("impact", 1)), frequency=int(data.get("frequency", 1)),
        profile=profile,
    )
