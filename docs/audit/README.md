# Audit ledger

This directory is the durable record of the repository-wide audit. Issues alone are not
enough: this project has 1,000+ of them and they get closed in sweeps, sometimes with a
"deferred" note instead of a fix. A closed issue disappears from the backlog, so the
findings live here instead.

## Files

| File | Content |
| --- | --- |
| `coverage.tsv` | One row per tracked file. This is the proof that nothing was skipped. |
| `FINDINGS.md` | Findings grouped by disposition, with the closing comment that justifies it. |
| `findings/waveN.json` | Machine-readable findings for audit wave N (`wave1.json` = the crate audit). |

`coverage.tsv` columns: `path`, `lines`, `category`, `phase`, `auditor`, `date`, `status`,
`findings`, `notes`.

- `category` — `src`, `test`, `fuzz`, `fixture`, `tool`, `ci`, `manifest`, `docs`, `example`,
  `binary`, `vendored`, `generated` or `exempt`.
- `phase` — the audit phase that reads it (`F1`…`F4`), or `-` for categories that are never
  read line by line.
- `status` — `covered` (read, with `auditor` and `date`) or `pending` (still to read).
- `findings` — issue range produced by reading that file, e.g. `#1105-#1119`.
- Binary, vendored, generated and exempt rows carry an explicit category and a note
  explaining why they are not read, so the inventory still accounts for them.

## Phases

| Phase | Scope | Category |
| --- | --- | --- |
| F0 | Coverage harness and recovery of the existing issue backlog | — |
| F1 | Tests, fuzz targets, fixtures | `test`, `fuzz`, `fixture` |
| F2 | Source, second pass with seven risk lenses | `src` |
| F3 | Tools, CI, manifests, docs, examples | `tool`, `ci`, `manifest`, `docs`, `example` |
| F4 | Measured performance | `tool` (bench harness) |
| F5 | Feature gap matrices (GitHub issues) | — |
| F6 | Dispositions and fixes | — |
| F7 | Verifiable close: `--check` green and no `pending` rows | — |

The seven risk lenses of F2 are: arithmetic and casts, panics and indexing, limits and
denial of service, UTF-8 and text handling, swallowed errors, global state and concurrency,
and API contract versus documentation.

## Usage

```sh
python3 tools/audit_coverage.py --generate   # add new tracked files as pending
python3 tools/audit_coverage.py --mark --path crates/html/src --auditor me --date 2026-11-17 \
    --findings '#1120-#1125'                 # records a review
python3 tools/audit_coverage.py --report     # coverage summary per category
python3 tools/audit_coverage.py --check      # gate: run in CI
```

`--check` fails when a tracked file is missing from the ledger, a row points at a file that
is no longer tracked, a category or phase drifted, a `covered` row lacks `auditor`/`date`, a
`pending` row carries them, or a binary/vendored/generated/exempt row is left pending.

A file edited since it was read is reported as drift (`<path>: 637 -> 688 lines, re-read
needed`) rather than a failure: this repository moves while the audit runs, so blocking
every unrelated pull request on a line count would just get the gate disabled. `--strict`
turns drift into a failure and is what the audit uses on its own branch and at the end of
each phase; the drift list is the work queue for re-reading.

`python3 tools/audit_recover_issues.py` rebuilds `findings/wave1.json` and `FINDINGS.md` from
the issue tracker: it reads the closing comment of every issue authored by the auditor and
classifies it as `fixed`, `deferred`, `duplicate`, `not-a-bug` or `unclassified`. It needs
an authenticated `gh`, so it is a maintenance tool, not part of the gate.

## Finding schema

```json
{
  "id": "svg-003",
  "file": "crates/svg/src/raster.rs",
  "line_start": 1858,
  "line_end": 1895,
  "phase": "F2",
  "lens": "arithmetic",
  "severity": "P1",
  "category": "bug",
  "title": "miter join paints a degenerate bowtie",
  "evidence": "probe: alpha 0 at (68,22) with miter vs 255 with bevel",
  "impact": "visible corruption of every mitered stroke",
  "fix": "clamp the miter tip to the vertex for degenerate joins",
  "test": "crates/svg/tests/miter_join.rs",
  "disposition": "fixed"
}
```

## Disposition rule

A finding is fixed directly, in its own pull request with a regression test, when it is P1 or
P2, was reproduced, touches one crate, changes less than ~150 lines, does not change a public
API, and a regression test is possible. Anything else is recorded here and tracked by an
issue, so that the evidence outlives the issue.
