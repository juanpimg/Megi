#!/usr/bin/env python3
"""Repro for w2-ci-01: blank pages never fail real_site_check.failure_reasons.

Expected (per docstring + #1485): a blank page is a hard failure -> 1 reason.
Actual: [] because failure_reasons compares against "PAGINA EN BLANK"
instead of classify()'s "PAGINA EN BLANCO".
"""

import sys

sys.path.insert(0, "tools")

from real_site_check import classify, failure_reasons  # noqa: E402

sites = [{"id": "x", "name": "X"}]
stats = {"display_items": 0}
label = classify(stats)
results = [{"name": "X", "result": label, "display_items": 0, "error": ""}]
reasons = failure_reasons(sites, results)
print(f"classify blank -> {label!r}")
print(f"failure_reasons -> {reasons}")
sys.exit(0 if reasons else 1)
