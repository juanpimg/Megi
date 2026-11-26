# Paridad visual frente a otros motores

Este documento registra la comparación de Megi con **Google Chrome 157.0.8103.50**
sobre las instantáneas versionadas de sitios reales (`tests/fixtures/real-sites/`).
Es una medición de render, no una métrica de conformidad WPT.

## Metodología

1. Chrome headless, mismo viewport que Megi y sin scrollbar:
   ```bash
   google-chrome --headless=new --no-sandbox --disable-gpu --hide-scrollbars \
     --force-device-scale-factor=1 --window-size=1024,768 \
     --virtual-time-budget=3000 --screenshot=out-chrome.png \
     file:///home/jp/Megi/tests/fixtures/real-sites/example.html
   ```
2. Megi, mismo viewport:
   ```bash
   ./target/debug/render tests/fixtures/real-sites/example.html out-megi.png \
     --width 1024 --height 768        # añadir --system-fonts para el modo de paridad
   ```
3. Comparación de los dos PNG con `python3 tools/visual_parity.py a.png b.png
   --json`: un píxel cuenta como *cambiado* si `max(|ΔR|,|ΔG|,|ΔB|) > 16`;
   además se registra la diferencia media absoluta de los tres canales y el
   bounding box de las diferencias. `--side salida.png` escribe una imagen
   lado a lado. Para un directorio con varias parejas,
   `python3 tools/visual_parity_report.py DIR --json salida.json --markdown salida.md`
   genera el informe completo. Ambas herramientas solo necesitan Pillow
   (`python3-pil`), no compilan Rust; sus tests son
   `python3 -m unittest tools.test_visual_parity tools.test_visual_parity_report`.

Las cifras de la tabla anterior están versionadas como baseline en
`tests/fixtures/visual-parity/baseline.json`. Para detectar regresiones sin
compilar:
`python3 tools/check_parity_baseline.py report.json` (tolerancia por defecto:
+0,5 puntos porcentuales y +1,0 de diferencia media). La propia baseline se
valida contra el manifiesto de sitios y su esquema con
`python3 tools/check_parity_schema.py`. La tabla de este documento se comprueba
contra la baseline mediante `python3 tools/check_parity_docs.py`. El registro de
todos estos artefactos en el ledger se comprueba con
`python3 tools/check_parity_ledger.py`.

Las capturas usadas para estas mediciones están bajo `/tmp/opencode/compare-out/`
(temporales, no versionadas); la herramienta y su metodología sí están versionadas.

## Resultados (1024×768, % de píxeles cambiados / diferencia media RGB)

| Sitio | Línea base | Tras `system-ui` + pistas legacy | Con `--system-fonts` |
|---|---:|---:|---:|
| example.com | 1,04 % / 1,354 | **0,92 % / 1,246** | 0,92 % / 1,246 |
| rust-lang.org | 12,02 % / 11,802 | 12,02 % / 11,802 | **10,78 % / 10,331** |
| news.ycombinator.com | 20,61 % / 29,686 | **19,97 % / 20,909** | **17,39 % / 18,653** |
| Wikipedia | 5,31 % / 5,256 | 5,31 % / 5,256 | **4,88 % / 4,911** |

Cambios que produjeron estas mejoras (todos en `main`):

- `bc7b61d3` — `system-ui`/`ui-sans-serif` se resuelven a la fuente por defecto del
  sistema (Noto Sans en Linux), como en Chrome. `example.com` pasa de 4 líneas de
  texto a 3, igual que el motor de referencia.
- `7c2f3a05` — pistas presentacionales HTML legacy: `bgcolor`, `width` en `<table>`
  y centrado de tablas hijas de `<center>`. HN recupera la cabecera naranja y la
  tabla centrada al 85 %.
- `55afeae6` — orden de `SANS_FALLBACKS` alineado con fontconfig.
- `4244abbb` — modo opt-in `--system-fonts` de `megi-render`: usa
  `FontDatabase::new_with_bundled_and_system_fonts()` para layout y paint. El modo
  por defecto sigue siendo determinista y **ningún golden cambió** (md5 verificados
  antes y después para `basic.rgba`, `showcase/golden.png`, `canvas_full/golden.png`
  y `webgl_triangle/golden.png`).

## Live vs snapshots

La versión live de `example.com` ya no es la página de una sola frase: incluye
texto en varios idiomas (árabe, chino, ruso, francés, español), un SVG inline y
más tipografía. Comparada con Chrome da `bbox (305,151,719,768)`, 49.927 píxeles
cambiados y diferencia media 6,58. Estructuralmente coincide, pero la paridad
pixel a pixel depende de shaping bidi/CJK y del hinting.

## Diferencias restantes (por nombre)

1. **Hinting/antialiasing de glifos**: incluso forzando la misma familia, el
   snapshot de `example.com` conserva un 0,89–0,92 % de píxeles distintos.
2. **Fuentes en SVG inline**: `pipeline/svg.rs` sigue usando la DB empaquetada,
   aunque se active `--system-fonts`.
3. **Imagen rota/placeholder**: Chrome muestra icono + texto alternativo
   (`Rust Logo`); Megi pinta un bloque gris del tamaño intrínseco.
4. **Controles de formulario**: el `<select>` de `rust.html` no dibuja la flecha
   nativa de Chrome.
5. **Legacy parcial**: faltan `cellpadding`, `cellspacing`, `align`, `valign` y
   colores de `body text/link` para tablas antiguas.
6. **Shaping complejo**: bidi/CJK/árabe de la página live.

## Cómo contribuir sin compilar

- Medir con `google-chrome --headless` + `Pillow` y publicar nuevas filas en la
  tabla anterior.
- Añadir fixtures HTML pequeños que aísplen una diferencia concreta y describir
  el comportamiento esperado con la regla correspondiente (CSS Images 3 §4.2,
  CSS Backgrounds 3 §2.5, HTML presentational hints).
- Revisar el código fuente con `deslop-please --allow .deslop-please.allow` y
  reportar hallazgos, sin necesidad de `cargo build`.
- Ejecutar los gates Python puros: `tools/check_arch.py`,
  `tools/check_docs_links.py`, `tools/audit_coverage.py --check --strict`.
