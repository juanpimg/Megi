# Megi website

Página estática y bilingüe (ES/EN) del proyecto **Megi Web Engine**.

- **URL de producción**: <https://juanpimg.github.io/Megi/> (GitHub Pages, proyecto `juanpimg/Megi`).
- **Sin build step**: HTML + CSS + JS estáticos. Se publica tal cual.
- **Sin peticiones externas**: fuentes autoalojadas, imágenes locales, iconos SVG inline. No hay CDNs ni trackers.
- **Tema claro/oscuro** y **ES/EN** con preferencias persistentes y enlaces directos (`?theme=dark&lang=en`).

> Nota de transparencia: esta web es una página normal; **no** está renderizada por Megi. Las capturas de la galería sí son renders reales del motor (goldens deterministas).

## Previsualizar en local

```bash
# desde la raíz del repo
python3 -m http.server 8137 --directory website
# abre http://127.0.0.1:8137/
```

También funciona abriendo `website/index.html` con `file://`, aunque el servidor local reproduce mejor las rutas y el manifest.

## Publicar (GitHub Pages)

El workflow [`.github/workflows/pages.yml`](../.github/workflows/pages.yml) despliega `website/` en cada push a `main` que toque la web (o manualmente con *Run workflow*).

1. En GitHub: **Settings → Pages → Build and deployment → Source: GitHub Actions**.
2. Push a `main` que incluya `website/**` (o lanza el workflow a mano).
3. La primera publicación aparecerá en <https://juanpimg.github.io/Megi/>.

Si más adelante se usa un dominio propio, actualiza `<link rel="canonical">`, las etiquetas `og:url`/`og:image` de `index.html`, `robots.txt` y `sitemap.xml`.

## Estructura

```text
website/
├── index.html                 # Landing completa (ES/EN, temas claro/oscuro)
├── site.webmanifest           # PWA/iconos
├── robots.txt · sitemap.xml · .nojekyll
├── assets/
│   ├── css/fonts.css          # @font-face autoalojado (subset latino)
│   ├── css/site.css           # Sistema de diseño y responsive
│   ├── js/site.js             # Tema, idioma, nav, reveals, contadores, lightbox, copiar
│   ├── fonts/                 # Inter, Space Grotesk, JetBrains Mono (OFL 1.1)
│   ├── img/                   # Capturas reales del motor + manifest.json
│   └── brand/                 # mark.svg, favicon, iconos, og.png
└── README.md
```

## Convenciones de edición

### Bilingüe (ES/EN)

Cada bloque de texto traducible lleva pares de spans con `lang`:

```html
<p><span lang="es">Texto en español</span><span lang="en">English text</span></p>
```

`site.js` conmuta `data-lang` en `<html>` y `site.css` oculta el idioma inactivo. Mantén **siempre los pares completos** (el QA comprueba la paridad). Para atributos (`aria-label`, `alt` de la galería) usa pares `data-aria-es`/`data-aria-en` o `data-caption-es`/`data-caption-en`, que el JS aplica al cambiar de idioma.

### Tema

Variables CSS en `:root` (claro) y `html[data-theme="dark"]` (oscuro). El bootstrap inline de `index.html` aplica el tema guardado o el del sistema antes del primer paint. No escribas colores literales en los componentes: usa las variables.

### Datos y cifras

La web documenta el estado real del motor con un tono **promocional**: explica qué es Megi, qué ya funciona y cómo sumarse; el detalle técnico vive en el README, `docs/` y la hoja de ruta. **Regla: ningún número sin fuente en el repo.** Al cerrar una ronda:

1. Actualiza el snapshot de la web (hero, estado, roadmap, FAQ, footer): tests, porcentajes de fase y rendimiento.
2. Verifica el recuento real con `cargo test --workspace` y el gate con `cargo run -p test_harness -- --summary`.
3. Revisa que no queden cifras antiguas (`grep -nE '990|8[0-9] %' website/index.html`).
4. Actualiza `lastmod` de `sitemap.xml` si cambia el contenido.

Snapshot actual: **ronda 15** (2026-11-10). La documentación de la ronda 15 cita **990 tests** y **14 módulos** (incluye `crates/svg`); Fase 5 ≈95 %, Fase 6 ≈80–85 %, Fase 7 ≈70 %. La web muestra 990/14 en el hero, los números, el roadmap y los badges. Si cambia la ronda, actualiza esas cifras en `index.html` (hero, «Los números», barras de fase, roadmap, CTA y footer) y la nota del hero.

### Capturas

Las imágenes de `assets/img/` son **renders reales del motor**, no capturas de otros navegadores ni retoques. `manifest.json` documenta origen, dimensiones, bytes y `md5` de cada una. Para regenerarlas:

```bash
cargo run -p render -- tests/fixtures/render/showcase/index.html /tmp/showcase.png --width 1024 --height 768
cargo run -p render -- tests/fixtures/render/images.html /tmp/images.png
cargo run -p render -- tests/fixtures/render/basic.html /tmp/basic.png --width 640 --height 400
cargo run --release -p paint --example html_to_png -- tests/fixtures/phase4/basic.html /tmp/phase4.png
```

Convierte a WebP sin pérdida (`magick input.png -quality 100 output.webp`) y actualiza `manifest.json` con los nuevos `md5`.

### Accesibilidad y calidad

- Un solo `<h1>`, landmarks (`header`/`nav`/`main`/`footer`), `alt` en imágenes, `aria-label` en iconos y controles, foco visible y `prefers-reduced-motion`.
- No añadas recursos externos: el sitio debe funcionar offline y sin trackers.
- Prueba en 390 px, 768 px, 1024 px y 1440 px, en claro/oscuro y ES/EN.

## Créditos

- Tipografías: **Inter** © The Inter Project Authors, **Space Grotesk** © The Space Grotesk Project Authors, **JetBrains Mono** © The JetBrains Mono Project Authors — licencia SIL OFL 1.1 (archivos `assets/fonts/OFL-*.txt`).
- Capturas: renders deterministas de Megi (`docs/images/`), validados con `tools/png_check.py`.
- Código del sitio: contribuciones bajo **MIT OR Apache-2.0**, igual que el resto del proyecto.
