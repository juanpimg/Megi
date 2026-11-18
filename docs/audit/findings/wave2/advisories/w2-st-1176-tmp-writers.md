# Borrador de advisory privado — w2-st-1176-tmp-writers (NO publicar como issue público)

- Nombre propuesto: `writers secundarios usan tmp predecible + truncate/symlink-follow (descargas, screenshots)`.
- Severidad propuesta: P2 (integridad, atacante local). Sin RCE remota.
- Commit afectado: `3c06db8` (`crates/shell/src/download.rs:138-161`, `crates/shell/src/headless.rs:318-348,388-409`; contraejemplo correcto `crates/shell/src/session.rs:477-504`).
- Alcance: `crates/shell` — `download::write_atomic`, `headless::save_screenshot`/`write_bytes`.

## Impacto (resumen, sin PoC público)
Los writers secundarios usan nombre fijo `.<name>.tmp` + `fs::write` (trunca y sigue symlinks). Un atacante local que plante un symlink en el nombre predecible puede redirigir bytes controlados fuera del directorio esperado. Solo `session.rs` se migró al patrón correcto (tmp único + `create_new` + guard). Dedup #1176 (la corrección solo cubrió `session.rs`; 3 call sites siguen vulnerables).

## Evidencia verificada (Mesas A/B, sin pasos explotables)
Lectura dirigida confirma el patrón vulnerable en los 3 call sites frente al patrón correcto de `session.rs:write_temporary`. La explotación (plantar symlink + provocar descarga) requiere display salvo `--save-png`, por lo que queda en `needs-repro` para el evento; el hecho estático está probado. No se publican nombres de fichero objetivo ni secuencias de plantado en este borrador.

## Fix sugerido
Reutilizar el patrón `write_temporary` único + `create_new` en los tres call sites (extraer helper común o replicar guard + `create_new`).

## Referencias de proceso
- `SECURITY.md`: canal privado según `SECURITY.md`. Nunca issue público.
- Mesa A: CONFIRMED (código) + ADVISORY + `needs-repro` (explotación). Mesa B: YA REPORTADO (#1176) + security + advisory.

## Dedup
Dedup: #1176 (evidencia nueva: la corrección solo cubrió `session.rs`). Mesa A: CONFIRMED + ADVISORY. Mesa B: YA REPORTADO, P2 mantener.
