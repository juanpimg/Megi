# Ola 2 — issues publicados y avisos privados

Fecha: 2026-11-18. Base auditada: `3c06db8` (worktree limpio). Publicados con `gh issue create` por la sesión controladora tras mesa de verificación (A reproductor + B fiscal + C chair).

## Issues públicos (73)

| Ámbito | IDs → issue |
|---|---|
| js | w2-js-001→#1618, w2-js-003→#1619, w2-js-004→#1620, w2-js-005→#1621, w2-js-006→#1622, w2-js-008→#1623, w2-js-009→#1624 |
| net | w2-net-004→#1625, 005→#1626, 006→#1627, 007→#1628, 008→#1629, 009→#1630, 010→#1631, 011→#1632, 012→#1633, 013→#1634, 014→#1635, d02→#1636, d03→#1637, d04→#1638, d05→#1639, d06→#1640, d07→#1641, d08→#1642, d09→#1643, d10→#1644 |
| browser | w2-browser-02→#1645, w2-browser-03→#1646 |
| tests/fuzz | w2-t-01→#1647, 02→#1648, 03→#1649, 04→#1650, 05→#1651, 06→#1652, 07→#1653, 08→#1654 |
| assets | w2-assets-03→#1655, 04→#1656, 05→#1657, 06→#1658, 07→#1659, 09→#1660, 10→#1661, 11→#1662, C1→#1663 |
| render | w2-render-02→#1664, 04→#1665, 10→#1666 |
| core | w2-core-01→#1667, 02→#1668, 03→#1669, 04→#1670, 05→#1671, 06→#1672, 07→#1673, 08→#1674, 09→#1675, 10a→#1676, 10b→#1677, 11→#1678, 12→#1679, 13→#1680 |
| CI/perf | w2-ci-01→#1681, w2-ci-03→#1682, w2-perf-01→#1683, 02→#1684, 03→#1685, 04→#1686, 05→#1687 |
| shell | w2-st-headless-tabs→#1689, w2-st-reader-quadratic→#1690, w2-st-session-doc-drift→#1691 |

## Avisos privados (7) — NO publicar como issues

Borradores en `advisories/` y en `meeting-net.md` / `meeting-render.md`:

- `w2-net-001` cookie `Set-Cookie` de PUSH antes del gate CORS (#1252)
- `w2-net-002` handshake WS acepta status bogus (`contains(" 101")`)
- `w2-net-003` máscara de cliente WS fija
- `w2-net-d01` handshake sin validar `Upgrade`/`Connection`
- `w2-render-01` DoS por amplificación de iframes (~70k renders desde ~1 KiB)
- `w2-st-1173-file-icon` favicon `file:` desde página http(s)
- `w2-st-1176-tmp-writers` writers atómicos con `.tmp` predecible

## Reuniones (actas)

`meeting-{core,js,net,render,assets,lp,st,f1,ci,browser}.md`. Cada una contiene la tabla final y las secciones `comment-existing` / `maintenance` / `exclude` con sus motivos. Los `comment-existing` no se publicaron para evitar ruido en issues cerrados; quedan documentados en el repositorio.
