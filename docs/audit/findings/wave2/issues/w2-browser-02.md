[render] `document.title` setter is a no-op: window/tab title never updates

## Resumen

Asignar `document.title` desde script no tiene ningún efecto observable: el setter solo escribe `host.title`, un campo que ningún consumidor de título lee. El título de ventana/pestaña se compone desde el elemento `<title>` del DOM (`crates/shell/src/title.rs: document_title`), que el setter nunca toca ni crea.

## Impacto

Contrato DOM roto y visible: cualquier página que actualice `document.title` (contadores, notificaciones, SPAs) muestra un título de ventana/pestaña obsoleto. P2.

## Pasos

1. Renderizar un documento con `<title>Original</title>`.
2. Ejecutar `document.title = "Nuevo";`.
3. Leer `document.title` → devuelve `"Nuevo"` (el getter lee `host.title`).
4. Componer el título de ventana (`shell::title::document_title` sobre el DOM) → sigue siendo `"Original"`; el elemento `<title>` no existe o conserva el texto viejo.

## Esperado vs observado

- Esperado (HTML Standard, `document.title`): el setter actualiza el elemento `<title>` del documento (creándolo en `<head>` si falta) y el título de ventana/pestaña refleja el cambio.
- Observado: solo cambia el campo interno `host.title`; el DOM y el título de ventana quedan intactos.

## Evidencia literal

`crates/js/src/runtime.rs` (HEAD auditado `3c06db8`):

```rust
fn document_title_set(
    _this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let title = arg_string(args, 0, context)?;
    with_host(|host| host.title = title);
    Ok(JsValue::undefined())
}
```

`host.title` no tiene más lectores en el árbol: `grep -n "host.title"` solo da la estimación de memoria (`runtime.rs:634`, `+ host.title.len()`) y el getter/setter (`runtime.rs:3778,3788`). En el lado de visualización, `crates/shell/src/title.rs` (`document_title`) recorre el DOM renderizado buscando el primer `<title>` HTML — nunca consulta `host.title`.

## Workaround

Ninguno desde script: no hay API alternativa que actualice el título de ventana.

## Arreglo sugerido

Sincronizar el setter con el elemento `<title>` (actualizar su texto, crearlo bajo `<head>` si falta), en vez de (o además de) escribir `host.title`.

Commit: `3c06db8`

Dedup: sin entradas previas — en `docs/audit/FINDINGS.md` el único `title` es `document_write` (#1125); nada en `docs/GAPS.md`, `issues-index.json` ni `gh issue list` abiertos (#1544, #1541, #1534, #1519, #1516).
