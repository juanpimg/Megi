# Curated Test262-style fixtures

These files are **locally authored** smoke cases written in the Test262 style
(frontmatter block plus `assert.sameValue` / `strictEqual` / `notSameValue`
calls). They are **not** verbatim files from `tc39/test262` and this directory is
**not** a vendored copy of that repository.

- **Scope:** ECMAScript language expressions/smoke subset, **NOT full Test262
  conformance**. Only semantics Boa 0.22 actually supports are adopted.
- **Pins:** `MANIFEST.json` records the exact SHA-256 and byte length of every
  case. `tools/test262_report.py` refuses to generate a report if any fixture
  differs from its pin.
- **Execution:** the offline `megi-test-harness --test262` reporter evaluates
  each case in a fresh `megi_js` realm with a minimal `assert` shim. The P/T in
  `docs/status/test262.json` is derived from those real results, never from
  source text.
- **Provenance:** Copyright (C) 2026 Megi contributors, BSD-3-Clause. No upstream
  Test262 bytes are redistributed here.
