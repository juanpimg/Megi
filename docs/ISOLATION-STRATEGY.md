# ADR-004 — Aislamiento por pestaña / sitio (F7-ISOLATION)

- **Estado**: **ACEPTADO (ADR-004, 2026-11-20)** — decisión de arquitectura para la Fase 7.
- **Contexto**: `crates/browser` renderiza todas las pestañas a través de **un único worker** (`worker::spawn_worker`) que ya atrapa panics para que una página hostil no tumbe el proceso (`worker_panic_isolation_tests`). Eso es aislamiento de *fallos*, pero no de *origen*: dos pestañas de sitios distintos comparten el mismo realm JS, la misma caché y el mismo hilo de render. F7-ISOLATION exige que pestañas de sitios distintos no puedan compartir estado de render.

## Decisión (por fases)

**Fase 1 — Política de sitio y claves de proceso (esta ADR).** Introducir una capa **pura y testeable** que derive, para cada URL, una `SiteKey` (origen effective: scheme + host + puerto, con reglas para `data:`, `file:`, `blob:` y orígenes opacos) y decida qué pestañas pueden compartir renderer. La política vive en `crates/browser/src/site.rs`, no depende de winit ni de IPC, y es la entrada obligatoria de cualquier planificador de procesos futuro. Todas las pestañas siguen usando hoy el worker único; la política se registra en cada pestaña y expone `can_share`/`group` para que el shell tome decisiones de agrupación.

**Fase 2 — Pool de renderers por sitio (pendiente).** Instanciar un worker por `SiteKey`, con reutilización y límite (`MAX_SITE_RENDERERS`). Cambia el enrutamiento de `RenderRequest` pero no el protocolo (sigue siendo `RenderRequest`/`RenderFrame` serializable).

**Fase 3 — Procesos del SO (pendiente).** Sustituir cada worker por un proceso hijo usando `std::process::Command`, con IPC sobre `stdin`/`stdout` y `RenderFrame` serializado. Sin FFI: `#![forbid(unsafe_code)]` se mantiene. Requiere ADR complementaria para el formato de IPC, manejo de crashes y permisos.

## Principios

1. La frontera de seguridad es el **sitio** (scheme/host/puerto), no la pestaña: pestañas del mismo sitio pueden compartir renderer; de sitios distintos, no.
2. `data:`, `about:blank`, `blob:` y errores se tratan como orígenes **opacos**: nunca comparten con un sitio real, pero sí entre sí cuando heredan el mismo origin (la herencia se modela explícitamente en la clave).
3. La política es **pura y determinista**: mismas URLs ⇒ misma clave, sin reloj, sin I/O, sin estado global.
4. Degradación controlada: si el número de sitios supera el límite, se reusa el renderer con la clave de menor prioridad y se reporta en el título, en lugar de fallar.

## Alternativas consideradas

- **Un proceso por pestaña**: más simple, pero rompe la agrupación por sitio (dos pestañas del mismo sitio no comparten caché/realm) y multiplica el coste de memoria. Descartado.
- **Aislamiento por hilo únicamente**: es el estado actual; ya atrapa panics pero no separa orígenes. Insuficiente para F7-ISOLATION.
- **FFI a APIs de sandbox del SO**: incompatible con `#![forbid(unsafe_code)]`. Descartado.

## Consecuencias y honestidad

- Tras la Fase 1, F7-ISOLATION seguirá marcada **`partial`**: existe la política y las claves, pero **no hay aún procesos ni renderers separados por sitio**. No se marcará `done` hasta la Fase 3 con evidencia verificable.
- `docs/ARCHITECTURE.md` añade una fila para la capa de política de sitio en `crates/browser`, sin autorizar dependencias nativas nuevas (no son necesarias en la Fase 1).
- Tests: la política se valida con URLs reales (`http(s)`, `data:`, `file:`, `about:blank`) y reglas de agrupación, sin arrancar ventanas ni procesos.
