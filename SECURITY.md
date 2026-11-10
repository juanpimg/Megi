# Security Policy

Megi is a from-scratch web engine: it parses and renders untrusted input by
design, so memory safety and denial-of-service resistance are treated as
correctness requirements, not features.

## Supported versions

The project is pre-1.0 and only the `main` branch is supported. Fixes are
landed directly on `main`; there are no maintained release branches yet.

## Reporting a vulnerability

Please report suspected vulnerabilities **privately**:

1. Open a private security advisory on GitHub
   (`Security` → `Advisories` → `New draft security advisory`) for
   `juanpimg/Megi`, or
2. Email the maintainer listed in the repository profile.

Do not open a public issue for a security report. Include:

- affected commit / version and platform,
- minimal input that triggers the issue (HTML/CSS/patch),
- observed behavior (crash, abort, hang, excessive CPU/memory) and expected
  behavior,
- whether the input is attacker-controlled in a realistic setting.

We aim to acknowledge reports within 7 days and to coordinate disclosure
after a fix is available.

## Security model and guarantees

- **Memory safety**: **every** engine crate (`dom`, `html`, `css`, `fonts`,
  `layout`, `paint`, `net`, `image`, `render`, `shell`, `js`, `test_harness`,
  `bench`) declares `#![forbid(unsafe_code)]` and is built with
  `clippy -D warnings`. The
  dependency policy in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) §3
  restricts external crates to low-level, audited building blocks
  (`serde`/`serde_json` in the test harness, the typography stack in `fonts`,
  and `rustls` + `webpki-roots` for TLS transport in `net`).
- **Network security**: `crates/net` validates `https:` servers against the
  Mozilla trust store compiled in through `webpki-roots` (no system store, no
  network at startup), checks the URL hostname/chain with `rustls` 0.23
  (ring backend) and supports a private CA injected via
  `FetchOptions::tls_config`. HTTP responses are capped (16 MiB body / 64 KiB
  headers), redirects are limited, and the HTTP cache follows an RFC 9111
  subset (`Cache-Control`/`Expires`/`ETag`/`304`/`Vary`). No `unsafe` is used
  for any of it.
- **Cookies, CORS and compression**: `crates/net` implements an RFC 6265 subset
  (`CookieJar` capped at 4,096 cookies: expired first, then oldest session;
  `FetchOptions.cookie_jar`) and a documented CORS subset (`Origin`, ACAO/
  credentials validation, preflight validation; no preflight cache). `gzip`/
  `deflate` responses are bounded by `CompressionLimits`: **64 MiB decompressed
  and a 1000:1 maximum ratio**, with CRC32/ISIZE verification, so compression
  bombs are rejected before unbounded allocation. `Accept-Encoding: gzip` is
  negotiated by default; `br`, stacked codings and HTTP/2 are not supported.
- **Hostile input**: parsing is bounded by `html::ParseOptions`
  (nesting depth, node count, attribute count and input size). Limits are
  reported through `LimitViolations` instead of aborting or panicking.
  Recursive post-processing and serialization are iterative so deeply nested
  documents cannot overflow the stack.
- **No unsound abstractions**: tree mutations are checked in release builds
  (`append_child` / `insert_before` reject self-insertion and ancestor
  cycles), and detached references never panic.

## Fuzzing and continuous robustness

- The parsers that consume untrusted input are fuzzed with libFuzzer through
  `cargo-fuzz`: **four targets** (`html_parse`, `css_parse`, `selectors`,
  `net_url`) with a versioned seed corpus. The harness lives in the standalone
  `fuzz/` workspace (nightly + ASan) and is **not** a dependency of the engine
  crates.
- **CI dev environment**: the quality/conformance/MSRV jobs install only the Wayland/X11/xcb **development** packages (`libwayland-dev`, `libxkbcommon*`, `libx11*`, `libxcb*`) that `winit`/`softbuffer` need to compile in the runner; they are build/test dependencies, never shipped or linked into the engine at runtime.
- **Wired into CI**: [`.github/workflows/fuzz-nightly.yml`](.github/workflows/fuzz-nightly.yml)
  runs a 5-minute smoke campaign per target daily (03:17 UTC) and on demand,
  uploading crash artifacts on failure.
- **Local campaign on record: ~1.06 M ASan executions, 0 crashes.** Triage
  policy, reproduction, minimisation and regression rules are documented in
  [`docs/FUZZING.md`](docs/FUZZING.md); memory-safety crashes and reachable
  panics/aborts are security-relevant and must be reported privately (see
  above).
- Remaining non-linear CPU paths and layout/paint recursion budgets are
  tracked in [`docs/HANDOFF.md`](docs/HANDOFF.md) and measured by the
  benchmark suite; see [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md).
- **Render pipeline limits** (`crates/render`): main document capped at 16 MiB,
  at most 32 external stylesheets, 1 MiB per stylesheet and 4 MiB of total
  author CSS, viewport up to 8192×8192; `--no-network` refuses http(s)
  resources. Non-fatal failures (e.g. a 404 stylesheet) become warnings in
  `RenderOutput::stats` instead of aborting the render. CSS `background-image`/
  `list-style-image` reuse the image caps (**32 background images, 8 MiB each,
  16 MiB total per page**); background geometry (round 15: repeated tiles,
  cover/contain) is capped at **4096 tiles per layer** and clipped to the padding
  box, and downgrade failures to the background color / placeholder. **Multi-layer backgrounds (round 16)** reuse the same per-layer image caps and the paint tile cap (**4096** per layer); `background_layers` is bounded by the CSS declaration size and those caps, with the legacy single-layer mirror used only for goldens.
  no marker plus a warning. Document scripts are capped at **64 scripts, 8 MiB
  per script and 32 MiB total**, downloads honor `--no-network`, and each script
  runs under a per-script and cumulative `ScriptingLimits` budget; `--no-scripting`
  disables execution entirely (and `--no-default-features` builds without Boa).
  With `--cookies`/`RenderOptions.cookies` a cookie jar lives for one render (or
  is shared as an explicit session jar by the shell), propagating `Set-Cookie`
  across document/redirects/subresources; `--origin` enables CORS/credentials
  validation, and a CORS denial is a **non-fatal warning** in `RenderOutput::stats`
  (never a crash). The **event loop is budgeted**: at most 10,000 timer/microtask
  tasks and 1,000 ms of virtual time per render, then a single re-render; exceeding
  a budget is reported in `RenderStats::timings.event_loop` and never hangs the
  process.
- **Web font decoding limits** (`crates/fonts`): WOFF2 containers are decoded by the authorized dependency **`woff2-patched` 0.4** (Apache-2.0, `#![forbid(unsafe_code)]`-style pure Rust API, no C/FFI, used only by `crates/fonts`). The decoded sfnt must pass the font database's validation; `crates/render` caps web fonts at **8 fonts / 8 MiB each / 32 MiB total per page**, honors `--no-network`, deduplicates sources and downgrades fetch/decode failures to a **non-fatal warning + fallback** instead of aborting the render. No network or filesystem capabilities are exposed to the font parser.
- **SVG rendering limits** (`crates/svg`, wired into `render` for `<img>` and `background-image`): the from-scratch XML parser and rasterizer enforce explicit anti-DoS limits — **8 MiB input, depth 256, 100k elements, 1,024 attributes/element, 1M flattened path segments, 16k px per side, 64 Mpx and `<use>` depth 16** — reject malformed/truncated XML and never panic on arbitrary input; no network/filesystem access and no `unsafe`. `crates/svg` also bounds its internal CSS/gradient layers (**1 MiB of stylesheet, 4,096 rules, 64 selectors/rule, 16 descendant compounds, 256 stops per gradient, 32 href hops**) and renders cyclic/broken gradient references as no paint without panicking. `crates/render` additionally caps each SVG at **16 Mpx / 64 MiB RGBA**, downgrades corrupt or oversized images to a **non-fatal warning with no pixels**, never exposes the SVG parser to network/filesystem access, and wraps direct SVG navigation in a synthetic HTML page. **Inline SVG (round 15)**: `<svg>` elements inside the document are replaced before styles/layout by a synthetic `<img>` with a `data:` URL (base64, no network); limits are **256 SVGs per document, 1 MiB per SVG, 8 MiB total, nesting depth 256**, exceeding any of them degrades to a non-fatal warning (the element is left untouched), and only the already-sandboxed `crates/svg` parser/rasterizer is used. **Round 17 SVG**: `<pattern>` tiling is capped at **10,000 tiles** (cyclic pattern references paint nothing, no panic) and basic `<text>` is capped at **1,000 text elements / 10,000 characters**, with glyph shaping delegated to the already-isolated `crates/fonts` bundled fonts (no network/filesystem access). **Event loop (round 15)**: rendering uses a **virtual clock only** (`run_until` in 1 ms steps, no wall-clock `run_tasks`), so timer-driven pages execute deterministically and cannot be perturbed by host load.
- **Image decoding limits** (`crates/image`): the PNG decoder enforces
  `PngLimits` (16384 px per side, 16 Mpx total, 64 MiB per chunk, 256 MiB of
  IDAT) before/while inflating, verifies chunk CRCs and never panics on
  malformed input; the inflate implementation is the crate's own and is
  fuzzed alongside the parsers.
- **JavaScript (Phase 6, ADR-001 accepted; WP-6.1/6.2/6.4 + WP-A/WP-B + advanced events done)**: the chosen engine is Boa 0.21.x (pure Rust, `Unlicense OR MIT`) with feature `js` **enabled by default**; the dependency is isolated in `crates/js`, which itself keeps `#![forbid(unsafe_code)]`, and every script must run under instruction/time/stack budgets with no network/filesystem capabilities. **Advanced events (round 12)**: `currentTarget`/`eventPhase` are real, `once`/`passive` are honoured, `removeEventListener` compares identity, inline `onclick`/`onload`/`onerror` handlers run in the same sandbox, and listener exceptions are caught into `last_event_error()` without aborting the remaining dispatch — a page cannot use a throwing listener to corrupt the host or panic the process. **Browser APIs (round 16)**: `getComputedStyle` is served from an immutable pre-script snapshot through a Rust provider (scripts cannot mutate host state out of band), `matchMedia`/viewport providers return plain values, and uncaught errors plus `console.warn/error` are downgraded to non-fatal warnings counted in `RenderStats::console_errors` (no abort, no host leak); `requestAnimationFrame` runs on the deterministic virtual clock (fixed epoch, 16 ms steps). `crates/render` executes the document's classic `<script>` elements **after parsing** under per-script and total `ScriptingLimits` (caps above), drains the **budgeted event loop** (see above) and re-renders the mutated DOM; ES modules are warned and skipped, and `async`/`defer` semantics are pending. **Parser-inserted scripts (WP-6.4)**: the parser exposes a `ScriptExecutor` hook; hosts that install one execute inline scripts at the `</script>` under the same budgets and re-tokenize `document.write` output in place. The conformance harness (`crates/test_harness`, **development-only, not shipped**) installs a Boa-backed executor with the same sandbox to run the `scripted_*` corpus; the parser itself stays engine-free and the no-scripting path is zero-cost. The CI keeps a `--no-default-features` build so the minimal std-only engine can be audited and shipped without the JS dependency (follow-up: feature-gate the harness's Boa dependency).
- **Image pipeline limits** (`crates/render`): at most 64 `<img>` resources per
  document, 8 MiB per image and 32 MiB of image bytes total; downloads honor the
  `net` caps and `--no-network`; undecodable or missing images downgrade to a
  non-fatal warning plus a gray placeholder instead of aborting the render.
- **Desktop shell** (`crates/shell`): `winit` 0.30 and `softbuffer` 0.4 are the
  only windowing dependencies (authorized by ARCHITECTURE §3.1) and their X11 /
  Wayland backends are loaded dynamically (`dlopen`); there is no GPU,
  compositor or external process. `--screenshot` runs fully headless without
  creating an `EventLoop`. Window mode renders off the event loop in a **worker thread** with generation checks, cancellation and coalescing, supports vertical scroll (pages taller than **8192 CSS px** are clipped with a visible marker), mouse/keyboard input, HiDPI scaling and **link navigation with bounded history** (`NavigationState`: cap 256 entries, forward truncation; only `http`/`https`/`file`/`data` URLs resolved through `crates/net`, HTTP redirects honored). Link hit regions come from `RenderOutput::hit_regions` (the same paint pass as the pixels) and are resolved with `paint::find_link_at`; `--cookies` installs an explicit bounded session jar and `--origin` the CORS origin. Navigation regions come from the **same render** as the pixels, and paint's hit-testing API is purely geometric (no new input parser). There is still no JS in the host loop, forms or tabs, so the attack surface remains the render pipeline plus the already-bounded network surface; **event-loop execution is deterministic (virtual clock only, round 15)**, so CI/host timing cannot change which timers run. **Zoom/auto-refresh (round 13)**: zoom only changes the CSS viewport (`physical / (scale * zoom)`); opt-in `--refresh-ms` re-renders **only** when `scripts_executed > 0 && script_timers_pending > 0`, through the generation-checked worker (cancellation/coalescing), preserving scroll and never looping on a static page. **Editable forms (round 17)**: the shell never evaluates host JS during editing (scripts stay disabled until reload), form controls are tracked by a stable `data-megi-form-id`, edits patch a bounded DOM snapshot and re-render off-thread; the snapshot is encoded as a `data:` URL served by the existing loader (no network), `Enter` never submits and there are no native widget processes/handles. **Decorations (round 17)**: `box-shadow`/`text-shadow` are bounded to **32 shadows**, blur radius to **64 px**, and the blur is a deterministic three-pass box filter (no randomness or threads); rounded rects/borders/groups are rasterized in-process with the existing caps.
- **DOM identity**: generational `NodeId` handles (20-bit slot + 12-bit
  generation, free-list reuse) reject stale/foreign handles and give clear
  panics instead of corrupting the arena; `descendants`/`ancestors` are
  iterative and terminate on cyclic links.
- **JavaScript budgets (Phase 6, WP-6.1)**: every `eval` is bounded by
  `js::Budget` mapped onto Boa `RuntimeLimits` (default **5,000,000 loop
  iterations, recursion depth 512 and 20,480 VM stack units**; `Budget::SMALL`
  for pages/tests) and violations surface as `JsError::Budget` instead of
  hanging or overflowing the native stack. The DOM bridge is a **side table of
  numeric handles capped at 1,000,000** entries: the generation-checked
  `NodeId` never leaves Rust and stale/foreign nodes raise a typed `TypeError`.
  No network, filesystem or wall-clock APIs are exposed to scripts. `render`
  wires the runtime into the pipeline with per-script + total budgets and the
  script caps above. **WP-A** adds `EventTarget`/`Event`/`CustomEvent` (capture
  window→document→ancestors→target and bubbling) and a timer scheduler whose
  clock is **only injectable from the Rust host** (`advance`/`run_until`); the
  listener/timer state is rooted in the bounded per-realm host side table and
  dispatch/timer queues are driven by the host, so scripts cannot reach network,
  filesystem, real wall clock or another realm. **WP-B** adds a full
  `CSSStyleDeclaration` and a `getComputedStyle` provider (a Rust callback: JS gets
  computed values, not extra capabilities) plus `NodeList` snapshots, `matches`/
  `closest`, `insertAdjacentHTML`/`Text` and wrapper-identity caching. **WP-6.4**
  executes parser-inserted scripts through the host-installed `ScriptExecutor` (the
  harness uses the same budgets). **Advanced events (round 12)** add real `currentTarget`/`eventPhase`, `once`/`passive`, identity-based `removeEventListener`, inline handlers and captured listener exceptions in the same sandbox. Pending: `document.onerror`, ES modules and real memory accounting.

## Out of scope

- Vulnerabilities in third-party dependencies that are fixed upstream and do
  not affect Megi's usage (report those upstream, but let us know).
- Denial of service that requires attacker control over the host machine or
  the Rust toolchain itself.
- Findings from `cargo-fuzz` that are already documented as expected behaviour
  (bounded resource exhaustion under `ParseOptions`/response caps; the 6
  scripted html5lib tree failures and 3 `xmlViolation` tokenizer gaps).
- Issues that only affect rendering fidelity without a security impact.
