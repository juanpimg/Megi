# Diseño — Hoja de ruta de Megi hacia un navegador 100 % utilizable en producción

> **Estado**: **aprobada** (PR #2290, mergeado; es la estrategia vigente). Documento de diseño; no contiene código ni cambia dependencias.
> **Fecha**: 2026-11-24 · **Fuente**: debate de cinco mesas paralelas + builder check + revisión posterior del dueño.
> **Versión 2**: elimina la calendarización y los topes funcionales de la v1. Organiza el trabajo por **hitos de capacidad**, no por fechas, y añade un **motor de work packages** para que los agentes de IA tengan trabajo continuo. El objetivo declarado es **un navegador 100 % utilizable en producción**.

---

## 1. Intención

Ladybird entró en alpha con ~4–7 años, ~8 ingenieros full-time, 2,18 M subtests WPT, motor propio con JIT y WASM, multiproceso sandboxeado y distribución multiplataforma. Es el listón: **queremos alcanzarlo y superarlo como navegador utilizable en producción**, no como proyecto de demostración.

La diferencia de recursos es real (~51.000–90.000 horas-persona suyas vs ~170 horas de atención nuestra), pero **no es una excusa para limitar el alcance**: el trabajo lo hacen agentes de IA de forma continua, con revisión humana. Lo que la v1 llamaba "anti-objetivos" pasan a ser **carreras diferidas con condición de entrada**: se harán, pero en el momento en que el cimiento lo permita, no nunca.

Este documento decide **qué**, **en qué orden lógico** y **qué significa "terminado"** para cada hito. No fija fechas. La ambición es máxima; la honestidad sobre el estado actual es obligatoria: los badges y reportes muestran **lo conseguido**, nunca lo planeado.

### 1.1 Principio rector

> Un agente de IA nunca debe preguntarse "¿qué hago ahora?". La cola de work packages se regenera sola a partir de WPT, Test262, sitios reales, fuzz, auditorías y deuda de profundidad. El humano revisa; los agentes producen.

---

## 2. Objetivo final: "100 % utilizable en producción"

No es una frase de marketing: es una barra de aceptación medible. Se alcanza cuando **todas y cada una** de estas condiciones están verdes y se mantienen en canales públicos:

| Dimensión | Barra de producción |
|---|---|
| Conformidad | WPT: **todos los subtests adoptados pasan**, con allowlist = 0 en cada época; cobertura WPT creciente hasta el 100 % del árbol WPT. Test262 para JS con allowlist = 0 en el subconjunto declarado. |
| Sitios reales | **100/100 sitios del corpus PASS**, sin warnings degradantes, sin errores JS de consola, < 2,5 s por página. |
| Estabilidad | crash-free sessions ≥ 99,9 % con consentimiento; 0 fugas cross-site; 0 crashes en campaña de fuzz de 1 M ejecuciones por target. |
| Seguridad | 0 `unsafe` en crates `megi-*`; IPC versionado y fuzzed; sandbox **enforced** en Linux, macOS y Windows; auditoría externa publicada. |
| Interactividad | Formularios completos, SW con push/background sync, storage persistente, focus/ARIA real, descargas, impresión, portapapeles, sesiones/restauración. |
| Gráficos y media | Canvas 2D completo, WebGL 1 funcional, imágenes AVIF/JPEG XL, audio y vídeo con códecs libres, aceleración opcional. |
| Herramientas | DevTools vía protocolo Firefox + WebDriver BiDi; `about:` pages; diagnóstico de red/render. |
| Distribución | Flatpak, .deb, .rpm, AppImage, DMG notarizado, instalador Windows firmado, mobile (Android/iOS) en progreso continuo; canales stable/beta/continuous con verificación criptográfica. |
| Rendimiento | p95 de render frío ≤ 250 ms en el corpus declarado; RSS ≤ 100 MB; binario ≤ 40 MB **o** justificación publicada si una capacidad lo incrementa. |
| Mantenibilidad | todo cambio con test, gate verde, docs actualizadas, y un bus factor > 1 (múltiples maintainers formados). |
| Plataforma extendida (M10) | WebGPU/WebXR/WebRTC completos, extensiones, sync y mobile terminados |
| Integración nativa (M11) | matriz `enforced` por SO para selectores, credenciales, notificaciones y permisos |
| Media/legal (M12) | streams protegidos funcionan, **o** ADR público con el límite y alternativas |

M9 es el hito de "producción en escritorio". **M10–M12 extienden la cobertura a todo lo técnicamente posible** y cierran el único frente que depende de decisiones legales. El estado actual está **lejos** de esta barra; cada hito declara qué parte de ella cierra.

---

## 3. Estado real (verificado; punto de partida, no meta)

- 17 crates, ~244.000 líneas Rust, 3.300 tests, `#![forbid(unsafe_code)]` en 17/17.
- Gate actual: html5lib 7.050/7.050 + 3.904/3.904 + 230/230; **30** comprobaciones caseras sobre 3 archivos WPT CSS Syntax. `coverage 75/75` es autorreferencial.
- `GAPS.md` es histórico: numerosas filas `absent` ya están implementadas. La brecha es **profundidad**, no anchura.
- Scripts reales fallan (example.com, Wikipedia); formularios, event loop persistente, bidi/CJK completo, Canvas 2D útil, Shadow DOM avanzado incompletos.
- Ausencias duras: ServiceWorker completo, WebAssembly, WebGL, vídeo/audio, HTTP/3, DevTools, JIT, sandbox de SO, network process.
- Explotos lógicos: cookies globales con HttpOnly ajeno, inyección de cookies desde el renderer, child heredando entorno.
- Release: publica sin esperar CI y con tarball no reproducible.
- Documentación que miente: ADR-004 dice "por defecto" cuando es opt-in; fuzz count desactualizado.

**Toda afirmación nueva debe contrastarse con el código antes de publicarse.**

---

## 4. Arquitectura de la hoja de ruta

### 4.1 Hitos de capacidad (sin fechas)

Los hitos son **dependencias lógicas**, no calendario. Un hito se cierra cuando su barra de aceptación está verde; el siguiente puede empezar en paralelo donde no comparta archivos. Si un hito se atasca, los agentes siguen alimentándose del motor de work packages (§4.3).

| Hito | Nombre | Qué cierra | Aceptación |
|---|---|---|---|
| M0 | **Verdad y cimiento** | Métrica honesta, WPT runner, explotos lógicos, release verificable | WPT `P/T` publicado; 0 fugas T1/T2; release con verify+firma |
| M1 | **Interactividad** | Event loop persistente, corpus JS, formularios, EventSource, sesión/foco | 10 fixtures E2E de tarea de usuario; sitios del corpus sin console_errors |
| M2 | **Profundidad declarativa** | Bidi/CJK, animaciones reales, Canvas 2D, CSS/tipografía avanzada, `@supports`/pseudo-elementos | fixtures con md5 estable; solapes y warnings conocidos cerrados |
| M3 | **Frontera de seguridad** | IPC validado, sandbox Linux, network process, image process | 31+ targets fuzz; 0 fugas cross-site; 12 pruebas negativas por SO |
| M4 | **Componentes y persistencia** | Shadow DOM, custom elements, SW fase 1, WASM, storage real | 100+ subtests WPT de componentes; offline demo |
| M5 | **Plataforma nativa** | Accesibilidad completa, impresión/PDF, diálogos, descargas, portapapeles, IPC multiplataforma | auditoría a11y; sandbox enforced en 3 SO |
| M6 | **Gráficos y media** | Canvas 2D completo, WebGL 1 por software, códecs de imagen/vídeo/audio | corpus de media E2E; WebGL WPT |
| M7 | **Herramientas de desarrollador** | DevTools (protocolo Firefox), WebDriver BiDi, `about:` pages, perfilado | suites de DevTools/WebDriver pasan |
| M8 | **Motor ofensivo** | JIT de JS o migración de motor, WASM JIT, paralelismo real de estilo/layout/paint, HTTP/3 | benchmarks publicados; sin regresión de determinismo salvo declaración |
| M9 | **Producción** | 100/100 sitios, allowlists a 0, distribución multiplataforma, auditoría externa, bus factor > 1 | barra completa de §2 |
| M10 | **Plataforma extendida** | WebGPU, WebXR, WebRTC completo, extensiones, sincronización de cuenta y mobile terminado | suites WPT/WebGPU/WebXR/WebRTC pasan; extensiones y sync E2E; mobile publicado |
| M11 | **Integración nativa** | Selectores de archivo, gestores de credenciales, notificaciones y permisos por SO, deep links, impresión nativa | matriz por SO `enforced`, tests E2E nativos |
| M12 | **Cobertura media/legal** | Decisión con datos sobre DRM/EME y codecs propietarios; compatibilidad máxima alcanzable legalmente | ADR publicado; streams protegidos funcionan **o** límite documentado con alternativas |

Los hitos **no se abandonan**: los que antes eran anti-objetivos ahora están en M5–M12 con condición de entrada explícita. **M10–M12 son la extensión que cubre todo lo técnicamente posible**; lo único que puede quedar fuera es un bloqueo legal, y aun así se decide y se documenta en M12.

### 4.2 Decisiones que se mantienen (con matices)

- **Boa se conserva mientras sirva**, con dos vías-ofensivas: (a) contribuir/actualizar Boa a medida que MSRV lo permita; (b) **JIT o migración a rquickjs en M8**, decidido por datos del corpus (no por dogma).
- **WebGL entra en M6** por software, sin GPU y sin FFI en código propio; se evalúan implementaciones Rust puras y ejecución WASM de implementaciones existentes.
- **Vídeo/audio entran en M6** con códecs Rust (`symphonia`, `rav1e`/`dav1d` evaluados como procesos aislados); nunca FFmpeg enlazado en el motor.
- **Service Workers completos entran en M4–M5** (push y background sync con integración OS y permisos).
- **HTTP/3/QUIC entra en M8** con `quinn`+`h3` cuando el stack esté maduro.
- **DevTools/WebDriver entra en M7**, antes de M8: es imprescindible para profesionalizar el proyecto.
- **Mobile**: se inicia como carrera paralela después de M5, sin fecha, con Android primero (winit ya lo soporta).
- **No se rearquitectura el DAG por imitación**: se añaden crates por responsabilidad, con `check_arch.py` como guardián.
- **El determinismo deja de ser absoluto cuando una capacidad lo requiera** (JIT, GPU, hilos): en ese caso se declara un perfil `deterministic` (por defecto en CI/goldens) y un perfil `performance` (explícito), con docs y tests para ambos.
- **WebGPU y WebXR entran en M10**: WebGPU sobre `wgpu` en perfil `performance` (con backends software cuando no haya GPU) y WebXR con adaptadores por dispositivo; ambos con WPT y manejo explícito de "no soportado".
- **WebRTC completo y extensiones entran en M10**: señalización, STUN/TURN, media y API de extensiones con permisos y aislamiento; la capa de permisos de M3 se reutiliza.
- **Sincronización de cuenta entra en M10**: marcadores/historial/contraseñas con cifrado extremo a extremo y opt-in; sin servidor propio obligatorio (protocolo abierto).
- **Mobile se termina en M10**: Android e iOS dejan de ser "progreso continuo" y pasan a barra de aceptación (render, input, ciclo de vida, distribución).
- **Integración nativa entra en M11**: selectores de archivo, credenciales del SO, notificaciones, permisos, deep links e impresión nativa, con matriz por SO y degradación visible.
- **DRM/EME se decide en M12 con datos**: se evalúan licencias (Widevine/PlayReady/FairPlay), coste, requisitos legales y alternativas. Si no se obtiene licencia, se publica un ADR con el límite y las alternativas (sin prometer lo que no se puede cumplir). Es el **único** tipo de hueco que puede quedar, y queda documentado, no oculto.

### 4.3 Motor de work packages (trabajo constante para agentes)

La cola no se agota porque se alimenta de **siete fuentes automáticas**:

| Fuente | Generador | Work package resultante |
|---|---|---|
| WPT fallos | `tools/wpt_failures.py` | `WP-WPT-<sha>-<key>`: un subtest, un crate, test de regresión |
| Test262 fallos | generador análogo | `WP-JS-<key>`: semántica o binding de JS |
| Sitios reales | `real_site_check.py --live` + diff | `WP-SITE-<host>-<issue>`: degradación concreta con repro |
| Fuzz crashes | artefactos de `fuzz-nightly.yml` | `WP-FUZZ-<target>-<hash>`: minimizar + fix + regresión |
| Auditoría/ledger | `audit_coverage.py --check` | `WP-AUDIT-<path>`: re-leer y documentar |
| Deuda de profundidad | inventario de subsets con caps | `WP-DEPTH-<crate>-<feature>`: completar un subset declarado |
| Errores de usuario | plantillas `crash`/`broken_site` | `WP-USER-<n>`: reproduce y corrige |

Reglas del motor:

1. **Nunca más de 10 WPs `in_progress`**; el resto queda en cola priorizada por impacto×frecuencia.
2. Todo WP lleva: ID, fuente, archivo exacto, expected/actual, spec, criterio de aceptación y comando de verificación.
3. Un agente **prepara** el PR (TDD: test que falla → fix → test que pasa → gate); un humano revisa. Los agentes son datos no confiables: su código se trata como código de terceros, con budgets y sin red.
4. Si un hito se bloquea, el motor sigue generando WPs de las otras fuentes: **nunca hay par productive**.
5. Cada WP cerrado mueve una métrica (subtests, sitios, crashes, deuda). Si no mueve ninguna, el WP se rechaza.

### 4.4 Seguridad: catálogo de amenazas y xsema de sitio

(se mantiene lo ya decidido: T1–T10, eTLD+1 con la PSL existente, SameSite dentro del jar, `file:` por directorio canonicalizado, `data:` por documento heredero, sandbox real en Linux y macOS/Windows **hasta M5**, con reporte `unsupported` visible mientras tanto)

### 4.5 WPT: denominador creciente sin techo

(se mantiene el método: shim JS sobre Boa, pin por SHA, allowlist monotónica por época, métrica `P/T` con distribución completa; se elimina el objetivo de 25.000 como techo: **las épocas continúan hasta cubrir el árbol WPT completo**, y el denominador nunca se reduce)

---

## 5. Estructura profesional del proyecto

Para ser un navegador de producción hace falta algo más que código:

- **Gobernanza**: BDFL + maintainers por área, lazy consensus, SLA de triaje, CODEOWNERS vivos, `GOVERNANCE.md`, bus factor >1 mediante rotación de revisores.
- **Calidad**: gates de CI (fmt/clippy/tests/doc/conformance/MSRV/fuzz/arch/docs), fuzz continuo con objetivos de ejecuciones, benchmarks con método publicado, auditoría externa en M9.
- **Distribución**: stable/beta/continuous +(AppImage/.deb/.rpm/Flatpak/DMG/instalador Windows), firmas y attestations, sin auto-update hasta que haya base instalada.
- **Producto**: crash reports y sitios rotos con consentimiento, welcome page, release notes honestas, DevTools.
- **Comunidad**: plantillas, labels declarativos, good-first-issues auditados, newsletters mensuales, sponsors transparentes.
- **Docs**: cada cifra regenerable, cada capacidad con evidencia, cada decisión con ADR.

---

## 6. Riesgos (sin fecha, con condición de entrada)

| Riesgo | Mitigación |
|---|---|
| Ambición infinita ⇒ dispersión | motor de WPs con límite de 10 en curso y priorización impacto×frecuencia; hitos con barra de aceptación |
| Agentes producen código que nadie entiende | TDD obligatorio, revisión humana, WPs atómicos ≤400 líneas, `check_arch.py` |
| JIT/GPU rompen determinismo | perfiles `deterministic` y `performance` separados, declarados y testados |
| FFI en código propio | `forbid(unsafe_code)` salvo workspace separado con revisión dedicada; `cargo-geiger` público |
| Boa insuficiente | datos del corpus deciden migración única en M8 |
| Ladybird sigue avanzando | no perseguir su número: perseguir la barra de producción de §2 y publicar el método |
| Burnout del mantenedor | los agentes producen, el humano revisa; gobernanza con plusieurs maintainers |

---

## 7. Lo que queda al constructor del plan

El diseño decide hitos, barras de aceptación, motor de WPs y decisiones arquitectónicas. Los planes ejecutan: firmas, tests, comandos y descomposición en tareas. En particular:

- Los directorios WPT concretos se confirman contra el SHA importado; las épocas continúan sin techo.
- La implementación de WebGL/software, vídeo, JIT y mobile exige un **spike** previo que fije la librería y el modelo de aislamiento; el plan lo incluye como tarea 1 de cada carrera.
- La barra de §2 es el destino; los planes pueden añadir hitos intermedios, pero no rebajarla sin aprobación.
