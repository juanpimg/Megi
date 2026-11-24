# Ola 2 — correcciones de seguridad mergeadas a `main`

Los 7 hallazgos tratados como aviso privado han sido arreglados con TDD (test RED visto antes del fix) y mergeados directamente a `main` el 2026-11-18.

| Advisory | Commit | Arreglo |
|---|---|---|
| `w2-net-001` cookie de PUSH antes del CORS | `b26fe91` | `store_pushed` valida CORS antes de escribir en el jar; el push rechazado no toca jar ni caché. Test: `cross_origin_push_with_set_cookie_leaves_no_cookie`. |
| `w2-net-002` status bogus en handshake | `54bf6c6` | `validate_handshake_response` parsea estrictamente el código `101` (rechaza `200 OK ( 101 ...)`). |
| `w2-net-d01` sin validación Upgrade/Connection | `54bf6c6` | Exige `Upgrade: websocket` y token `Upgrade` en `Connection` (case-insensitive, listas). |
| `w2-net-003` máscara WS fija | `4f80af4` | `client_mask()` deriva 4 bytes de `RandomState` + contador + tiempo + pid; `encode_frame` y el vector RFC 6455 intactos. |
| `w2-render-01` amplificación de iframes | `d302b2f` | `FrameBudget` global compartido por la recursión; 4×4 self-nested pasa de 341 document loads a 17 (1 + 16 frames). |
| `w2-st-1173` favicon `file:` desde http | `8f7e5d7` | `fetch(icon_url, document_url, options)`: `file:` sólo se permite si la página también es `file:`. |
| `w2-st-1176` `.tmp` predecible / symlink race | `bb08550` | Nombres `.<name>.<pid>.<nanos>.<counter>.tmp` con `OpenOptions::create_new` y reintento; usado por `write_atomic`, `save_screenshot` y `write_bytes`. |

## Verificación

En `HEAD` `3a9218f` (worktree limpio): `cargo fmt --all -- --check` → 0, `cargo clippy --workspace --all-targets -- -D warnings` → 0, `cargo check --workspace --no-default-features` → 0, `cargo test --workspace` → 0, `cargo run --release -p megi-test-harness -- --summary` → 0, `python3 tools/audit_coverage.py --check --strict` → 0. Evidencia en `/tmp/megi-pushverify.log`.

`git rev-list --left-right --count origin/main...HEAD` → `0 0` tras `git push origin main`.

Los issues públicos `#1618–#1687`, `#1689–#1691` siguen abiertos; los avisos no se publicaron como issues, pero su corrección ya está en `main`.

## Avisos privados (originales, consolidados 2026-11-24)

### w2-render-01

# Borrador de advisory privado — w2-render-01 (NO publicar como issue público)

- Nombre propuesto: `Per-level iframe cap allows render-count amplification (~0.3 KiB → hundreds of renders; W=16 projects ~70k renders, hang/OOM)`.
- Severidad propuesta: P1 (disponibilidad). Sin corrupción de memoria, sin escape de sandbox ni ejecución de código fuera del render.
- Commit afectado: `3c06db8` (`crates/render/src/frames.rs:39-48`, `lib.rs:1223`, `frames.rs:113`).
- Alcance: `crates/render` — `collect_frames` / `render_url_inner` / `MAX_FRAMES_PER_PAGE` / `MAX_FRAME_DEPTH`.

## Impacto (resumen, sin PoC público)
El cap `MAX_FRAMES_PER_PAGE` (=16) se aplica por nivel de `collect_frames` (vector `out` local por llamada), no como presupuesto global por página; la recursión vía `render_url_inner(..., depth+1)` solo topa en `MAX_FRAME_DEPTH` (=4). Una página adversa (local o remota) con iframes anidados autorreferenciales multiplica el nº de renders (1+W+W²+…): con W=4 medido 341 renders desde ~0.3 KiB (≈31.5 s en el binario debug del auditor; ×32 frente al control plano); la proyección aritmética con W=16 es 1+16+256+4096+65536 = 69905 renders (horas/OOM). El depth-cap evita la recursión infinita (termina) pero no la amplificación. Si el shell renderiza contenido no confiable, es DoS remoto no autenticado contra disponibilidad; el OOM agota memoria del proceso host.

## PoC resumido (solo para el borrador privado, no difundir)
Página mínima con un puñado de `<iframe src=self>` anidados

### w2-st-1173-file-icon

# Borrador de advisory privado — w2-st-1173-file-icon (NO publicar como issue público)

- Nombre propuesto: `favicon fetch lee ficheros locales indicados por páginas http(s) sin chequeo de esquema`.
- Severidad propuesta: P2 (confidencialidad). Sin RCE; oráculo de existencia/validez + exfiltración de píxeles 16×16 vía icono.
- Commit afectado: `3c06db8` (`crates/browser/src/favicon.rs:50-129`).
- Alcance: `crates/browser` — `icon_url` / `fetch` rama `File` / `for_page`.

## Impacto (resumen, sin PoC público)
`for_page` no filtra por esquema de la página: una página remota puede referenciar un icono `file:` y el shell lee el fichero local para pintarlo en la banda chrome (tab + address). Efecto observado: el render difiere según el fichero exista/sea imagen válida (oráculo), y los píxeles del icono se exfiltran al screenshot. Dedup #1173 (evidencia nueva en HEAD).

## Evidencia verificada (Mesas A/B, sin pasos explotables)
Lectura: `icon_url` acepta cualquier `href` resuelto; rama `File` lee disco sin mirar la página; `for_page` no filtra. Mesa A replicó el oráculo presente/ausente en headless con `--chrome` (md5 difieren). No se publican fixtures, rutas locales ni diffs de píxeles en este borrador.

## Fix sugerido
En `for_page`/`icon_url`, exigir mismo esquema (o rechazar `file:` cuando la página es `http(s)`/`data:`); resto → fallback. Test: página http con icono file nunca toca disco.

## Referencias de proceso
- `SECURITY.md`: reportar por canal privado (aviso GitHub o em

### w2-st-1176-tmp-writers

# Borrador de advisory privado — w2-st-1176-tmp-writers (NO publicar como issue público)

- Nombre propuesto: `writers secundarios usan tmp predecible + truncate/symlink-follow (descargas, screenshots)`.
- Severidad propuesta: P2 (integridad, atacante local). Sin RCE remota.
- Commit afectado: `3c06db8` (`crates/browser/src/download.rs:138-161`, `crates/browser/src/headless.rs:318-348,388-409`; contraejemplo correcto `crates/browser/src/session.rs:477-504`).
- Alcance: `crates/browser` — `download::write_atomic`, `headless::save_screenshot`/`write_bytes`.

## Impacto (resumen, sin PoC público)
Los writers secundarios usan nombre fijo `.<name>.tmp` + `fs::write` (trunca y sigue symlinks). Un atacante local que plante un symlink en el nombre predecible puede redirigir bytes controlados fuera del directorio esperado. Solo `session.rs` se migró al patrón correcto (tmp único + `create_new` + guard). Dedup #1176 (la corrección solo cubrió `session.rs`; 3 call sites siguen vulnerables).

## Evidencia verificada (Mesas A/B, sin pasos explotables)
Lectura dirigida confirma el patrón vulnerable en los 3 call sites frente al patrón correcto de `session.rs:write_temporary`. La explotación (plantar symlink + provocar descarga) requiere display salvo `--save-png`, por lo que queda en `needs-repro` para el evento; el hecho estático está probado. No se publican nombres de fichero objetivo ni secuencias de plantado en este borrador.

## Fix sugerido
Reutilizar el patrón `write_temporary` únic

