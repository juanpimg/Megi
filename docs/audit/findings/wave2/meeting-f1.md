# Mesa C — reunión de verificación (scope F1, `w2-t-*`)

- Fecha: 2026-11-18. Chair: Mesa C.
- Commit auditado: `3c06db8` (árbol limpio `/tmp/opencode/megi-verify`; presencia re-comprobada por Mesa A en HEAD principal `2eaab73`: los 8 persisten, único desplazamiento: oráculo de `charset.rs` ahora en línea 47, contenido idéntico).
- Fuentes: informe `docs/audit/findings/wave2/f1-tests-fuzz-fixtures.md` (8 hallazgos `w2-t-01..08`), Mesa A `verify-f1-A.md` (8/8 VERIFIED), Mesa B `verify-f1-B.md` (8/8 SOLO NUEVO). (Nota: `verifier-template.md` (Mesa C) no existe en el árbol; formato reconstruido de `meeting-net.md` / `meeting-js.md`.)
- Regla include: A=VERIFIED y B=SOLO NUEVO → los 8 se incluyen. No se editó código ni ledger. Ningún `security`/producto: todo el scope es `test`/`fuzz` (w2-t-01 expresamente `fuzz`/`test`, nunca `security`).

## Tabla final

| id | A | B | sev | área | etiquetas | include/exclude | motivo / cuerpo |
|---|---|---|---|---|---|---|---|
| w2-t-01 | VERIFIED | SOLO NUEVO | P2 (techo; P3 admisible) | fuzz target charset | fuzz, test | include | Oráculo "BOM ⇒ sin U+FFFD" falso positivo; aborta la campaña con entradas válidas (lossy correcto). Cuerpo en `issues/w2-t-01.md`. |
| w2-t-02 | VERIFIED | SOLO NUEVO | P3 | js websocket_api_tests | test | include | `Rc` de enviados descartado con `let _ = seen`; `send` sin assert. Cuerpo en `issues/w2-t-02.md`. |
| w2-t-03 | VERIFIED | SOLO NUEVO | P3 | css cascade_core | test | include | Test "records rules for each group" no toca `collection` (`let _`). Cuerpo en `issues/w2-t-03.md`. |
| w2-t-04 | VERIFIED | SOLO NUEVO | P3 | paint dialog_top_layer | test | include | `filter(SolidColor).count()` descartado con `let _`; sin assert de orden en display list. Cuerpo en `issues/w2-t-04.md`. |
| w2-t-05 | VERIFIED | SOLO NUEVO | P3 | net auth_tests | test | include | `eprintln!("DEBUG …")` commiteado (línea 69). Cuerpo en `issues/w2-t-05.md`. |
| w2-t-06 | VERIFIED | SOLO NUEVO | P3 | image EXIF fixtures | test, fixture | include | Dos generadores escriben `exif_o{1..8}.jpg` con distinta geometría (12x8 vs 16x16). Cuerpo en `issues/w2-t-06.md`. |
| w2-t-07 | VERIFIED | SOLO NUEVO | P3 | render websocket_e2e helper | test | include | Paquete de 4 subdefectos latentes (head sin cota, `len as u8`, temporal constante, sin guardia). Un issue con checklist. Cuerpo en `issues/w2-t-07.md`. |
| w2-t-08 | VERIFIED | SOLO NUEVO | P3 | render form_post_tests | test | include | `Content-Length` case sensible vs gemelo case-insensitive (latente). Cuerpo en `issues/w2-t-08.md`. |

Resultado: 8 include (cuerpos en `docs/audit/findings/wave2/issues/w2-t-0{1..8}.md` con primera línea `[fuzz|test] …`, evidencia literal, `Commit: 3c06db8` y `Dedup: new`), 0 excludes, 0 descartes DUPLICADO, 0 security/advisory.

## Notas del Chair
- w2-t-01: mantener P2 como techo (contrato del harness roto + enmascara bugs clase #1212); si la política wave2 reserva P2 a impacto usuario, rebajar a P3 es defendible. En todo caso filear como `test`/`fuzz`, nunca RCE/producto (recomendación Mesa B).
- w2-t-07: aceptado como un issue de endurecimiento del helper con checklist de 4 ítems (recomendación Mesa B); split cotas/framing vs higiene temporal queda como opción del mantenedor.
- Dedup residual (Mesa B §4): en el momento de `gh issue create`, re-chequear `gh search` por `websocket_e2e` / `generate_exif` por si otro auditor wave2 reporta el mismo helper.
