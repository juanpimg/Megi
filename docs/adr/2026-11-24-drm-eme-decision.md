# ADR — DRM/EME: no soportar DRM; límite documentado con alternativas

- **Estado**: ACEPTADO — decisión derivada de la spec v2 aprobada en **PR #2290** (`docs/superpowers/specs/2026-11-24-megi-ambition-design.md:102`). No se registra aprobación individual adicional del dueño: la rama (c) ya está autorizada por la spec vigente.
- **Fecha**: 2026-11-24 (plan), registrado 2026-11-25.
- **Hito**: M12 — Cobertura media/legal.
- **Referencias**: spec v2 §4.2 (`docs/superpowers/specs/2026-11-24-megi-ambition-design.md:102`); master plan (`docs/superpowers/plans/2026-11-24-megi-ambition-master-plan.md:101`); plan de medios (`docs/superpowers/plans/2026-11-24-media-licensing-plan.md:7`, `:35-42`); `docs/AGENT-INDEX.md:29`.

## Contexto

La barra de M12 es **«DRM/EME decidido con ADR público: funciona, o límite documentado con alternativas»** (master plan, línea 101) — una disyunción explícita, no una obligación de integrar DRM. La spec v2 autoriza la rama (c): si no se obtiene licencia, se publica un ADR con el límite y las alternativas, «sin prometer lo que no se puede cumplir»; es el único hueco que puede quedar, y queda documentado, no oculto (spec, línea 102).

Estado real del código (verificado en este árbol, sin ejecutar cargo):

- No hay ningún CDM licenciado ni credenciales de producción en el repositorio; no hay ningún stream protegido funcionando.
- No hay implementación EME: `grep -E '\bEME\b|MediaKeys|requestMediaKeySystemAccess|MediaKeySession|encrypted-media' crates/**/*.rs` → 0 coincidencias. No existe crate `media` (los 18 crates son `bench, browser, core, css, display, fonts, html, image, js, layout, net, paint, raster, render, sandbox, svg, test_harness, webview`).
- `docs/status/media-matrix.toml` y `tools/media_check.py` (Task 1 del plan) no existen todavía; por tanto hoy no hay matriz de streams medida.

## Datos públicos y verificables de las tres licencias

### Widevine (Google)

- Requiere acuerdo de licencia: «A license agreement is required for the use of Widevine products or services. Widevine does not assess any fee for use of its products and services.» El producto adopta EME (W3C) y Common Encryption (ISO/IEC 23001-7 y 23001-9). El cliente se integra de forma nativa en la plataforma del dispositivo; el *license request* debe pasar por un proxy HTTPS operado por el socio, nunca directamente al servicio de licencias de Widevine. Fuente: <https://developers.google.com/widevine/drm/overview> (consultada 2026-11-25; página con fecha de actualización 2024-10-09 UTC).
- Dos vías de emisión de licencias: **(1) Cloud License Service**, accesible globalmente, sin cargos de acceso ni por transacción, con credenciales por organización para licenciatarios y entornos de prueba y producción; **(2) License Server SDK**, para auto-hospedar el servicio de licencias. Referencias de integración abiertas: Shaka Packager, Shaka Player y ExoPlayer. Misma fuente.
- Acceso al repositorio de socios: cuenta Google con correo corporativo individual válido (no se aceptan gmail/outlook/yahoo personales); «Partner access to the repository is contingent upon signing of the Widevine Master License Agreement»; cada repositorio se evalúa y aprueba por separado (código de cliente/dispositivo frente a SDK de servidor); para una integración **Level 1** se necesita la librería OEMCrypto apropiada del fabricante del chip. Fuente: <https://developers.google.com/widevine/access> (consultada 2026-11-25; fecha de actualización 2025-09-29 UTC).
- Tipos/capacidades declaradas en la configuración EME: key system `com.widevine.alpha`; cadenas de robustez `SW_SECURE_CRYPTO`, `SW_SECURE_DECODE`, `HW_SECURE_CRYPTO`, `HW_SECURE_DECODE`, `HW_SECURE_ALL` (constantes de Chromium documentadas por Shaka). EME exige URL segura (https o localhost). Fuente: <https://shaka-project.github.io/shaka-player/docs/api/tutorial-drm-config.html> (consultada 2026-11-25).

### PlayReady (Microsoft)

- Licencia obligatoria: «If you are an original equipment manufacturer (OEM), a PlayReady Client developer, or a PlayReady service provider, you must first obtain a PlayReady license from Microsoft before you can begin developing your product». Hay licencias separadas por tipo de producto (distribución en dispositivo, aplicación software descargable, aplicaciones de servidor/despliegue de servicio); aportan derechos de IP, protección de IP e indemnización. Un proveedor de contenido que usa clientes/servidores PlayReady de terceros **no** necesita licencia de Microsoft sólo para cifrar con una cabecera PlayReady. Fuente: <https://learn.microsoft.com/en-us/playready/overview/license-to-use-playready> (consultada 2026-11-25; fecha de actualización 2025-06-13).
- Tres niveles de seguridad: **SL150** (desarrollo/prueba, no apto para contenido comercial, secretos sin proteger), **SL2000** (dispositivos/apps reforzados que consumen contenido comercial; protección software u hardware; habitualmente llamado Software-DRM) y **SL3000** (sólo dispositivos, obligatoriamente mediante medios hardware con Trusted Execution Environment (TEE) del procesador, PlayReady 3.0+, máxima calidad comercial; Hardware-DRM). Cada licencia lleva `MinimumSecurityLevel` 150/2000/3000; el certificado del cliente se incrusta en fabricación; hay reglas de cumplimiento y robustez. Fuente: <https://learn.microsoft.com/en-us/playready/overview/security-level> (consultada 2026-11-25).
- Microsoft entrega Server SDK, Device Porting Kit y certificados a licenciatarios/partners, que desarrollan y despliegan clientes y servidores. PlayReady aplica los derechos elegidos por el proveedor (caducidad, niveles de seguridad, restricciones de salida). Fuentes: <https://learn.microsoft.com/en-us/playready/overview/products-deliverables-services> y <https://learn.microsoft.com/en-us/playready/overview/overview> (consultadas 2026-11-25). Key system EME: `com.microsoft.playready` (fuente Shaka indicada arriba).

### FairPlay Streaming (Apple)

- FPS protege la entrega de medios en dispositivos Apple sobre HLS. El **FairPlay Streaming Server SDK** incluye guía de implementación, referencia, claves de desarrollo para implementadores del Key Server Module (KSM) y una app de ejemplo iOS. Requiere **membership en el Apple Developer Program** y está sujeto al Apple Developer Program License Agreement. Las credenciales de despliegue requieren aprobación: las solicita el *Account Holder* y se aprueban sólo si el equipo ofrece un servicio de streaming a consumidores; **no** se aprueban cuentas de terceros que actúen en nombre de dueños/licenciatarios de contenido. Hay streams de prueba. Fuente: <https://developer.apple.com/streaming/fps/> (consultada 2026-11-25).
- En la configuración EME, la cadena de robustez de FairPlay debe ser vacía según la documentación de Apple (fuente Shaka indicada arriba).

## Coste, requisitos legales y riesgo arquitectónico

- **Coste**: Widevine no cobra fee de uso, pero exige firmar el Master License Agreement y credenciales por organización; PlayReady exige una licencia de Microsoft negociada por tipo de producto; FPS exige membership de Apple y aprobación de credenciales. Ninguna de las fuentes públicas consultadas publica un precio de licencia cerrado para clientes, por lo que **no se afirma ninguna cifra**. El coste real de cada vía (integración, certificación, soporte multiplataforma) no está cuantificado públicamente y tampoco se inventa aquí.
- **Requisitos legales**: acuerdo de licencia firmado antes de tocar código (plan, línea 15), credenciales por organización, reglas de cumplimiento/robustez, restricciones de salida, protección de secretos y, en PlayReady, indemnización contractual. Sin licencia firmada no hay derecho a distribuir un CDM ni a anunciar compatibilidad.
- **Incompatibilidad con el proyecto**: los 18 crates llevan `#![forbid(unsafe_code)]` (veto absoluto, `AGENTS.md:44`; 18/18 en `docs/status/progress.toml`). Los CDM son binarios nativos propietarios (Widevine «natively embedded into the device platform»; PlayReady SL3000 exige TEE); integrarlos exige FFI/`unsafe` y un proceso aislado con confianza hardware, además de credenciales que hoy no existen. El DAG prohíbe dependencias circulares y límites de capa (`AGENTS.md:46-68`): `browser` no importa crates de motor directamente y no hay crate `media`. Aunque la spec prevé perfiles `deterministic`/`performance` y un proceso aislado para media (plan, línea 9), **sin licencia y sin auditoría del binario propietario no se puede cumplir el veto `forbid(unsafe_code)` ni la promesa de determinismo**. No hay base técnica ni legal para la vía (a) ni para la (b) hoy.

## Decisión

Se adopta la **vía (c) del plan**: **no soportar DRM/EME**. Concretamente:

1. No se integra ningún CDM de Widevine, PlayReady ni FairPlay; no se solicitan credenciales de producción ni se firman acuerdos en el marco de este WP.
2. No se promete reproducción de contenido protegido: **Netflix, Disney+, Amazon Prime Video, HBO Max ni ningún otro servicio que exija DRM quedan fuera de alcance declarado**.
3. El límite se publica en este ADR y se registraría en la matriz de soporte cuando Task 1 del plan esté implementada; la ejecución pendiente (bloquear EME con error tipado y mensaje claro, plan `:49`) queda como WP separado, no se implementa aquí.
4. Esta decisión no es una renuncia permanente: puede revisarse si se obtiene una licencia firmada **y** se aprueba una excepción arquitectónica documentada para el CDM.

La decisión deriva de la spec v2 aprobada en PR #2290 (línea 102) y del plan de medios (líneas 7 y 35-42); no se registra aprobación individual del dueño porque la autorización ya está en la spec vigente.

## Alternativas reales (sin DRM)

- **Codecs libres**: AV1, VP9 y Opus como vía de audio/vídeo sin DRM (plan Task 4, `:53-59`). Hoy no hay crate `media` ni dependencia `symphonia` en el workspace: Task 4 es trabajo futuro, no una capacidad ya entregada.
- **Imágenes**: AVIF ya está implementado como decodificador de imagen puro Rust (`avif-rust` 0.3, MIT, sin C/FFI) bajo ADR-005 en `docs/AVIF-STRATEGY.md:1-12`, con evidence en `crates/image/src/avif.rs` y `crates/image/tests/decode.rs`. JPEG XL sigue pendiente y no se afirma soporte.
- **EME Clear Key**: los *keys* van sin cifrar, es obligatorio en EME y sirve **sólo para pruebas**, no es un mecanismo de protección de contenido (fuente Shaka indicada arriba); no se presenta como alternativa de DRM.

## Límites explícitos

- No hay CDM licenciado, no hay credenciales de producción y no hay streams protegidos funcionando en Megi.
- No hay implementación EME ni matriz `docs/status/media-matrix.toml` medida; Task 1 y Task 3 del plan siguen abiertas.
- No se afirma compatibilidad con ningún servicio de streaming con DRM; no se promete Netflix.
- No se ha firmado acuerdo con Google, Microsoft ni Apple en el marco de este WP.
- Este ADR documenta el hueco permitido por la spec v2 (línea 102): visible y verificable, no oculto.

## Fuentes públicas consultadas (2026-11-25)

1. Google Widevine DRM overview — <https://developers.google.com/widevine/drm/overview>
2. Google Widevine access — <https://developers.google.com/widevine/access>
3. Shaka Player — DRM configuration tutorial — <https://shaka-project.github.io/shaka-player/docs/api/tutorial-drm-config.html>
4. Microsoft PlayReady overview — <https://learn.microsoft.com/en-us/playready/overview/overview>
5. Microsoft PlayReady license to use — <https://learn.microsoft.com/en-us/playready/overview/license-to-use-playready>
6. Microsoft PlayReady security level — <https://learn.microsoft.com/en-us/playready/overview/security-level>
7. Microsoft PlayReady products, deliverables and services — <https://learn.microsoft.com/en-us/playready/overview/products-deliverables-services>
8. Apple FairPlay Streaming — <https://developer.apple.com/streaming/fps/>
