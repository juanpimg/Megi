# JavaScript + Web API gap matrix — Megi engine

Repo: upstream `main` (9c94e86) @ `9c94e86` ("Merge pull request #1491 … fix/issue-1373-normalize"), read-only.
Subject: `crates/js/` (Boa 0.22 embedded runtime) and the host wiring in `crates/render/src/scripts.rs` and
`crates/shell/`.
Method: read `crates/js/src/{runtime,host,events,import_map,error,lib}.rs` (10,175 lines; there is no
`module.rs` — module loading lives in `runtime.rs` + `import_map.rs`), the 20 files in `crates/js/tests/`,
the render script host providers, and the shell worker. No cargo was run; every claim below is a code
citation.

**Headline.** The runtime is a *pre-render* JS environment: it installs ~45 globals, runs page scripts
during a deterministic render pass (lifecycle events + a virtual-time timer loop) and then drops the
realm. It has a credible ES-module story (import maps, top-level await) and a useful `fetch` shim, but it
has **no `navigator`, no `performance`, no `document.cookie`, no `TextEncoder`, no `scrollY`, no
`Element.querySelector`**, and the winit shell never dispatches user input into JS. That is the exact
opposite of what mainstream sites assume, so the gap list below is dominated by cheap missing shims.

---

## 1. What is actually installed (verified from the code)

| Global / interface | Evidence | Notes |
|---|---|---|
| `window` (the global object itself) | `crates/js/src/runtime.rs:618-640` | `window_value = JsValue::from(global.clone())` at :621 |
| `console` | `runtime.rs:808-824` | exactly 8 methods: log, info, warn, error, debug, assert, table, dir (no `time`, `count`, `group`, `trace`) |
| `document` | `runtime.rs:893-1001` | see §2 for methods/properties |
| `history` | `runtime.rs:702`, installed `:642-690` | pushState/replaceState/back/forward/go + `length`/`state`; actions are *queued* for the host |
| `location` | `runtime.rs:715` | assign/replace/reload + href/pathname/search/hash/origin/protocol/host |
| `localStorage`, `sessionStorage` | `runtime.rs:788` | getItem/setItem/removeItem/clear/key/`length`; `QuotaExceededError` at 1 MiB per realm (`runtime.rs:3391-3393`) |
| `Worker` | `runtime.rs:5254-5767` | max 8 live, 256 queued messages, JSON-only payloads, no `importScripts` |
| `MutationObserver` | `runtime.rs:~670`, caps in `host.rs` | records attribute/childList mutations |
| `Event`, `CustomEvent` | `events.rs:152-160`, ctors `:374`, `:406` | real capture/bubble dispatch, `preventDefault`, `stopImmediatePropagation` (`events.rs:632-700`) |
| `setTimeout`/`setInterval`/`clearTimeout`/`clearInterval` | `events.rs:1362` `timer_globals()`, cap `events.rs:1358` | 4096 pending timers |
| `requestAnimationFrame`/`cancelAnimationFrame` | `events.rs:163-172`, impl `:1425`, `:1434` | queued 16 ms ahead on the virtual clock |
| `addEventListener`/`removeEventListener`/`dispatchEvent` on window, document, every element | `events.rs:194-203`, `runtime.rs:1146-1150`, `runtime.rs:989-997` | options object with capture/once/passive (`events.rs` Listener) |
| `getComputedStyle`, `matchMedia`, `scrollTo`, `scrollBy` | `events.rs:199-202`, impls `:1181`, `:1059`, `:1114`, `:1124` | backed by host snapshot/media providers |
| `innerWidth`, `innerHeight`, `devicePixelRatio`, `pageXOffset`, `pageYOffset` | `events.rs:174-180` | **no `scrollX`/`scrollY`** |
| `onclick`, `onload`, `onerror` (window, document, elements) | `events.rs:280-284` | only these three inline handlers |
| `URL`, `URLSearchParams` | prelude `runtime.rs:4831`, `:4855` (installed by `install_gap_apis`, `:4676`) | custom JS parser, no IDNA/punycode, no `createObjectURL` |
| `btoa`, `atob`, `structuredClone` | `runtime.rs:4864`, `:4876`, `:4898` | clone: plain objects/arrays/Date/Map/Set, cycles throw; no transferables, no DOM nodes |
| `queueMicrotask`, `requestIdleCallback`, `cancelIdleCallback` | `runtime.rs:4899-4902` | idle callback always reports `timeRemaining() === 50` |
| `MouseEvent`, `PointerEvent` | `runtime.rs:4903-4916` | shims with clientX/Y, button, pointerId |
| `Node` constants object | `runtime.rs:4917` | |
| `Headers`, `AbortSignal`, `AbortController`, `ReadableStream` | `runtime.rs:4918`, `:4949`, `:4971`, `:4980` | headers validated against CR/LF/NUL/colon |
| `Response`, `Request`, `fetch` | `runtime.rs:5021`, `:5047`, `:5066` + native `fetch_native` (`runtime.rs:5740+`) | string bodies only; same-origin only; 64 headers / 8 MiB body / 64 calls / 32 MiB per page (`crates/render/src/scripts.rs:96-104`) |

Everything else in the Web platform is absent. `rg` over `crates/js/src` for `navigator`, `performance`,
`cookie`, `TextEncoder`, `TextDecoder`, `Blob`, `File`, `FileReader`, `FormData`, `WebSocket`,
`EventSource`, `IntersectionObserver`, `ResizeObserver`, `customElements`, `attachShadow`, `getSelection`,
`clipboard`, `Notification`, `geolocation`, `indexedDB`, `serviceWorker`, `SharedWorker`, `OffscreenCanvas`,
`getContext`, `MessageChannel` returns **no implementation site** (the only hits are prose in `lib.rs`
and the `cookie` string in the forbidden-header list at `runtime.rs:5798`).

## 2. `document` and `Element` surface (the part sites actually touch)

`document` (`runtime.rs:893-1001`) has: `write`, `writeln`, `getElementById`, `getElementsByTagName`,
`getElementsByClassName`, `querySelector`, `querySelectorAll`, `createElement`, `createTextNode`,
`createDocumentFragment`, `normalize`, `compareDocumentPosition`, `close`, `addEventListener`,
`removeEventListener`, `dispatchEvent`, plus properties `documentElement`, `body`, `head`, `title`,
`activeElement`, `URL`, `readyState` and the three inline handlers.

It has **no** `cookie`, `referrer`, `baseURI`, `visibilityState`/`hidden`, `currentScript`, `doctype`,
`createComment`, `createEvent`, `importNode`, `adoptNode`, `hasFocus`, `execCommand`.

Every node wrapper (`runtime.rs:1007-1150`) exposes 33 accessors — `nodeType`, `tagName`, `id`,
`className`, `textContent`, `innerHTML`, `outerHTML`, `children`, `parentNode`, `firstChild`, `lastChild`,
`nextSibling`, `previousSibling`, `style`, `classList`, `childNodes`, `value`, `checked`, `disabled`,
`selected`, `options`, `innerText`, `dataset`, `attributes`, `offsetWidth`, `offsetHeight`, `clientWidth`,
`clientHeight`, `title`, `lang`, `href`, `src`, `selectedIndex` — and 21 methods + the 3 event methods:
`setAttribute`, `getAttribute`, `getAttributeNode`, `removeAttributeNode`, `removeAttribute`,
`hasAttribute`, `appendChild`, `insertBefore`, `removeChild`, `replaceChild`, `matches`, `closest`,
`insertAdjacentHTML`, `insertAdjacentText`, `cloneNode`, `normalize`, `compareDocumentPosition`, `focus`,
`blur`, `scrollIntoView`, `getBoundingClientRect`.

Missing on Element: `querySelector`/`querySelectorAll` (they exist only on `document`), `click()`,
`append`/`prepend`/`before`/`after`/`replaceWith`/`remove`, `contains`, `hasChildNodes`, `getAttributeNames`,
`children` is a static array (not a live `HTMLCollection`), no `Element`/`HTMLElement` constructor at all
(`instanceof Element` throws), no `DOMRect` class (getBoundingClientRect returns a plain object,
`runtime.rs:5146-5186`).

`getBoundingClientRect`/`offsetWidth` work only when the host injects a layout provider
(`runtime.rs:5119-5230`); without one they return zeros, and `scrollIntoView` is a no-op.

## 3. Matrix

Status legend: **present** = usable subset that matches the spec for common code; **partial** = the global
exists but a spec-common part is missing or behaves unlike a browser; **absent** = the global is never
installed.

| # | Bucket | Status | Evidence | What is missing | Real-site class that breaks | Effort (LOC, crate) |
|---|---|---|---|---|---|---|
| 1 | Live DOM event loop / interactivity | absent | shell deps (`crates/shell/Cargo.toml`) contain no `js`; input is handled in Rust (`crates/shell/src/forms.rs`); JS runs only inside `render::render_url` (`crates/shell/src/worker.rs:517,520`), which drops the realm | no persistent runtime, no bridging of click/input/key events into JS, no microtask turns after load | every interactive site: menus, tabs, validation, SPA routing | 1,500–3,000 (render + shell + js) |
| 2 | `navigator` | absent | no install site in `crates/js/src` | userAgent, language(s), platform, onLine, hardwareConcurrency, plugins, sendBeacon, permissions | UA/feature gates, analytics bootstraps, "unsupported browser" walls | 120 (js) |
| 3 | `document.cookie` | absent | `runtime.rs:5798` only lists `cookie` as a forbidden fetch header | get/set with SameSite/Path/Domain semantics bridging `net::cookies` | consent banners, sessions, A/B bucketing | 180 (js + net) |
| 4 | Scroll geometry | partial | window accessors end at `pageXOffset`/`pageYOffset` (`events.rs:174-180`); no `scrollTop`/`scrollHeight` in `runtime.rs` | `window.scrollX/scrollY`, `Element.scrollTop/scrollLeft/scrollHeight/scrollWidth`, `offsetTop/offsetLeft/offsetParent`, `clientTop`, `DOMRect` | sticky headers, scroll-spy nav, infinite scroll, carousels | 120 (js) |
| 5 | `performance` | absent | no install site | `now()`, `timeOrigin`, `mark/measure`, `PerformanceObserver` | analytics, React/Vue devtools, animation timing | 80 (js) |
| 6 | `TextEncoder` / `TextDecoder` | absent | no install site | UTF-8 encode/decode (+ stream forms) | every modern bundle polyfill, binary fetch handling | 200 (js) |
| 7 | `Element.querySelector(All)` | absent | `crates/js/src/runtime.rs:1120-1150` method list; selectors only on `document` (`:916-940`) | element-scoped selector queries | virtually all component code (`el.querySelector('.x')`) | 40 (js) |
| 8 | `crypto` | absent | only doc mention `lib.rs:186`; no `Crypto` object | `getRandomValues`, `randomUUID`, `subtle` | token/ID generation, `crypto.randomUUID()` in frameworks | 50 (js) |
| 9 | Programmatic activation | absent | no `click` in the element method list (`runtime.rs:1120-1150`) | `Element.click()`, `HTMLElement.tabIndex` | custom buttons/links, test harnesses, mobile menus | 30 (js) |
| 10 | History events / SPA routing | partial | `history` queues `HistoryAction` to the host (`runtime.rs:702`); no `popstate` dispatch anywhere | `popstate`, `hashchange`, `navigation` events; pushState does not change the live URL for scripts | React Router/Vue Router/Turbo/HTMX | 120 (js + render) |
| 11 | Keyboard/input/focus events | partial | only `Event`, `CustomEvent`, `MouseEvent`, `PointerEvent` (`events.rs:152-160`, `runtime.rs:4903-4916`); inline handlers limited to click/load/error (`events.rs:280-284`) | `KeyboardEvent`, `InputEvent`, `FocusEvent`, `WheelEvent`, `SubmitEvent`, `DragEvent`, real key/input dispatch from the shell | autocomplete, menus, form UX, hotkeys | 200 (js + shell) |
| 12 | Form validity / submission | partial | `value`/`checked`/`disabled`/`selected`/`options`/`selectedIndex` accessors (`runtime.rs:1630-1798`); shell notes "Enter never submits" (`crates/shell/src/forms.rs:20`, `cli.rs:73`) | `HTMLFormElement.submit/requestSubmit/elements`, `validity`/`checkValidity`/`reportValidity`, `setCustomValidity`, `input.files`, `setSelectionRange` | login/search/newsletter forms | 300 (js) |
| 13 | `HTMLDialogElement`, `template.content` | absent | `document.createElement` accepts any name (`runtime.rs:2209`) but `template.content`/`dialog.showModal` are absent | modal dialogs, template cloning | cookie banners, modals, web components | 120 (js) |
| 14 | Canvas 2D / WebGL / OffscreenCanvas / `Image` | absent | no `getContext` in `crates/js/src`; paint rasterizes layout only | all raster drawing APIs | charts, maps, games, image editors, captchas | 700+ (paint or new crate) |
| 15 | WebSocket / EventSource | absent | no install site; net has no WS client | WS client + EventSource | chat, live dashboards, notifications | 600 (net + js) |
| 16 | Observers beyond MutationObserver | partial | `IntersectionObserver`, `ResizeObserver`, `PerformanceObserver` absent | element visibility/size observation | lazy loading, virtual lists, responsive widgets | 400 (js + render) |
| 17 | Binary/upload types: `Blob`, `File`, `FileReader`, `FormData` | partial | only `btoa`/`atob` and `Uint8Array` bodies (`runtime.rs:4864-4876`, `:5740+`) | Blob/File/FileReader/FormData, `URL.createObjectURL`, ArrayBuffer request bodies | uploads, previews, clipboard images, PDF export | 400 (js) |
| 18 | `customElements` / Shadow DOM | absent | no `customElements`, `attachShadow`, `ShadowRoot`, `HTMLElement` | web components | design systems (Shoelace, Lit, FAST) | 800 (js + css/paint) |
| 19 | Storage beyond localStorage | partial | `localStorage`/`sessionStorage` only (`runtime.rs:788`); hard 1 MiB quota (`runtime.rs:3391-3393`) | `storage` events (cross-tab), IndexedDB, Cache API | offline apps, PWAs, editor autosave | 80 for events; 1,500+ for IndexedDB |
| 20 | `window` messaging & navigation | partial | no `postMessage`, `MessageChannel`, `open`, `close`, `name`, `top`, `parent` in `runtime.rs` | cross-frame/worker messaging, popups | embeds, OAuth popups, worker RPC | 300 (js) |
| 21 | Misc platform APIs | absent | no install sites | `alert/confirm/prompt`, `Notification`, `Clipboard`, `geolocation`, `getSelection`, `CSS.escape/supports`, `matchMedia` change events (`events.rs:1087` registers but never fires) | copy buttons, toasts, permissions flows | 50 each (js) |
| 22 | ES modules | present | `MegiModuleLoader` + `eval_module_async` (`runtime.rs:5810-6400`), import maps (`crates/js/src/import_map.rs`), real top-level await with a 10,000-iteration budget | `scopes`/`integrity` ignored (`import_map.rs:1-18`) | — (this is the strongest area) | — |
| 23 | Fetch | partial | prelude `runtime.rs:5066` + native `fetch_native`; host provider `crates/render/src/scripts.rs:786-905` | same-origin only, no cookies/credentials, string bodies only, 64 calls/32 MiB per page, no streaming request bodies | API-driven sites beyond trivial JSON | 250 (js + net) |
| 24 | Timers & scheduling | present | `events.rs:1358-1434`; virtual-time loop `crates/render/src/scripts.rs:1319-1379` (1,000 ms default budget) | no throttling semantics, no `requestIdleCallback` deadline realism | — (adequate for prerender) | — |

## 4. Top 15 by real-site breakage per line of implementation

1. **`navigator` shim** (120 LOC, `crates/js`): one object with userAgent/language/onLine/platform +
   `sendBeacon` no-op unblocks a large share of "unsupported browser" and analytics bootstraps. 
2. **`document.cookie`** (180 LOC, `crates/js` + `crates/net`): bridges the existing cookie jar; without it
   consent/auth flows silently no-op.
3. **`window.scrollX/scrollY` + `Element.scrollTop/scrollHeight/scrollWidth`** (120 LOC, `crates/js`):
   the most-used geometry reads after getBoundingClientRect are simply not there.
4. **`performance.now()` / `timeOrigin`** (80 LOC, `crates/js`): analytics and animation code refuses to run
   without it; the host clock already exists (`host.rs` `now_ms`).
5. **`Element.querySelector`/`querySelectorAll`** (40 LOC, `crates/js`): delegate to the existing document
   selector path with the node as root.
6. **`crypto.getRandomValues` + `randomUUID`** (50 LOC, `crates/js`): the blocking call for ID generation.
7. **`Element.click()`** (30 LOC, `crates/js`): dispatch a synthetic click through the existing
   `dispatch_event` path.
8. **`TextEncoder`/`TextDecoder`** (200 LOC, `crates/js`): needed by fetch bodies and most bundles.
9. **`popstate`/`hashchange`** (120 LOC, `crates/js` + `crates/render`): turns the queued history actions
   into real events so routers stop hard-reloading.
10. **`KeyboardEvent`/`InputEvent`/`FocusEvent` constructors** (200 LOC, `crates/js` + `crates/shell`):
    also the precondition for wiring shell key/input events into JS at all.
11. **`storage` events** (80 LOC, `crates/js`): localStorage writes currently never notify other tabs/
    windows, so cross-tab sync code deadlocks in its own assumptions.
12. **`IntersectionObserver`** (200 LOC, `crates/js` + `crates/render`): lazy-loading libraries fall back to
    scroll polling, which bucket 3 also breaks.
13. **`ResizeObserver`** (200 LOC, `crates/js` + `crates/render`): responsive components and editors
    observe sizes on mount.
14. **`Blob`/`File`/`FormData`** (400 LOC, `crates/js`): uploads and previews; also unlocks ArrayBuffer
    request bodies in `fetch`.
15. **`HTMLFormElement.submit`/`requestSubmit` + validity** (300 LOC, `crates/js`): the shell deliberately
    never submits (`crates/shell/src/forms.rs:20`), so JS-side submission is the only viable path.

Not ranked but strategically largest: **bucket 1** (no live event loop in the shell). Until the shell keeps
a live realm and forwards input, every "partial" row above only helps *prerendered* output; the ranking
above is therefore ordered for the cheapest improvements to prerender fidelity.

## 5. Notes and caveats

- The JS host is deliberately deterministic: virtual clock, 1,000 ms default budget
  (`crates/render/src/scripts.rs:104`), 64 fetch calls/32 MiB per page (`:96-98`), 4,096 timers, 8 workers.
  Any "live browser" work must either keep that budget model or introduce a wall-clock mode.
- `crates/js/src/lib.rs:1-285` is stale: it still documents "No network … exposed", "fetch/XHR not
  implemented", "requestIdleCallback is not implemented" and "Boa 0.21", all contradicted by the code
  (fetch at `runtime.rs:5066`, idle callbacks at `:4901`, layout rects at `:5119+`, Boa 0.22 in
  `Cargo.toml`). The crate docs should be regenerated from this matrix.
- `crates/js/src/module.rs` does not exist; module machinery is `MegiModuleLoader` in `runtime.rs` plus
  `import_map.rs`.
- Worker realms get only `console` + timers (`runtime.rs:594-616`): no `fetch`, no `importScripts`, no
  `MessageChannel`; messages are JSON-serialized (`json_state`), so `structuredClone` semantics are lost.
- `MouseEvent`/`PointerEvent` are JS shims: `preventDefault()` sets a JS property but does not reach the
  host dispatch state, so `dispatchEvent()` still returns `true` for a "prevented" synthetic mouse event
  (`runtime.rs:4903-4916` vs `events.rs:632-700`).
