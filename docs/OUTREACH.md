# Difusión y captación de contribuciones

Materiales para dar a conocer **Megi Web Engine** y atraer colaboradores. Las cifras son la
instantánea de `PROGRESS.md` (2026-11-18): 14 crates, 2.802 tests, 208k líneas Rust,
77 % de capacidades (54/70), 0 `unsafe`, gate de conformidad en PASS, pre-alpha v0.1.0.

## Principios

1. **Usar el nombre completo**, «Megi Web Engine», y siempre el enlace canónico. «Megi» sola
   colisiona con una extensión de navegador, un fork del kernel para PinePhone y otros
   proyectos. Nunca publicar sólo el nombre.
2. **Ser honesto**: es pre-alpha. Decir qué funciona y qué falta genera más confianza que
   prometer un navegador completo. Las cifras van con su fuente (`PROGRESS.md`).
3. **Aportar antes de promocionar**: en hilos ajenos, responder técnicamente primero y
   mencionar Megi sólo si encaja de verdad. Nada de copiar el mismo texto en veinte sitios.
4. **No publicar desde una cuenta nueva ni en masa**: se interpreta como spam y puede
   causar baneos. Un canal a la vez, con días de separación.
5. **No inventar métricas** y no decir «100 % compatible con la web»: el 100 % es de
   `html5lib-tests` (tokenizador/árbol/serializador), no del navegador.

## Argumentos que mejor funcionan

- Motor **from scratch**: ni Blink, ni WebKit, ni Gecko; ni siquiera crates de parsing o
  layout de terceros para el núcleo.
- **14 crates en DAG**, con `#![forbid(unsafe_code)]` en todos.
- **Verificación closed-loop**: html5lib-tests 7.050/7.050 + 3.904/3.904 + 230/230, WPT
  CSS Syntax 30/30, y un ledger de auditoría línea por línea al 100 %.
- **Cosas concretas ya hechas**: HTTP/2 + TLS, caché RFC 9111, Boa 0.22 con módulos ES y
  `fetch`, SVG propio, PNG propio, shell con pestañas y modo screenshot headless.
- **Puerta de entrada para contribuir**: 10 issues `good first issue` y 5 `help wanted`,
  más un prompt `docs/AI-QUICKSTART.md` para agentes.

## Dónde publicar (orden recomendado)

| Canal | Por qué | Regla a respetar |
|---|---|---|
| `Show HN: Megi — a browser engine written from scratch in Rust` | Audiencia técnica; precedentes recientes (Aurora, Vaev, Gosub) | Título `Show HN:`; el contenido debe ser técnico y transparente; no pedir votos |
| r/rust (`Showcase`) | Comunidad Rust; hay hilos de motores desde cero | Flair correcto; responder preguntas; evitar «mira mi repo» sin sustancia |
| Lobsters | Audiencia de sistemas y Rust; valora posts con detalle técnico | Enviar el repositorio conDescription larga; sin títulos clickbait |
| Foro de Rust (`users.rust-lang.org`) | Menos ruido, good para pedir contribuciones | Categoría Showcase/Announcements; tono colaborativo |
| Blog propio / dev.to / Hashnode | Artículo de profundidad con capturas reales | Mostrar el pipeline HTML→CSS→layout→PNG con evidencia |
| Mastodon / X / LinkedIn | Alcance corto; útil para enlazar el artículo | Hilo corto con 3–4 datos verificables, no sólo un enlace |
| Hilos existentes (Gosub, Vaev, Servo, browser engines) | Aportar contexto sin abrir otro post | Leer las reglas del subreddit; comentar primero algo útil |

## Borradores

### Show HN / título

```
Show HN: Megi – a browser engine written from scratch in Rust
```

### Show HN / cuerpo

```
Hi HN,

I've been building Megi Web Engine, a browser engine written from scratch in Rust. It is
not a Chromium/WebKit/Gecko wrapper, and the core HTML/CSS/layout/paint code does not use
third-party engine crates either. It is pre-alpha, so I want to be precise about what it
does and does not do.

What works today:
- WHATWG HTML tokenizer and tree builder, verified against html5lib-tests:
  7,050/7,050 tokenizer, 3,904/3,904 tree, 230/230 serializer.
- CSS Syntax / Selectors L4 / cascade layers / container queries / var() and calc().
- Layout (BFC/IFC, tables, flexbox, grid), a 2D rasterizer and an in-house PNG encoder.
- HTTP/1.1 and HTTP/2 with rustls TLS, HTTP cache, cookies, CORS, and WebSockets.
- JavaScript through embedded Boa 0.22, with DOM bindings, events, timers, ES modules
  and fetch. A desktop shell with tabs and a headless screenshot mode.

The workspace has 14 crates in an acyclic dependency graph, all with
#![forbid(unsafe_code)]. The test suite has 2,802 tests; `test_harness --summary` is the
conformance gate and passes. Capability coverage is a verifiable 54/70 (77%), published
as a manifest with evidence rather than an estimate.

What is missing: runtime CSS animations, CJK/complex-script shaping, an accessibility
tree, and multi-process tab isolation. It is a research/pre-alpha engine, not a daily
driver.

Repo: https://github.com/juanpimg/Megi
Docs: https://juanpimg.github.io/Megi-web/
Good first issues: https://github.com/juanpimg/Megi/labels/good%20first%20issue

I'm especially looking for help with SVG/CSS conformance edge cases, fuzz triage and
documentation. Happy to answer questions about the architecture or the audit process.
```

### r/rust (flair: Showcase)

```
Título: Megi – 14-crate browser engine in Rust, no unsafe, html5lib 100%, looking for contributors

Cuerpo: mismo contenido que Show HN, recortado a 4 párrafos, con una pregunta abierta
al final: «¿qué parte del pipeline os parece más interesante para intentar mejorar?».
Incluir los enlaces a `good first issue` y a `docs/ARCHITECTURE.md`.
```

### Lobsters

```
Título: Megi: a from-scratch browser engine in Rust with a verified capability manifest

Descripción: 2–3 párrafos técnicos. Lobsters valora el detalle: explicar la arena DOM
con NodeId de 4 bytes y generaciones, el DAG de 14 crates, y por qué el gate usa
html5lib-tests + WPT en bucle cerrado. Terminar con el enlace al repo y al informe de
cobertura. Etiquetas: rust, browsers, programming.
```

### Hilo corto (Mastodon / X)

```
1/ Estamos construyendo Megi Web Engine: un motor de navegador en Rust desde cero, sin
   Blink/WebKit/Gecko y sin unsafe. 14 crates, 2.802 tests, pre-alpha v0.1.0.

2/ Verificación: html5lib-tests 7.050/7.050 + 3.904/3.904, WPT CSS 30/30, y un ledger
   de auditoría línea por línea al 100 %. La cobertura publicada es 54/70 (77 %),
   con evidencia por capacidad.

3/ Ya funciona: HTML/CSS/layout/paint, HTTP/2 + TLS, Boa 0.22 con módulos y fetch, SVG,
   PNG propio y shell con pestañas. Falta: animaciones CSS, CJK, accesibilidad y
   aislamiento por pestaña.

4/ Buscamos contribuciones: hay 10 issues `good first issue` y 5 `help wanted`.
   https://github.com/juanpimg/Megi · docs: https://juanpimg.github.io/Megi-web/
```

### Comentario en hilo ajeno (plantilla)

```
Gracias por el análisis; muy de acuerdo con [punto concreto del hilo]. Como dato adicional,
nosotros llevamos tiempo con un motor desde cero en Rust y nos encontramos el mismo problema
de [problema técnico del que habla el hilo]. Lo resolvimos de esta forma: [detalle técnico
breve con evidencia]. Si a alguien le sirve, está en
https://github.com/juanpimg/Megi/blob/main/docs/ARCHITECTURE.md
```

> Regla: el texto entre corchetes **debe** rellenarse con contenido real del hilo. Si no se
> puede aportar nada técnico, no comentar.

## Antes de publicar

- [ ] Comprobar que las cifras siguen siendo las de `PROGRESS.md` (el repo cambia rápido).
- [ ] Usar «Megi Web Engine» y el enlace completo.
- [ ] Elegir un solo canal por día; no coincidir con otro post del proyecto.
- [ ] Tener una cuenta con historial y aceptar responder preguntas durante 24–48 h.
- [ ] En hilos ajenos, leer primero las reglas del sitio sobre autopromoción.
- [ ] No publicar vulnerabilidades: van a un aviso privado según `SECURITY.md`.
