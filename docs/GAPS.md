# Matriz de gaps histórica

> **DOCUMENTO HISTÓRICO — REDUCIDO A PUNTERO (2026-11-24).**
> Este archivo se conserva solo para no romper enlaces antiguos. No es fuente de estado ni de estrategia.

El contenido íntegro puede recuperarse del historial de Git (`git log -- docs/GAPS.md`). Para el estado vigente y la estrategia actual, consulta `docs/status/coverage.html` y el código.

## Known fixture exclusions (#2435)

The five vendored html5lib encoding fixtures are **not executed** by any harness:

- `tests/html5lib-tests/encoding/tests1.dat`
- `tests/html5lib-tests/encoding/tests2.dat`
- `tests/html5lib-tests/encoding/test-yahoo-jp.dat`
- `tests/html5lib-tests/encoding/scripted/tests1.dat`
- `tests/html5lib-tests/encoding/chardet/test_big5.txt`

`crates/test_harness/src/lib.rs` only registers the tokenizer, tree-construction,
tree-construction-json and serializer harnesses. There is no encoding harness, so
these files are recorded as `exempt / vendored` in `docs/audit/coverage.tsv` and
are explicitly **excluded, not passing**. They are not part of the conformance
denominator. Charset prescan coverage is tracked as follow-up issue #2378
(`content="text/html;charset=…"` without a space); when that issue is implemented,
these fixtures should be adopted with an encoding harness and re-recorded in the
coverage ledger.
