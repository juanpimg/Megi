# Ola 2 — cierre verificable (2026-11-18)

## Alcance y cobertura

- Ledger: `python3 tools/audit_coverage.py --report` → **1320 archivos rastreados, 818 cubiertos, 0 pendientes, 100 %** en todas las categorías auditables (`src`, `test`, `fuzz`, `fixture`, `tool`, `ci`, `manifest`, `docs`, `example`). Las categorías `binary`/`vendored`/`generated`/`exempt` están contabilizadas y exentas por construcción.
- `python3 tools/audit_coverage.py --check --strict` → `audit ledger ok: 1320 files, 818 covered, 0 pending, 254563 lines tracked, 0 file(s) to re-read, 0 unknown file(s)`.
- Base auditada: `3c06db8` en worktree limpio; cierre: commit de esta ola. El árbol principal mutó en paralelo (varias confirmaciones del usuario), por lo que la cola de relectura se recalculó al inicio y los patrones de `crates/net/src/websocket.rs` se recomprobaron tras el cambio a `wss://`.

## Método

10 auditores en scopes disjuntos leyeron línea por línea todo el código; cada hallazgo pasó por una mesa de tres subagentes: A (reproductor independiente), B (fiscal de dedup/severidad) y C (chair que decide include/comment/exclude/advisory). Las actas están en `meeting-*.md` y las verificaciones en `verify-*.md`.

## Resultados

- **73 issues públicos** creados con `gh issue create`, con evidencia literal, pasos, commit base y `Dedup:`. Mapping completo en `PUBLISHED.md`.
- **7 avisos privados** (no publicados, según `SECURITY.md`): `w2-net-001/002/003/d01`, `w2-render-01`, `w2-st-1173-file-icon`, `w2-st-1176-tmp-writers`. Borradores en `advisories/` y en las actas de `net`/`render`.
- **`comment-existing`**: 12 de `lp`, 13 de `st`, 6 de `render`, 3 de `assets`, 2 de `js`, 1 de `browser`. No se publicaron comentarios en issues cerrados para evitar ruido; los textos están listos en las actas.
- **Descartes documentados**: `w2-render-05` (convergente), `w2-render-06` (oráculo roto; el desfase título↔elemento queda como evidencia), `w2-render-11` (aterrizó en commit posterior), `w2-render-13`/`w2-core-14`/`w2-core-latent`/`F18` (needs-repro), 8 de `lp` y 8 de `st` (ya corregidos o ventana no ejecutable).

## Limitaciones honestas

1. Varios informes fueron borrados por otra sesión al liberar `/tmp`; se restauraron desde el contexto de los auditores y se verificó su existencia con `wc -l`/`ls`. Las mesas que no encontraron fuentes al primer intento lo documentaron en sus actas y se re-ejecutaron.
2. `test_harness --strict` sigue fallando con 38 diferencias documentadas de parse-error; el gate no estricto pasa. No es un hallazgo nuevo de esta ola (ámbito parser, ya conocido).
3. La mesa A de `render` encontró que el oráculo de `w2-render-06` era defectuoso (`document.title` no sincroniza `<title>`), por lo que la magnitud reportada no es fiable; el acoplamiento al wall-clock está confirmado por código.
4. `cargo deny`/`cargo audit` no están instalados localmente; el empaquetado completo de CI usa `--no-verify`. Queda como tarea de mantenimiento (`w2-ci-02` en `meeting-ci.md`).
5. `w2-st-1173`/`w2-st-1176` y los cuatro hallazgos `net` requieren decidir canal privado; no se abrió ningún issue público con detalles explotables.
