# Verificación en vivo de accesibilidad (F7-A11Y / #989)

## Diagnóstico del bus AT-SPI en este entorno

1. `NO_AT_BRIDGE=1` estaba definido globalmente: desactiva el puente ATK en aplicaciones GTK
   (Orca no aparecía como cliente AT). Hay que arrancar Megi/AT con `env -u NO_AT_BRIDGE`.
2. `at-spi-dbus-bus.service` estaba enmascarado. Tras `systemctl --user unmask/start` el bus
   `org.a11y.Bus` respondió, pero `org.a11y.Status.IsEnabled` era `false`.
3. `accesskit_unix` activa su adaptador **sólo** cuando `org.a11y.Status.IsEnabled == true`.
   Con `dbus-send ... Properties.Set ... IsEnabled variant:boolean:true` el adaptor se activó.

## Bug real encontrado y corregido

Al activarse, Megi entraba en pánico:

```
thread 'main' panicked at accesskit_consumer-0.39.2/src/tree.rs:318:13:
TreeUpdate includes 3 nodes which are neither in the current tree nor a child of another node
```

Los nodos del snapshot cuyos ancestros DOM no tenían rol quedaban sin padre en el `TreeUpdate`.
`build_tree_update` ahora los monta bajo la raíz expuesta; test determinista:
`crates/browser/src/a11y_backend_orphan_tests.rs::snapshot_nodes_without_an_exposed_parent_mount_under_the_root`
(RED con el pánico exacto → GREEN).

## Árbol AT real y acción (X11 :10.0, `gi.repository.Atspi`)

```
=== MEGI LIVE AT TREE ===
application name='megi' checked=False
 heading name='Accessibility live test' checked=False
  button name='Press me' checked=False
  check box name='Remember me' checked=True
  link name='Go to target' checked=False
LINK ACTION: Go to target -> True

# título de la ventana antes: AT_LIVE_PAGE — file:///tmp/a11y-final.html
# título después de do_action(0):
[2/2] AT_TARGET_REACHED — file:///tmp/a11y-target.html
```

El cliente AT (usando la API real `Atspi`, no el sustituto `AtBus`) leyó roles, nombres y el
estado `CHECKED` del checkbox, e invocó una acción que navegó la página. No se capturó salida
de voz de Orca (no se registró como cliente); el verificado es el protocolo AT-SPI, que es lo
que consume cualquier lector de pantalla.

## Límites documentados

- No se verifica el rasterizado de controles por el sistema operativo (Megi dibuja su propio
  chrome y widgets con estilo nativo); la accesibilidad se verifica a nivel de protocolo AT.
- La verificación en vivo es manual; CI ejecuta el test determinista del árbol y el paseo con
  `accesskit_consumer`, que son la parte automatizable sin bus AT.
- Tras la prueba se restauró el entorno: bus enmascarado, `IsEnabled=false`, `NO_AT_BRIDGE`
  intacto y procesos terminados.
