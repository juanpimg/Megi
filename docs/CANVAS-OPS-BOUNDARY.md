# ADR-006 — Límite de interfaz `CanvasOps` (capacidades de Canvas 2D)

**Estado:** aceptado · **Fecha:** 2026-11-25 · **Alcance:** `crates/js` (sin cambios en el DAG)

## Contexto

`deslop-please` detectó un único hallazgo nuevo sobre la deuda congelada en
`.deslop-please.allow`: el SCC gigante de `megi-js` pasó de 385 a 408 miembros.
La causa no fue `events`, sino el WP que añadió
`runtime::gap_apis::canvas_paths` (23 funciones nuevas) después de la línea base
de la auditoría: `canvas.rs` (dentro del SCC) llama a esas funciones y cada una
de ellas volvía a borrowar `host::state::Host` mediante `with_context_host` o
`this_node`. Eso cerraba el ciclo

```
canvas::canvas_get_context -> canvas_paths::register_* -> canvas_paths::* ->
    globals::install::with_context_host -> Host -> ... -> canvas::canvas_get_context
```

`canvas_support::canvas_context_node` también se movió de módulo y aparecía como
miembro nuevo. El plan anterior (commit `48358513`) ya había documentado que
"detaching them needs a host-state architecture change".

## Decisión

Se introduce una capa inferior, `crate::canvas::ops`, que guarda en el
`HostDefined` de Boa un conjunto **copiable y sin lifetimes** de punteros a
función (`CanvasOps`): resolución de handles, dimensiones del elemento,
creación de superficie, `handle_for`, mutación/lectura de superficie y snapshot.

- Los bindings Canvas 2D (`canvas.rs`, `canvas_paths.rs` y `canvas_support.rs`)
  **sólo** conocen `CanvasOps`; ya no nombran `Host`, `host_cell`,
  `with_context_host` ni `this_node`.
- `install_globals` instala las implementaciones once, con closures que no
  capturan nada y que borrowan `Host` dentro de cada llamada. `Host` sigue siendo
  el único dueño del estado (AGENTS.md §2, excepción `ContextHost`).
- `CanvasOps` es `Copy`, así que un binding copia las capacidades antes de
  mutar el `Context`: no hay `Rc`/`RefCell` nuevos ni estado TLS.
- El límite es *estrecho por diseño*: seis operaciones, sin exponer el tipo
  `Host` ni el runtime a la capa de bindings.

## Consecuencias para el DAG y la auditoría

- **DAG de crates: sin cambios.** Todo ocurre dentro de `megi-js`; no hay
  edges nuevos entre crates y `tools/check_arch.py` no requiere actualización.
- El SCC pasa de 408 a **368 miembros**, todos incluidos en la entrada ya
  permitida de `.deslop-please.allow`. El allowlist **no se ha tocado**: una
  corrección desde los principios (módulo profundo + interfaz estrecha), no una
  nueva excepción.
- No se han añadido `unsafe`, dependencias ni lifetimes propios; el fichero
  nuevo tiene 200 líneas (< 400).

## Alternativas descartadas

- **Mover `Host::frames`/`workers` a otro slot:** rompe otro borde del mismo
  SCC, pero son 59 call-sites y no elimina el borde `canvas -> Host`. Cambia la
  propiedad del estado sin arreglar el límite de los bindings.
- **Trait `EventContext` implementado por `Host` para todos los eventos:** son
  57 call-sites y deja el ciclo `host -> runtime -> events`; mucho mayor y no
  addressa la causa observada.
- **Añadir una entrada al allowlist:** explícitamente prohibido por AGENTS.md §8.
