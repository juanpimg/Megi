# Mesa C — reunión de verificación, scope browser (`w2-browser-*`)

Fecha: 2026-11-18 · Chair: Mesa C.
HEAD auditado por el informe: `3c06db8` · HEAD actual: `e87f206`.

## Fuentes

- Plantilla: `.superpowers/sdd/2026-11-18-auditoria-linea-por-linea-megi/verifier-template.md`.
- Mesa B: `docs/audit/findings/wave2/verify-browser-B.md` (veredictos y dedup citados abajo).
- Mesa A / informe original: `docs/audit/findings/wave2/verify-browser-A.md` y `docs/audit/findings/wave2/f5-break-browser.md` **no localizados** — búsqueda en todo el árbol (`w2-browser` solo aparece en `verify-browser-B.md`). Decisión: se acepta el `A=VERIFIED` del brief del chair para 02/03, reforzado con verificación propia a nivel de código sobre `3c06db8` (citas literales en cada cuerpo). 01 se trata como regresión ya reportada (comentario, no issue). 04 se excluye por WIP declarado en el propio informe.
- No se editó código ni ledger. Ningún hallazgo requiere aviso `security` privado (03 es publicable según Mesa B).

## Tabla final

| id | veredicto consensuado | sev | área | etiquetas | include/exclude | motivo |
|---|---|---|---|---|---|---|
| w2-browser-01 | YA REPORTADO #1470 (regresión, evidencia nueva) | P2 | render | bug, P2, render | exclude (comment-existing) | #1470 CLOSED/fixed describe el mismo síntoma; la evidencia nueva muestra otra vía (snapshot pre-script del pipeline, no `OnceLock`). Comentar/reabrir #1470, no abrir issue. |
| w2-browser-02 | SOLO NUEVO · A=VERIFIED (brief; mecanismo confirmado en código `3c06db8`) | P2 | render | bug, P2, render | **include** | Setter `document.title` no-op (solo `host.title`, sin lectores); título de ventana sale del `<title>` del DOM. Sin colisiones. |
| w2-browser-03 | SOLO NUEVO · A=VERIFIED (brief; mecanismo confirmado en código `3c06db8`) | P3 | render | P3, render, security (menor, publicable) | **include** | Errores del worker loader con ruta `file://` absoluta hasta el script. Solo local, sin remoto. Sin colisiones. |
| w2-browser-04 | REQUIERE MÁS EVIDENCIA | n/a (P2 plausible si aterriza) | — | — | exclude (WIP) | Código inexistente en `3c06db8` (`grep contentDocument` vacío, el propio informe lo declara); aterrizó después en `e87f206` (#1544). Fuera del alcance de esta ola; encargar verificación fresca contra el HEAD nuevo. |

## `comment-existing` — w2-browser-01 → #1470 (NO abrir issue nuevo)

Texto listo para `gh issue comment 1470 --body ...` (o reapertura):

```text
Reapertura/regresión parcial con evidencia nueva (ola 2, HEAD `3c06db8`).

El fix del `OnceLock` eliminó el congelado de una sola medida, pero el
proveedor que lo reemplaza sigue siendo un snapshot pre-script del pipeline:
`crates/render/src/scripts.rs:1039-1047` lo admite en el propio comentario
("the provider itself is the pre-script snapshot in the pipeline").
`getBoundingClientRect()`/`offset*`/`client*` tras mutación por script siguen
devolviendo geometría pre-script (nodos creados por script típicamente 0×0).

Vía anterior (cerrada): `OnceLock` en el setter del layout provider.
Vía actual: proveedor = layout de `doc_snapshot` + `pre_script_styles`
calculado antes de ejecutar scripts.

Propuesta: re-layout tras scripts o proveedor sucio bajo demanda
(re-evaluar contra el documento vivo, invalidado por mutación), como ya
sugiere el informe de ola 2. Dejo a criterio del mantenedor reabrir este
issue o continuar aquí en vez de abrir uno nuevo.

Dedup: w2-browser-01, ola 2, commit `3c06db8`. Sin issue nuevo asociado.
```

## Includes — cuerpos listos para `gh issue create --body-file`

- w2-browser-02 (P2): `docs/audit/findings/wave2/issues/w2-browser-02.md`
  - Título: `[render] document.title setter is a no-op: window/tab title never updates`
  - Comando: `gh issue create --title '[render] document.title setter is a no-op: window/tab title never updates' --label bug,P2,render --body-file docs/audit/findings/wave2/issues/w2-browser-02.md`
- w2-browser-03 (P3): `docs/audit/findings/wave2/issues/w2-browser-03.md`
  - Título: `[render] worker loader errors leak absolute local paths to page scripts`
  - Comando: `gh issue create --title '[render] worker loader errors leak absolute local paths to page scripts' --label P3,render,security --body-file docs/audit/findings/wave2/issues/w2-browser-03.md`

## Security (aviso privado)

Ninguno. w2-browser-03 lleva etiqueta `security` menor publicable (fingerprinting local `file://`, sin remoto ni bypass); no es advisory.

## Descartes (con motivo)

- w2-browser-01 → `comment-existing` en #1470 (duplicado con evidencia nueva de regresión; abrir issue duplicaría).
- w2-browser-04 → `exclude`, WIP fuera del alcance de la ola (`3c06db8` no contiene el código; verificar de nuevo contra `e87f206`+ antes de publicar).

## Concern trasladado de Mesa B (no bloquea)

Discrepancia de índice: la plantilla habla de "5 abiertos" pero `issues-index.json` marca 0 open mientras `gh issue list` devuelve 5 (#1544, #1541, #1534, #1519, #1516). El índice va por detrás del tracker; usar `gh` como fuente. No afecta a estos 4 veredictos.
