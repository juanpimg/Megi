[svg] sin soporte hsl() en paints SVG: pinta transparente

## Resumen
`parse_color` (`crates/svg/src/color.rs:68-71`) sólo ramifica `rgb`/`rgba`; la forma funcional `hsl()`/`hsla()` (CSS Color 3 §4.2.4, la más usada) cae en `_ => None` → `Paint::None` (transparente).

## Impacto
P2 — bug de corrección: SVGs válidos con `hsl()` quedan invisibles en silencio.

## Pasos para reproducir
1. `docs/audit/findings/wave2/repro-w2-assets/hsl.svg` (`hsl(0, 100%, 50%)`).
2. `cargo run -p megi-svg --example svg_to_png -- hsl.svg out.png` + conteo de píxeles (o sonda Mesa A `render`).

## Esperado vs observado
Esperado: rojo opaco `(255, 0, 0, 255)`.
Observado: `painted= 0` (transparente).

## Evidencia literal
```
hsl.svg: 20x20 painted=0 … top=[((0, 0, 0, 0), 400)]
```
Código: `color.rs:68-71` sólo ramifica `rgb|rgba` (`_ => None`).

Commit: 3c06db8

## Fix sugerido
Parser `hsl()`/`hsla()` (h en grados, s/l en %) con clamp, como el del crate css (#1130).

## Dedup
Dedup: new (relacionado #1130, otro crate — sin colisión en ledger para `svg`). Mesa A: VERIFIED. Mesa B: SOLO NUEVO, P2 mantener.
