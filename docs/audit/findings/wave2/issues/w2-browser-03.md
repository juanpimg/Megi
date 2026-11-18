[render] worker loader errors leak absolute local paths to page scripts

## Resumen

Cuando `new Worker(url)` falla en un documento `file://`, el mensaje de error entregado al script de la página contiene la URL `file://` absoluta resuelta (directorio local completo). Un script puede sondear rutas y distinguir "existe / bloqueado / límite" por el contenido del mensaje: fingerprinting del layout del disco local.

## Impacto

Exfiltración menor de rutas locales hacia script de página, solo en documentos `file://` (el loader solo soporta `file://` en render). Sin remoto ni bypass de lectura (el contenido del fichero nunca se entrega). Endurecimiento; P3, publicable, no advisory.

## Pasos

1. Abrir un documento `file:///tmp/sitio/index.html` con scripts.
2. Ejecutar `try { new Worker("sub/worker.js"); } catch (e) { console.log(e.message); }` con `sub/worker.js` inexistente.
3. El mensaje contiene `worker script file:///tmp/sitio/sub/worker.js: ...` (ruta absoluta completa).
4. Repetir con un documento `https://` o variando rutas: el prefijo cambia (`local file access is blocked...` vs error de IO), revelando existencia/estructura de directorios locales.

## Esperado vs observado

- Esperado: el error expuesto al script no incluye la ruta absoluta resuelta (basename, URL original tal como se pasó, o mensaje genérico).
- Observado: la ruta resuelta completa llega al script vía `pending_error` → `onerror`/excepción no capturada.

## Evidencia literal

`crates/render/src/scripts.rs:760-` (`load_worker_source`, HEAD auditado `3c06db8`): cada variante de error incrusta `{url}` (la URL ya resuelta):

```rust
let parsed = Url::parse(url)
    .map_err(|error| js::JsError::Runtime(format!("worker script URL {url}: {error}")))?;
...
    return Err(js::JsError::Runtime(format!(
        "worker scripts only support file:// in render: {url}"
    )));
...
    return Err(js::JsError::Runtime(format!(
        "worker script {url}: local file access is blocked for this document"
    )));
...
let bytes = net::load_file_url_with_limit(&parsed, max_bytes)
    .map_err(|error| js::JsError::Runtime(format!("worker script {url}: {error}")))?;
String::from_utf8(bytes)
    .map_err(|error| js::JsError::Runtime(format!("worker script {url} is not UTF-8: {error}")))
```

Y `crates/js/src/runtime.rs` (`worker_constructor`, `3c06db8:8017-8024`) entrega ese texto al script sin sanear:

```rust
let resolved = host.resolve_url(&url);
...
let source = loader(&resolved).map_err(|error| js_error(error.to_string()))?;
```

## Workaround

Servir el documento por `http(s)` en vez de `file://`: el loader rechaza todo lo que no sea `file://` sin revelar rutas (a costa de perder workers locales).

## Arreglo sugerido

Sanear `resolved` en el error del worker loader: basename, URL original sin resolver, o mensaje genérico por clase de fallo.

Commit: `3c06db8`

Dedup: sin entradas previas — workers en ledger: #1221, #1138–#1140 (todos distintos); sin fila en `docs/GAPS.md`; sin colisión en `issues-index.json` ni `gh issue list` abiertos. Etiqueta `security` menor publicable sugerida (no advisory).
