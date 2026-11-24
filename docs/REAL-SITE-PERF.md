# Real-site performance audit (issue #1899)

## Reproducción determinista

Snapshot de `https://www.microsoft.com/` (2026-11-24, 201 KB HTML) guardado localmente y
renderizado con `megi-render --release --width 1024 --height 768 --base https://www.microsoft.com/`.
La página produce 290 `display_items` y un PNG de 52810 bytes en todas las ejecuciones.

## Antes del arreglo

```
total 23874 ms · layout 14013 ms · scripting 3481 ms · styles_pre_script 2580 ms · fonts 3230 ms
```

Perfilado con instrumentación temporal (retirada): la construcción del árbol de layout era
1,2 ms, pero `layout_block_with_viewport` tardaba 2,7–14 s. Un único `<div>` del footer
(6898 caracteres de texto) concentraba 2,5 s.

## Causa raíz

`FontDatabase::measure_text()` y el fallback de fuentes volvían a analizar los bytes sfnt de
cada cara (`TtfFace::parse`) **por cada carácter y por cada búsqueda de glifo**. Además, los
carácteres de control (`\n`) no tienen glifo: buscar uno recorría todas las fuentes del sistema
(2–3 s por carácter singular).

## Después del arreglo

- Cada cara se analiza como máximo dos veces por medición (descubrimiento + cálculo).
- Los carácteres de control usan directamente el glifo `.notdef` de la fuente primaria;
  los espacios normales conservan su avance real.

```
total 8625 ms · layout 14.8 ms · scripting 4258 ms · styles_pre_script 1567 ms · fonts 2241 ms
```

El layout pasó de 2,7–14 s a **14,8 ms** (~100–1000×). La salida PNG es **idéntica píxel a
píxel** (0/786432 píxeles distintos) frente al render anterior. El tiempo restante depende de
red (DNS de los CDN de Microsoft falla en este entorno) y de la ejecución JS de librerías
externas, no del motor de layout.

## Tests deterministas

- `crates/fonts/src/db/shape.rs::measure_text_parses_each_face_once_not_once_per_glyph`
  — 121 análisis de fuentes → ≤ 8 (50 caracteres, 2 caras).
- `crates/fonts/src/db/shape.rs::whitespace_never_scans_the_system_fallback_fonts`
  — 41 análisis y 4,08 s → ≤ 16 y 0,04 s (RED→GREEN verificado).
