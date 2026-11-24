# ADR-004 — Aislamiento por pestaña / sitio (F7-ISOLATION)

- **Estado**: **ACEPTADO (ADR-004, 2026-11-20); endurecimiento F0 (M0) IMPLEMENTADO y auditado**. Los procesos por sitio existen (`site.rs`, `worker/pool.rs`, `ipc/process.rs`, `process_e2e_tests.rs`) como backend **opt-in vía `--ipc-render`** con `env_clear()`, `PATH` fijado, grupo de procesos POSIX, `FileGrantTable` con contención `realpath` y tope de 64 MiB, watchdog de 5 s y política de cookies por sitio (sin `HttpOnly` ni canal de retorno al padre). **M3** cierra la frontera F1 + sandbox Linux y **M5** cierra la matriz por plataforma. Ver `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` §3 y §4.3.
- **Contexto**: `crates/browser` renderiza todas las pestañas a través de **un único worker** (`worker::spawn_worker`) que ya atrapa panics para que una página hostil no tumbe el proceso (`worker_panic_isolation_tests`). Eso es aislamiento de *fallos*, pero no de *origen*: dos pestañas de sitios distintos comparten el mismo realm JS, la misma caché y el mismo hilo de render. F7-ISOLATION exige que pestañas de sitios distintos no puedan compartir estado de render.

## Decisión (por fases)

**Fase 1 — Política de sitio y claves de proceso (HECHA).** Introducir una capa **pura y testeable** que derive, para cada URL, una `SiteKey` (origen effective: scheme + host + puerto, con reglas para `data:`, `file:`, `blob:` y orígenes opacos) y decida qué pestañas pueden compartir renderer. La política vive en `crates/browser/src/site.rs`, no depende de winit ni de IPC, y es la entrada obligatoria de cualquier planificador de procesos futuro. Todas las pestañas siguen usando hoy el worker único; la política se registra en cada pestaña y expone `can_share`/`group` para que el shell tome decisiones de agrupación.

**Fase 2 — Pool de renderers por sitio (HECHA).** Instanciar un worker por `SiteKey`, con reutilización y límite (`MAX_SITE_RENDERERS`). Cambia el enrutamiento de `RenderRequest` pero no el protocolo (sigue siendo `RenderRequest`/`RenderFrame` serializable).

Tras la Fase 2 el shell enruta cada `RenderRequest` al worker de su sitio (`crates/browser/src/worker/pool.rs`), con límite `MAX_SITE_RENDERERS` y desalojo LRU controlado; las pestañas de sitios distintos ya no comparten hilo/realm de render.

**Fase 3 — Procesos del SO (por sitio, asíncronos, opt-in vía `--ipc-render`; HECHA).** Sustituir el worker por un proceso hijo cuando `--ipc-render` está activo usando `std::process::Command`, con IPC sobre `stdin`/`stdout` y `IpcRenderResponse` serializado (sin canal de cookies hacia el padre). Sin FFI propio: `#![forbid(unsafe_code)]` se mantiene en los 17 crates del motor.

## Principios

1. La frontera de seguridad es el **sitio** (scheme/host/puerto), no la pestaña: pestañas del mismo sitio pueden compartir renderer; de sitios distintos, no.
2. `data:`, `about:blank`, `blob:` y errores se tratan como orígenes **opacos**: nunca comparten con un sitio real, pero sí entre sí cuando heredan el mismo origin (la herencia se modela explícitamente en la clave).
3. La política es **pura y determinista**: mismas URLs ⇒ misma clave, sin reloj, sin I/O, sin estado global.
4. Degradación controlada: si el número de sitios supera el límite, se reusa el renderer con la clave de menor prioridad y se reporta en el título, en lugar de fallar.

## Alternativas consideradas

- **Un proceso por pestaña**: más simple, pero rompe la agrupación por sitio (dos pestañas del mismo sitio no comparten caché/realm) y multiplica el coste de memoria. Descartado.
- **Aislamiento por hilo únicamente**: fue el estado inicial; ya atrapaba panics pero no separaba orígenes. Insuficiente para F7-ISOLATION.
- **FFI directo en crates del motor a APIs de sandbox del SO**: prohibido por `#![forbid(unsafe_code)]` en los 17 crates `megi-*`; el sandbox de M3/M5 usa crates seguros auditados (`landlock`, `rustix`) o, si hiciera falta un shim, un workspace separado con revisión dedicada.

## Consecuencias y honestidad

- F7-ISOLATION figura **`done`** en `docs/status/coverage.toml`, con la batería E2E (multi-pestaña con sitios distintos, crashes de hijos, sesiones) como evidencia. La arquitectura está en su lugar.
- `docs/ARCHITECTURE.md` añade una fila para la capa de política de sitio en `crates/browser`, sin autorizar dependencias nativas nuevas (no son necesarias en la Fase 1).
- Tests: la política se valida con URLs reales (`http(s)`, `data:`, `file:`, `about:blank`) y reglas de agrupación, sin arrancar ventanas ni procesos.

## Auditoría F0 (0 fugas T1/T2/T3/T5/T9)

Todas las mitigaciones lógicas de F0 están verificadas por pruebas negativas automatizadas en `crates/browser`:

| Amenaza | Mitigación implementada | Evidencia / test negativo |
|---|---|---|
| **T1** (fuga de cookies `HttpOnly` u otro sitio) | `seeded_cookies_for` filtra por eTLD+1 (`SiteKey`) y excluye `HttpOnly` (`crates/browser/src/ipc/cookie_policy.rs`) | `crates/browser/tests/ipc_semantic_tests.rs` |
| **T2** (inyección de cookies desde renderer) | `validate_cookie_from_renderer` rechaza dominios ajenos y flags `HttpOnly`/`Secure`; `IpcRenderResponse` no incluye `cookies` (`crates/browser/src/ipc.rs`, `cookie_policy.rs`) | `crates/browser/tests/ipc_semantic_tests.rs` |
| **T3** (escape de `file://` o symlinks) | `FileGrantTable` valida contención por `realpath`, rechaza `..`/symlinks externos y aplica tope de 64 MiB (`crates/browser/src/ipc/file_grant.rs`) | `crates/browser/tests/file_grant_tests.rs` |
| **T5** (renderer colgado sin recuperación) | Watchdog de 5 s mata el grupo de procesos y emite `IpcError::Watchdog(gen)` incrementando recuperaciones (`crates/browser/src/ipc/process_pool.rs`) | `crates/browser/tests/watchdog_tests.rs` |
| **T9** (herencia de variables de entorno) | `renderer_command` ejecuta `env_clear()`, fija `PATH=/usr/bin:/bin`, `current_dir(scratch)` y `process_group(0)` (`crates/browser/src/ipc/process.rs`) | `crates/browser/tests/spawn_hardening_tests.rs` |

Comando de verificación:

```bash
cargo test -p megi-browser --test ipc_semantic_tests --test file_grant_tests --test spawn_hardening_tests --test watchdog_tests
```
