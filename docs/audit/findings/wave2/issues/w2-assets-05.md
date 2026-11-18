[svg] visibility="inherit" oculta en vez de heredar

## Resumen
`document.rs:613-616` hace `self.visibility = value == "visible"`, por lo que `inherit` (y cualquier valor no-`visible`) se convierte en oculto, aun con padre visible. `inherit` debe conservar el valor del padre clonado.

## Impacto
P3 — bug de corrección: subárboles con `visibility="inherit"` explícito desaparecen. Caso borde explícito, fix one-liner.

## Pasos para reproducir
1. `docs/audit/findings/wave2/repro-w2-assets/vis-inherit.svg` (rect rojo con `visibility="inherit"` bajo raíz visible).
2. Render con `svg_to_png` o sonda Mesa A `render`.

## Esperado vs observado
Esperado: 400 px rojos (SVG 1.1 §5.3 / CSS2 `visibility` heredable; `inherit` toma el valor del padre).
Observado: `painted= 0`.

## Evidencia literal
```
vis-inherit.svg: 20x20 painted=0 … top=[((0, 0, 0, 0), 400)]
```
Código: `document.rs:613-616` (`self.visibility = value == "visible"`).

Commit: 3c06db8

## Fix sugerido
No tocar `visibility` cuando el valor es `inherit` (conservar el del padre clonado); mantener `hidden|collapse → false`.

## Dedup
Dedup: new (sin colisión en ledger). Mesa A: VERIFIED. Mesa B: SOLO NUEVO, P3 ok.
