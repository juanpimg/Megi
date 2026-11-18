[render] navegación directa a SVG usa fuentes del sistema (no determinista entre hosts)

## Resumen
La ruta de navegación directa a SVG (`crates/render/src/lib.rs:1502`, `render_svg_document`) construye la base con `FontDatabase::new_with_bundled_and_system_fonts()`, mientras que la ruta normal usa `new_with_bundled_fonts()` (`webfonts.rs:143`). El mismo SVG produce distinto PNG según las fuentes instaladas en el host.

## Impacto
P2 — contrato de determinismo roto: mismo SVG → distinto PNG (métricas de layout) según el host. Sin fuga de datos; divergencia de render entre máquinas.

## Pasos para reproducir
1. Crate auxiliar del auditor en `/home/jp/.cache/f2-repro` (sin modificar), caso `exp04`, sobre HEAD 3c06db8.
2. Comparar `FontDatabase::new_with_bundled_fonts().len()` vs `new_with_bundled_and_system_fonts().len()` y `identity_digest`.

## Esperado vs observado
Esperado: misma base bundled en ambas rutas; `equal=true`.
Observado: `bundled_faces=2` vs `with_system_faces=1106`, `bundled_digest=14069339365844523798` vs `with_system_digest=8606334732936271880`, `equal=false`.

## Evidencia literal
```
EXP04 bundled_faces=2 with_system_faces=1106 bundled_digest=14069339365844523798 with_system_digest=8606334732936271880 equal=false
```
Código: `lib.rs:1488` (ruta SVG directa) vs `webfonts.rs:143`.

Commit: 3c06db8

## Workaround
Ninguno vía CLI; normalizar fuentes del host para comparar goldens.

## Dedup
Dedup: new (fonts en ledger #1228–#1239, todas distintas; abiertas sin tema fuentes). Mesa A: VERIFIED. Mesa B: SOLO NUEVO, P2 mantener. Fix sugerido: usar la base bundled también en `render_svg_document`.
