[svg] opacity="50%" en la raíz se ignora (sólo acepta decimal)

## Resumen
La opacidad raíz (`crates/svg/src/document.rs:419-422`) usa `v.parse::<f32>()` en vez de `parse_opacity` (que sí acepta `%`); `opacity="50%"` falla el parse → `unwrap_or(1.0)`. El resto de elementos sí acepta `%` (`parse_opacity_attribute`).

## Impacto
P3 — bug de corrección: inconsistencia raíz vs resto; opacidad porcentual en raíz sin efecto.

## Pasos para reproducir
1. `docs/audit/findings/wave2/repro-w2-assets/root-opacity-pct.svg` (`opacity="50%"`, rect rojo) vs control decimal `root-opacity.svg` (`opacity="0.5"`).
2. Leer píxel centro (10,10) (sonda Mesa A `file`).

## Esperado vs observado
Esperado: centro `(255, 0, 0, 128)` (SVG 1.1 §12, `opacity` acepta number o percentage).
Observado con `"50%"`: centro `(255, 0, 0, 255)`; con `"0.5"`: `(255, 0, 0, 128)`.

## Evidencia literal
```
root-opacity-dec.svg (opacity="0.5"): px(10,10)=(255,0,0,128)
root-opacity-pct.svg (opacity="50%"): px(10,10)=(255,0,0,255)
```
Código: `document.rs:419-422` (`v.parse::<f32>()` + `unwrap_or(1.0)`).

Commit: 3c06db8

## Fix sugerido
`parse_opacity_attribute(root, "opacity").unwrap_or(1.0)`.

## Dedup
Dedup: new, distinto de #1190 (paint, Fixed) y #1268 (svg, Fixed) — aquéllos eran "opacidad raíz ignorada" (decimal tampoco funcionaba); aquí el decimal funciona y `%` no. Referenciarlos al filear para evitar cierre como duplicado. Mesa A: VERIFIED. Mesa B: SOLO NUEVO, P3 ok.
