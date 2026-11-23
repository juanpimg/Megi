# Browser gap matrix

> **Histórico (2026-11-12):** esta matriz describe el estado de esa fecha y **no es fuente de estado vigente**. Numerosas filas `absent` ya están implementadas (`js/src/xhr.rs`, `js/src/websocket.rs`, `css/src/query.rs`, `css/src/transform.rs`, `net/src/{csp,integrity,hsts,mixed_content}.rs`, formularios, `[hidden]`, HSTS/CSP/SRI). Consulta siempre `docs/status/coverage.html` y el código antes de actuar.

This file records **what Megi does not implement yet**, measured against what a
general-purpose browser has to do to load and run a normal page. It is the durable
counterpart of the gap-audit phase (F5) of the repository audit: every `absent` row
here is also a GitHub issue, so the list survives even if an issue gets closed.

- **Scope:** `crates/*/src` at `main`, cross-checked against the HTML/CSS/JS surfaces a
  page can reach and against the shell chrome a user can touch.
- **Method:** read every module header (they document the intended scope), then verify
  each claim against the code with grep plus targeted reads. Nothing was inferred from
  documentation alone; the evidence column names the file and line that proves the row.
- **Status values:** `present` = implemented and wired for pages; `partial` = present but
  missing a piece that changes behaviour; `absent` = no code path was found.
- **Raw matrices** (one file per area, with the full per-feature tables and the
  ranking by consequence): [`docs/audit/findings/f5/`](audit/findings/f5/).

Status counts across the eight matrices below: 31 present, 34 partial, 71 absent.

---

## 1. HTML

| Feature | Status | Evidence | Consequence |
| --- | --- | --- | --- |
| Parsing (full HTML5 tree construction, fragments) | present | `crates/html/src/tree_builder.rs` | — |
| Forms: rendering and input | partial | `crates/browser/src/forms.rs:1-24` | controls work, submission does not (see below) |
| **Form submission** (`action`, `method`, `FormData`, Enter/click) | **absent** | `crates/browser/src/lib.rs:134`, `forms.rs:20` | login, search and checkout do nothing (#1526) |
| **`[hidden]` attribute** | **absent** | no `[hidden]` rule in `crates/css/src/user_agent.rs` | hidden panels stay visible and take layout space (#1530) |
| **Shadow DOM / custom elements** | **absent** | `attachShadow`/`customElements` only appear as `feDropShadow` (`tree_builder.rs:322`) | framework components render empty (#1529) |
| **`srcset`/`sizes`, `<picture>`** | **absent** | `crates/render/src/images.rs:90-113` | `srcset`-only images never load (#1531) |
| **`<iframe>`** | **absent** | `crates/layout/src/tree_builder.rs:228-244` is a plain replaced box | maps, players, payments, SSO blank (#1532) |
| **`<canvas>`** | **absent** | same replaced box; `getContext` not implemented | charts, games, PDF.js blank (#1533) |
| **`<dialog>` / top layer** | **absent** | no `showModal`, no `::backdrop` | dialogs lay out in flow, no modality (#1535) |
| `<details>`/`<summary>` | absent | no UA rule for either | closed panels show all content |
| `<video>`/`<audio>` | absent | replaced box only | media sites inert |

## 2. CSS

| Feature | Status | Evidence | Consequence |
| --- | --- | --- | --- |
| Cascade, specificity, inheritance, custom properties | present | `crates/css/src/cascade.rs` | — |
| 334 property arms, gradients, shadows, borders, flex, grid | present | `crates/css/src/properties.rs`, `layout/` | — |
| **`transform`** | **absent** | no field in `ComputedStyle` (`properties.rs:1962-2164`) | centred modals, carousels, hover scales misplaced (#1527) |
| **`::before`/`::after` + `content`** | **absent** | `selector.rs:18-20` "pseudo-element boxes pending" | icon fonts, badges, clearfix gone (#1528) |
| **`@supports`** | **absent** | `parser.rs:235` handles only import/font-face/keyframes/media/container/layer | whole feature-detection blocks dropped (#1537 bucket) |
| `@media` features | partial | `media.rs:402` knows width/height/orientation only | dark mode, touch, HiDPI blocks vanish (#1522) |
| `position: sticky` | partial | no scroll container in layout (`block.rs:1379-1385`) | sticky headers scroll away |
| Animations / transitions runtime | partial | `animation_runtime.rs` exists | tracked as #941 |

## 3. JavaScript and Web APIs

| Feature | Status | Evidence | Consequence |
| --- | --- | --- | --- |
| ES modules, top-level await, import maps | present | `crates/js/src/runtime.rs` module graph | — |
| ~45 globals (window, document, fetch, storage, workers, MutationObserver…) | present | `crates/js/src/runtime.rs`, `host.rs` | — |
| **Live event loop for pages** | **absent** | the shell has no `js` dependency; JS only runs inside `render_url` (`shell/src/worker.rs:517`) | scripts run at load time only; no timers/events after paint |
| **`navigator`, `performance`, `document.cookie`, `crypto`, `TextEncoder`** | **absent** | `rg` over `crates/js/src` | feature detection fails, sessions break (#1496–#1505) |
| **`Element.querySelector(All)`** | **absent** | not in the prelude | the most-used DOM API is missing (#1500) |
| **`XMLHttpRequest`** | **absent** | `rg XMLHttpRequest` → 0 | legacy sites never load (#1541) |
| **`WebSocket`** | **absent** | `rg WebSocket` → 0 | real-time sites inert (#1542) |
| **EventSource / Service Workers** | **absent** | `rg` → 0 | no live feeds, no offline |
| **Canvas 2D / WebGL** | **absent** | no `getContext` | ~700+ LOC away (#1533) |
| **`Blob`/`FormData` upload** | **absent** | `rg multipart` in `net` → 0 | file upload impossible |
| Input events (keyboard, input, focus, popstate, hashchange) | absent | the shell never dispatches user input into JS | interactive pages are static |

## 4. Networking

| Feature | Status | Evidence | Consequence |
| --- | --- | --- | --- |
| HTTP/1.1 (chunked, trailers, 100-continue) | present | `crates/net/src/http.rs` | — |
| HTTP/2 (HPACK incl. Huffman, flow control, multiplexing, h2c) | present | `crates/net/src/h2/` | — |
| TLS 1.2/1.3, SNI, ALPN | present | `http.rs:1655-1740` | — |
| Cache (RFC 9111 subset), cookies + SameSite + PSL, CORS validation, referrer policy, ranges, compression, proxies (HTTP/SOCKS5/PAC) | present | `cache.rs`, `cookies.rs`, `cors.rs`, `referrer.rs`, `range.rs`, `proxy.rs` | — |
| **HTTP/3 / QUIC** | **absent** | no QUIC dependency | h3-only origins unreachable |
| **HSTS** | **absent** | `rg hsts` → 0 | downgrade attacks possible (#1529 bucket → #1529/#1538) |
| **CSP** | **absent** | `rg content-security-policy` → 0 | injection = code execution (#1536) |
| **Subresource Integrity** | **absent** | `import_map.rs:12` documents it is ignored | CDN/MITM script substitution (#1537) |
| **Mixed-content blocking** | **absent** | `render/src/lib.rs:742-780` has no scheme check | https pages run http scripts (#1538) |
| `fetch()` cross-origin | partial/broken | `scripts.rs:811-827` rejects before CORS runs | CORS implementation unreachable (#1540) |
| Auth beyond URL Basic | partial | `http.rs:1587,513` | 401/407 render their body (#1544) |
| Redirect policy | partial | `http.rs:810-935` | no manual/error mode, downgrade allowed (#1545) |
| `document.cookie` | **absent** | no JS surface | sessions/CSRF break (#1541 bucket) |

## 5. Security controls

Sixteen control greps (CSP, SRI, mixed content, secure context, HSTS, CT, COOP/COEP/CORP,
X-Frame-Options, sandbox, Content-Disposition, WebSocket, EventSource, ServiceWorker,
multipart, DoH, WWW-Authenticate, Sec-Fetch, PNA) return **zero matches** in `crates/*/src`
for all but TLS validation, Referrer-Policy, cookie flags and the same-origin fetch guard.
That is the honest summary: the transport is solid, the *policy layer* a browser applies on
top of it is not implemented. Tracked as #1536–#1547.

## 6. Shell / UX

Present: tabs, history/navigation, address bar, find, zoom, reader mode, downloads
(partial), context menu, `file://` and `about:` pages, session restore, favicons, error
pages, keyboard shortcuts.
Absent: bookmarks, **scrollbars**, devtools/view-source, print, fullscreen, page
copy/paste, drag & drop, multiple windows (`window.open`), dialogs (`alert`/`confirm`/
`prompt`, #1521), autofill/permissions, PDF/media, smooth scrolling, dark mode (#1522).

## 7. Accessibility

`crates/browser/src/a11y.rs` computes roles, accessible names and a focus order, and nothing
consumes them: `shell/src/lib.rs:206` only re-exports the module and the page is drawn as
pixels (`chrome.rs:1408`). Consequences: screen readers see a blank window (#1516),
keyboard cannot reach links and `:focus` never matches (#1517), no `label[for]`, no focus
management, no `alt` rendering, chrome not AT-operable.

## 8. Internationalisation

`dir` is stored but never mapped to the CSS `direction` property (#1518); there is no IME
support, so CJK and dead-key input cannot reach any field (#1519); no UAX #14 line breaking
for CJK text (#1520); `:lang()` is rejected and `Intl`/`toLocale*` do not exist, with
Spanish/English UI strings hardcoded (#1523); bidi is a UAX #9 subset with known word
reversal and override gaps (#1447, #1448); no `writing-mode`.

---

## What this means for "a complete browser"

Ordered by how much of the web breaks without it:

1. **Form submission and a live event loop** — without them a page renders but cannot be
   used; this is the single largest functional gap.
2. **`Element.querySelector`, `navigator`, `document.cookie`, `performance`,
   `TextEncoder`** — cheap shims that unblock most modern sites (#1496–#1505).
3. **The policy layer: CSP, SRI, mixed content, HSTS** — the code is a network client, not
   yet a browser that can safely load untrusted pages (#1536–#1539).
4. **`transform`, `::before`/`::after`, `@supports`** — the three CSS features whose absence
   visibly breaks layout on mainstream sites (#1527, #1528, #1537).
5. **`<iframe>`, `<canvas>`, `<dialog>`, `srcset`** — embed and rendering surfaces.
6. **Accessibility and IME** — the difference between "renders a page" and "usable by a
   person" (#1516–#1520).

## Keeping this file honest

- A row moves from `absent`/`partial` to `present` only when code plus a test lands; update
  this table in the same PR.
- New gaps found by later audits get a row here **and** an issue; the issue may be closed,
  the row stays until the code exists.
- The audit ledger (`docs/audit/coverage.tsv`, enforced by
  `python3 tools/audit_coverage.py --check`) tracks *which files were read*, not which
  features exist; this file is the feature-side counterpart.
