# Plantilla de reunión de verificación (ola 2)

## Mesa A — Reproductor independiente

Lee el informe asignado. Para cada hallazgo ejecuta exactamente su `repro` contra `/tmp/opencode/megi-verify` (HEAD 3c06db8), con `timeout` y captura de stdout/stderr. Reconvierte el repro si no es ejecutable. Veredicto: `VERIFIED` / `PARTIAL` / `NOT VERIFIED` / `NEEDS INFO`, con comando y salida literal. Escribe `verify-<scope>-A.md`. No edites código ni ledger; no borres archivos.

## Mesa B — Fiscal de dedup y severidad

Dedup contra `docs/audit/FINDINGS.md`, `docs/GAPS.md` y el índice de issues (1000 issues). Comprueba severidad P0–P3, título concreto, cuerpo accionable y spec citada. Veredicto: `SOLO NUEVO` / `YA REPORTADO #n` / `DUPLICADO` / `REQUIERE MÁS EVIDENCIA`. Marca `security` los hallazgos explotables. Escribe `verify-<scope>-B.md`.

## Mesa C — Chair

Lee informe + mesa A + mesa B. Produce `meeting-<scope>.md` con tabla final (id, veredicto, severidad, área, etiquetas, include/exclude y motivo), cuerpo de issue en `issues/<id>.md` para cada `include`, sección `comment-existing` con texto listo, lista `advisory` (nunca issue público) y descartes con motivo. `include` exige A=VERIFIED/PARTIAL y B≠DUPLICADO. Empate o duda → exclude. Nunca publicar `needs-repro`.
