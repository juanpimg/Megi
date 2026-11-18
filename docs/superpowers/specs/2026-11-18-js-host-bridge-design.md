# Diseño — Puente seguro entre Boa y el estado del motor

**Fecha:** 2026-11-18 · **Estado:** propuesta para aprobación · **Alcance:** `crates/js` únicamente

## 1. El problema en palabras sencillas

El motor de JavaScript (Boa) llama a funciones de Rust ("bindings") para leer la página, crear temporizadores, etc. Todas esas funciones comparten **un único objeto `Host`** con ~77 campos, guardado en un `Rc<RefCell<Host>>` dentro de una variable TLS (`CURRENT_HOST`).

`RefCell` permite **un solo usuario mutable a la vez**. Cuando un binding está usando el `Host` y, sin soltarlo, llama a otro binding (por ejemplo: despachar un evento → escucha JS → `document.body.appendChild`), el segundo intenta tomar el mismo `Host` y el programa entra en pánico: `already borrowed`.

**No es un fallo de comportamiento hoy**: los ~2.831 tests pasan y el caso real de eventos anidados funciona. Es una **debilidad estructural**: cualquier binding nuevo que llame a JS mientras mantiene el préstamo puede romper el motor en producción.

### Intentos descartados (no repetir)

1. **Copia desacoplada del `Host`** (`detached_copy`/`merge_from`): se implementó y se revirtió. La copia se queda obsoleta cuando el binding exterior crea nodos DOM que el interior busca después → 10+ tests fallan con `invalid DOM node handle 0`.
2. **Capturas de `NativeFunction`** (`from_copy_closure_with_captures`): exige que la captura implemente `Trace`; `Host` contiene `Document`, `HashMap` y `Box<dyn Fn>`, por lo que haría falta una implementación manual invasiva. Además obligaría a un registro global con `Mutex`, que no es reentrante.
3. **Cola de mensajes con hilo propio**: Boa ejecuta los bindings en el mismo hilo; no hay nadie que bombee la cola mientras el binding espera la respuesta → bloqueo.

## 2. Debate de subagentes y decisión

Tres subagentes analizaron el código en paralelo con enfoques distintos:

| Enfoque | Fortaleza | Debilidad |
|---|---|---|
| **A. Dividir `Host` en 7 celdas** (DomState, EventState, TimerState, ObserverState, StorageState, ScriptState, ProviderSet) | Reduce el alcance de cada préstamo; operaciones cruzadas legales | Mantiene TLS + `Rc`; no elimina el riesgo de doble préstamo dentro de la misma celda; 272 llamadas a migrar |
| **B. `HostState` por valor en `Context` + guardián `park/resume`** | Sin TLS ni `Rc`; respuestas síncronas | ~200 puntos deben "aparcar" el estado antes de llamar a JS; un olvido se convierte en error de JS en producción |
| **C. `HostDefined` + `host_cell(context)`** | El compilador impide mantener `&mut Context` y el préstamo a la vez; un solo `Host` vivo; sin copias; APIs 100 % seguras | Requiere migrar los 272 sitios y revisar enlaces entre realms (workers/iframes) |

**Decisión:** **C como núcleo, con A y B como refuerzos puntuales.**

- **C** elimina el TLS y el `Rc<RefCell<Host>>` principal: el `Host` vive dentro del `Context` de Boa y cada binding lo toma prestado desde el `&mut Context` que ya recibe. Mantener el préstamo mientras se llama a una función que usa `&mut Context` es un **error de compilación**, no un pánico en producción.
- **A** se usa solo para las dos operaciones que necesitan créditos cruzados (despacho de eventos y entrega de `MutationObserver`): separar `EventState`/`ObserverState` de `DomState` permite tomar el estado de eventos, hacer un snapshot de `Vec<NodeId>`, soltar el préstamo y luego resolver nodos. La página y su tabla de manejadores (`document`, `nodes`, `node_index`) **permanecen juntos** para no repetir el error de la copia obsoleta.
- **B** se usa para los providers: viven en un slot aparte como `Arc<dyn Fn…>` inmutable. Un provider que vuelve a llamar a JavaScript encuentra el `Host` aparcado en el `Context` y no puede provocar doble préstamo.

## 3. Contrato objetivo

```rust
// crates/js/src/host.rs
pub(crate) fn install_host(context: &mut Context, host: Rc<RefCell<Host>>); // interim: the SAME Rc owned by JsRuntime
pub(crate) fn host_cell(context: &mut Context) -> JsResult<RefMut<'_, Host>>;
pub(crate) fn host_ref(context: &Context) -> JsResult<Ref<'_, Host>>;

// slot aparte, inmutable
pub(crate) fn install_providers(context: &mut Context, providers: HostProviders);
pub(crate) fn providers(context: &Context) -> Option<&HostProviders>;

// Helpers de dominio (solo donde el préstamo cruzado lo exige)
pub(crate) fn with_events<R>(context: &mut Context, f: impl FnOnce(&mut EventState) -> R) -> JsResult<R>;
pub(crate) fn with_observers<R>(context: &mut Context, f: impl FnOnce(&mut ObserverState) -> R) -> JsResult<R>;
```

Durante la migración `JsRuntime` conserva el mismo `Rc<RefCell<Host>>` y el `Context` guarda **ese mismo Rc**; no se crea un segundo `Host`. Al final se elimina el campo `host` y el `Rc` restante. Se eliminan `CURRENT_HOST`, `HostScope`, `current_host()` y `with_host()`.

**Reglas de oro para los bindings:**
1. Tomar el préstamo al entrar; **soltarlo antes** de `callback.call`, `obj.get/set`, `array_*`, providers o cualquier función que reciba `&mut Context`.
2. Nunca guardar un `Ref`/`RefMut` a través de una llamada a JS.
3. Si hace falta volver a entrar, usar `try_borrow_mut()` y devolver un `JsError` tipado en lugar de hacer pánico.
4. Providers = `Arc`, nunca `Box`, y se invocan **fuera** de cualquier préstamo.

## 4. Migración por capas

Cada fase termina con `cargo test -p megi-js` en verde y sin cambios de comportamiento.

1. **Instalar el cell en el `Context`** junto al TLS actual (puente en paralelo). Sin cambios observables.
2. **Migrar los bindings hoja** (consola, storage, timers, viewport/media): `with_host` → `host_cell(context)`.
3. **Migrar el despacho de eventos** con snapshot-then-call; dividir `EventState` de `DomState`.
4. **Migrar DOM y `MutationObserver`**; dividir `ObserverState`; separar providers a `Arc<HostProviders>`.
5. **Migrar module loader, workers e iframes**; sustituir los enlaces `Rc<RefCell<...>>` entre realms por buzones propios drenados por el pump existente.
6. **Borrar el TLS y el campo `host`**; pasar `try_borrow_mut` en todo el puente; auditoría final.

## 5. Criterios de aceptación

- `grep -R 'CURRENT_HOST\|with_host\|Rc<RefCell<Host>>' crates/js/src` → 0 resultados.
- `python3 tools/check_arch.py` → 0 violaciones nuevas (los `Rc`legacy se eliminan del baseline).
- `cargo test -p megi-js` → todos los tests (312+) pasan, incluidos los nuevos test dereentrancia de providers y de despacho anidado.
- `cargo test --workspace`, gate `PASS`, PNG byte-idéntico.
- Ninguna función pública nueva con `unsafe` o con `Rc`.
