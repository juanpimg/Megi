# Verificación Mesa B — scope F1 (tests, fuzz targets, fixtures)

Fiscal de dedup y severidad sobre `docs/audit/findings/wave2/f1-tests-fuzz-fixtures.md` (8 hallazgos `w2-t-01..08`).
No se edita código ni ledger. Fecha: 2026-11-18.

## Fuentes de dedup consultadas

- `docs/audit/FINDINGS.md` (228 findings: 118 deferred, 107 fixed, 3 unclassified). Búsqueda por `charset`, `BOM`, `FFFD`, `websocket_send`, `cascade_core`, `dialog_top_layer`, `DEBUG`, `generate_exif`, `websocket_e2e`, `form_post`, `Content-Length`: único hit relevante es #1212 (prescan panic, fixed) y #1338 (svg entities→FFFD, deferred, otro crate). Ningún solape con los 8.
- `docs/GAPS.md`: brechas de producto (WebSocket ausente #1534, XHR ausente, etc.). Ninguna fila cubre oráculos de test/fuzz ni helpers e2e; sin solape.
- `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/issues-index.json` (1000 issues; abiertos reales: #1544, #1541, #1534, #1519, #1516): ninguno es sobre estos archivos de test. #1364 (EXIF oracle, CLOSED) es antecedente citado por el propio informe, distinto del conflicto entre generadores. #1212 CLOSED, distinto.
- `gh issue list --limit 20`: sólo abiertos de producto (iframe, shadow DOM, websocket ausente, IME, a11y). Sin solape.
- `docs/audit/findings/f1/` (7 archivos): el informe ya declara dedup contra ellos; verificación por grep no muestra estos 8 targets (F1 no cubre `fuzz_targets/charset.rs`; `let _` muertos citados son de otros archivos).

## Tabla de veredictos

| id | veredicto | sev. propuesta | título concreto | accionable | spec citada | nota fiscal |
|----|-----------|----------------|-----------------|------------|-------------|-------------|
| w2-t-01 | SOLO NUEVO | P2 (mantener; P3 admisible según política de infra) | sí | sí | sí (WHATWG Encoding §13.2 + `crates/render/src/charset.rs:32-39`) | **No es falso positivo y no es defecto de producto: es defecto del oráculo fuzz.** Evidencia verificada por lectura: `fuzz/fuzz_targets/charset.rs:44-49` exige `!starts_with(U+FFFD)` tras BOM, mientras el producto pela BOM y decodifica lossy (correcto). El fix propuesto (`!starts_with(U+FEFF)`) es un one-liner accionable. Dedup limpio (#1212 es otro bug, ya fixed). P2 es el techo: rompe el contrato del harness y enmascara bugs reales (clase #1212), pero es infra-only — debe filearse como `test/fuzz`, nunca como RCE/producto. No descartar. |
| w2-t-02 | SOLO NUEVO | P3 (ok) | sí | sí | n/a (test) | `Rc sent` creado y descartado con `let _ = seen` (línea 40), provider usa `Vec::new()` propio; test `send` sin assert. Débil por diseño, fix concreto (cablear `Rc` + `assert_eq!`). Patrón `let _` similar en otros archivos pero nunca en este: no duplicado. P3 correcta (sólo cobertura). |
| w2-t-03 | SOLO NUEVO | P3 (ok) | sí | sí | n/a (test) | Título promete "records rules for each group" pero sólo aserta `stylesheet.rules.len()==1` y `let _ = &collection`. Fix concreto (asertar grupos o renombrar). F1 css-svg-html #5 es otro archivo: no duplicado. P3 correcta. |
| w2-t-04 | SOLO NUEVO | P3 (ok) | sí | sí | n/a (test) | `filter(SolidColor).count()` descartado con `let _` (líneas 116-120); orden top-layer sin assert de display list. Fix concreto (comparar índices). F1 paint-image #5 es `robustness.rs`: no duplicado. P3 correcta. |
| w2-t-05 | SOLO NUEVO | P3 (ok) | sí | sí (trivial) | n/a | `eprintln!("DEBUG status=…")` en `auth_tests.rs:69`, presente en HEAD, ruido en logs CI. Fix = borrar línea (assert de línea 70 ya cubre). Un hallazgo menor pero válido y barato. P3 correcta. |
| w2-t-06 | SOLO NUEVO | P3 (ok) | sí | sí | n/a (fixture) | Conflicto real verificado por lectura: ambos generadores escriben `exif_o{1..8}.jpg`, uno 12x8 y otro 16x16; último en ganar silencia. Ningún test fija geometría. #1364/#1555 cerrados son del oráculo, no del conflicto entre scripts: no duplicado. Fix concreto (un canónico + assert geometría). Requiere Pillow para repro, pero la colisión es visible por lectura: no necesita más evidencia. P3 correcta. |
| w2-t-07 | SOLO NUEVO | P3 (ok) | parcial (paquete de 4 subdefectos) | sí | n/a (test) | Los 4 subdefectos se verifican por lectura: `head` sin cota (vs cap 64 KiB en `auth_tests.rs:16`), `len as u8` trunca ≥126, `AtomicUsize::new(0).fetch_add` siempre 0 (nombre temporal constante), sin guardia de borrado + `join` tras assert. Todo latente (payload actual 15 B, test pasa). Recomendación al Chair: aceptar como **un** issue de endurecimiento del helper, o pedir split en 2 (cotas/framing vs higiene temporal). No es DUPLICADO; P3 correcta. |
| w2-t-08 | SOLO NUEVO | P3 (ok) | sí | sí | n/a (test; conducta prod verificada en `net/src/http.rs:1688`) | Inconsistencia real intra-archivo: `strip_prefix("Content-Length: ")` exacto (línea 34-40) vs gemelo case-insensitive (líneas 119-127). Hoy latente porque el producto emite capitalizado. Fix = copiar el parseo robusto (one-liner). Flake potencial, no fallo activo: P3 correcta, no rebajar ni escalar. |

## Concerns

1. **w2-t-01, clasificación**: es el único P2 del scope y el único que no es producto ni test unitario sino harness de fuzz. Si la política wave2 reserva P2 a impacto usuario, REBAJAR A P3 es defendible; lo mantengo en P2 porque una campaña fuzz que aborta al primer input válido es contrato roto + enmascara bugs de decodificador (precedente #1212). En todo caso, etiquetar `fuzz`, `test`, nunca `security`/producto.
2. **w2-t-07, granularidad**: cuatro defectos en un hallazgo dificultan `gh issue create` atómico. Sugerencia al Chair (no bloqueante): un issue con checklist de 4 ítems, o split cotas vs temporales.
3. **Severidades 02-08**: todas P3 y proporcionadas (debilidad de cobertura, ruido de logs, latencias sin impacto usuario). Ninguna amerita P2 ni SECURITY.
4. **Dedup residual**: `issues-index.json` contiene 1000 issues pero el ledger habla de 228 findings + oleadas F5/GAPS (#1496-#1555); la búsqueda por título/archivo no muestra colisiones, pero el Chair debería re-chequear `gh search` por `websocket_e2e` / `generate_exif` en el momento de crear issues, por si otro auditor wave2 reporta el mismo helper.
