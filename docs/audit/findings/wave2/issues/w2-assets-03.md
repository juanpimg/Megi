[svg] fill con 132 de los 148 colores CSS con nombre queda sin paint (invisible)

## Resumen
`named_color` (`crates/svg/src/color.rs:138-163`) sólo reconoce 16 colores básicos + `orange` + `transparent`. Cualquier otro nombre válido (`rebeccapurple`, `coral`, `darkblue`, `lightgray`, … — 132 de 148) cae en `_ => None`, y `parse_color` → `None` → `Paint::None` (transparente).

## Impacto
P2 — bug de corrección: SVGs válidos con colores comunes quedan invisibles en silencio. Fallo silencioso a transparente.

## Pasos para reproducir
1. `docs/audit/findings/wave2/repro-w2-assets/namedcolor.svg` (`fill="rebeccapurple"`).
2. `cargo run -p svg --example svg_to_png -- namedcolor.svg out.png` + conteo de píxeles (o sonda Mesa A `render` vía `svg::render_svg`).

## Esperado vs observado
Esperado: rectángulo opaco `(102, 51, 153)` (CSS Color 3 / SVG 1.1 §4.2; `rebeccapurple` es CSS Color 4).
Observado: `painted= 0` (todo transparente).

## Evidencia literal
```
namedcolor.svg: 20x20 painted=0 … top=[((0, 0, 0, 0), 400)]
```
Informe F2: `painted= 0` (esperado `(102, 51, 153)` opaco). Código: `color.rs:138-163` (`named_color`, sólo 16 básicos + `orange` + `transparent`).

Commit: 3c06db8

## Fix sugerido
Tabla completa de 148 nombres (reutilizar la del crate css o una tabla compartida).

## Dedup
Dedup: new (relacionado con #1129 pero de otro crate — #1129 fixed es del crate `css`; en `svg` sigue vivo). Mesa A: VERIFIED. Mesa B: SOLO NUEVO, P2 mantener.
