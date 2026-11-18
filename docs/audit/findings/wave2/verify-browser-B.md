# Mesa B — fiscal de dedup y severidad: `f5-break-browser.md`

Fecha: 2026-11-18 · HEAD auditado por el informe: `3c06db8` · HEAD actual al fiscalizar: `e87f206`.
Fuentes: `docs/audit/FINDINGS.md` (ledger, 228), `docs/GAPS.md`, `issues-index.json` (1000),
`gh issue list` (open: #1544, #1541, #1534, #1519, #1516).

## Veredictos

| id | veredicto | severidad | título | accionable | spec | motivo |
|---|---|---|---|---|---|---|
| w2-browser-01 | YA REPORTADO #1470 (comentar con evidencia nueva) | P2 — mantener | concreto | sí (re-layout tras scripts o proveedor sucio bajo demanda; `scripts.rs:1046-1053`) | existe (CSSOM View §5) | #1470 cerrado como fixed describe el mismo síntoma (geometría congelada pre-script); la evidencia nueva muestra que persiste por otra vía (snapshot pre-script del pipeline, no `OnceLock`). No es issue nuevo: comentar/reabrir #1470. P2 proporcionada (contrato roto, geometría falsa visible). |
| w2-browser-02 | SOLO NUEVO | P2 — mantener | concreto | sí (sincronizar setter con el elemento `<title>`, crear si falta; `runtime.rs:3785-3793`, `title.rs:8-12`) | existe (HTML Standard §`document.title`) | Sin entradas en FINDINGS (grep `title` solo da `document_write` #1125), GAPS ni issues abiertos. `host.title` sin más lectores (`runtime.rs:634,3781,3791`) confirma el no-op. P2 proporcionada (contrato roto, título de ventana/pestaña nunca se actualiza). |
| w2-browser-03 | SOLO NUEVO | P3 — mantener | concreto | sí (sanear `resolved` en el error del worker loader, basename o URL original; `runtime.rs:2245`) | n/a formal (endurecimiento; expectativa razonable, no cita spec — aceptable para P3) | Sin entradas previas (worker en FINDINGS: #1221, #1138–#1140, todos distintos; GAPS sin fila). Solo afecta a `file://` (fingerprinting de layout local, sin remoto ni bypass). P3 proporcionada; etiqueta `security` menor publicable, no advisory — correcto no tratarlo como advisory. |
| w2-browser-04 | REQUIERE MÁS EVIDENCIA → **excluir** | n/a (si aterriza, P2 plausible) | concreto | sí, pero contra código que no existía en el HEAD auditado | existe (DOM Standard §`querySelectorAll`) | WIP confirmado: en `3c06db8` `grep contentDocument` vacío (el propio informe lo declara, `no aplica a HEAD`). El código (`frame_query_selector_all` vía `get_elements_by_tag_name`) ha aterrizado DESPUÉS en `e87f206` (#1544, verificado en `git show HEAD:.../runtime.rs:6610-6623`). Fuera del alcance de esta ola: excluir aquí; la Chair puede encargar verificación fresca contra el HEAD nuevo. |

## Dedup (resumen)

- 01 → #1470 existe, CLOSED/fixed, mismo síntoma y misma área. Dedup del auditor correcto, pero la consecuencia procesal es comentar en #1470, no abrir issue nuevo.
- 02, 03 → sin colisión en ledger, GAPS, índice ni `gh issue list` abierto. Nuevos.
- 04 → sin colisión posible (código inexistente en el HEAD auditado). Excluir por WIP.

## Concerns para la Chair

1. Patrón de regresión real en 01: #1470 se cerró cambiando el mecanismo (`OnceLock`) sin re-evaluar el invariante (proveedor = snapshot pre-script, `scripts.rs:1049-1051` lo admite en comentario). Revisar con la misma lente #1473/#1461/#1474/#1475, como sugiere el auditor.
2. Discrepancia de índice: la plantilla habla de "5 abiertos" pero `issues-index.json` marca 0 open y `gh issue list` devuelve 5 open (#1544, #1541, #1534, #1519, #1516) — el índice va por detrás del tracker; no afecta a estos 4 veredictos (ninguno colisiona con esos 5), pero la Chair debe usar `gh` como fuente.
3. Ningún hallazgo de esta mesa requiere aviso `security` privado (03 es publicable según el propio informe).
