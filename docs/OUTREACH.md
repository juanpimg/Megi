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

### Show HN / cuerpo (voz real, sin tono de marketing)

```
I've been working on this for a while and it's finally at a point where I think it's
worth showing, with the caveats up front.

Megi is a browser engine written from scratch in Rust. No Chromium/WebKit/Gecko
underneath, and the core HTML/CSS/layout/paint code doesn't use engine crates either.
It's pre-alpha and not a daily driver. What it does today: it parses HTML and builds the
DOM with full html5lib-tests conformance (7,050/7,050 tokenizer, 3,904/3,904 tree,
230/230 serializer), resolves CSS including layers, container queries, var() and calc(),
lays out tables/flexbox/grid, rasterizes to PNG, and runs JS through embedded Boa with
DOM bindings, ES modules and fetch. There's also an HTTP/2 + TLS stack and a desktop
shell with tabs.

The part I'm proudest of is probably not a feature: every crate has
#![forbid(unsafe_code)], and there's a capability manifest where each "done" item points
at a test or a conformance gate instead of a percentage I made up. Right now it's 54/70
(77%), with 2 partial. The audit ledger covers every auditable file in the repo.

What's still missing is substantial: runtime CSS animations, CJK and complex-script
shaping, an accessibility tree, and multi-process tab isolation. I'm not claiming this
competes with real browsers.

I'm mostly looking for two things: people who want to try it and tell me where it breaks,
and help with the smaller issues. There are 10 labeled "good first issue" and 5 "help
wanted", most with a repro and a local command to verify.

Repo: https://github.com/juanpimg/Megi
Docs: https://juanpimg.github.io/Megi-web/

Happy to answer questions about the architecture or why I made certain choices. I'll be
around today.
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
