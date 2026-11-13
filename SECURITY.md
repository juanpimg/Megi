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
  `cargo-fuzz`: **seven targets** (`html_parse`, `css_parse`, `selectors`,
  `net_url`, `render_pipeline`, `js_eval`, `svg_parse`) with a versioned seed
  corpus; the round-18 additions cover the full render pipeline (data: URL and
  caps), the JavaScript host (`Budget` + DOM APIs + event dispatch) and the SVG
  parser (strict `Limits`). The harness lives in the standalone
  `fuzz/` workspace (nightly + ASan) and is **not** a dependency of the engine
  crates.
- **CI dev environment**: the quality/conformance/MSRV jobs install only the Wayland/X11/xcb **development** packages (`libwayland-dev`, `libxkbcommon*`, `libx11*`, `libxcb*`) that `winit`/`softbuffer` need to compile in the runner; they are build/test dependencies, never shipped or linked into the engine at runtime.
- **Wired into CI**: [`.github/workflows/fuzz-nightly.yml`](.github/workflows/fuzz-nightly.yml)
  runs a 5-minute smoke campaign per target daily (03:17 UTC) over the 7-target
  matrix and on demand, uploading crash artifacts on failure; pull requests touching
  `fuzz/**` run a cheap build-only job (`cargo +nightly fuzz build`) that never
  blocks the quality gate. Local round-18 verification built all 7 targets and ran
  bounded smoke campaigns (400 runs per parser target, 40 for the expensive
  pipeline, 300 for JS) with no panics/ASan/UBSan findings; `js_eval` runs with
  `-detect_leaks=0` because Boa 0.21 leaks a constant ~56-byte one-time allocation.
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
- **Round 18 limits**: **paint clipping** restricts `overflow:hidden` + `border-radius` to an offscreen 4×4-AA mask capped at **16.7 Mpx**, and `background-clip:text` composites through a glyph mask (multi-line text, **4,096 runs** cap); without those properties the display list stays byte-identical (goldens unchanged). **Grid MVP** parses at most **1,024 tracks** per template and only adds layout-time budgets (no new input surface). **CSS custom properties/`var()`** are bounded to **32 substitution levels** with cycle detection (fallback/inherit, never panic) and `calc()` rejects mixed `%`+`px` expressions. **Net (round 18)**: SameSite/PSL/Referrer-Policy/Range parsing only touches already-untrusted header/URL bytes, rejects malformed ranges with a typed `InvalidRange`, enforces the reduced PSL with a safe fallback, defaults cookies to `SameSite=Lax` when absent, and never exposes network/filesystem capabilities to parsers. **Fuzzing**: the three new targets and their corpora are CI-wired; crashes remain report-only outside `fuzz/**` until triaged and minimised.
- **Round 19 limits**: **CSS gradients** are capped at **64 stops per gradient and 256 gradients per computed style**; the parser rejects unknown function/angle/unit forms and paint tiles each gradient deterministically (no network/filesystem access, no new dependencies). **Advanced grid** keeps the **1,024-track** cap (templates, `minmax()`, `repeat(auto-fill|auto-fit)`), rejects unsupported structures (named lines/zones/subgrid/explicit overlaps) without panicking, and resolves `order` deterministically (stable document order). **Render referrer/SameSite**: `ReferrerConfig` is installed in the loader before subresources (header > `meta` > default `strict-origin-when-cross-origin`), so the emitted `Referer` and automatic SameSite rejects follow the visible document policy; resolution is a **per-render snapshot** (no hover/active, approximated top-level model) and no network capability is exposed to the CSS/HTML parsers. **Find-in-page** (`crates/shell/src/find.rs`) searches **plain text only** (case-insensitive literal, no regex/URLs/images), walks Text nodes with a match cap, and its highlight overlays are painted in the framebuffer/headless path without invoking scripts or network. **Fuzzing**: round 18’s 7-target matrix and corpora remain unchanged in round 19.
- **Round 20 limits**: **custom properties** are lazy/COW (`Option<Arc<HashMap>>`); no parser/runtime capability is added, and the fast path only scans declaration bytes for `--`/`var(`. **conic-gradient**/`repeating-conic`, color hints and interpolation spaces reuse the gradient parser limits (**64 stops per gradient, 256 gradients per computed style**), reject unknown spaces/units and write deterministically; only `srgb`/`srgb-linear`/`oklab` are accepted (no `display-p3` or arbitrary color functions). **Mixed `calc()`** uses a bounded side table (`CalcLengths`) with the existing nested-expression depth limit; when the containing block is indefinite the percentage component is documented as zero, never an unbounded allocation. **Typed text/sticky**: `text-transform` is applied during shaping without mutating the DOM, spacing values are lengths and sticky is a headless flow-slot model (no JS/scroll side channel exposed to parsers). **Shell tabs** cap at **MAX_TABS = 32**, use the existing URL parser/loader path (cookies/origin policy unchanged per tab), and closing the last tab opens a blank New Tab instead of hanging; cli `--tabs` only accepts URLs validated by the same loader. **Bench E2E rows** generate in-process fixtures and the `layout/sticky-text` row stays SKIP; they add no runtime/network surface. **Fuzzing**: the 7-target matrix/corpora remain unchanged in round 20.
- **Round 22 limits**: **JPEG/GIF decoders** (`crates/image`) validate markers/LZW against explicit caps before allocating (dimensions, frame bytes, palette/code sizes); malformed, truncated, progressive/CMYK JPEGs and animated/unknown GIFs return typed errors, never panic. **HTTP/2** (`crates/net`) caps frames at **16 KiB**, header blocks at **64 KiB**, at most **2 concurrent streams** in the MVP and validates HPACK static/dynamic tables and Huffman decoding before use; `RST_STREAM`/`GOAWAY` and body caps bound remote work and no ALPN/h2c downgrade bypasses TLS verification. **SVG effects** keep the documented caps (**4,096 shapes per clipPath/mask, 16 filter primitives, blur radius 64 px, 64M sample budget**) and drop cyclic clip/use/mask references without panicking; **CSS filters/mix-blend** use an offscreen `EffectGroup` capped at **16.7 Mpx**, blur radius **64 px** and **16 functions**, so compositing cannot allocate unbounded intermediate buffers. **Shell chrome** is drawn locally from existing render output (address bar/tab strip/`<select>` dropdown, cap 8 rows) and adds no network or filesystem capability; tab state remains bounded. **Fuzzing**: no new fuzz targets in round 22; the new JPEG/GIF/HPACK parsers are covered by unit/robustness tests but a dedicated fuzz campaign is still pending (report-only).
- **Round 23 limits**: **progressive JPEG** parsing honours the same marker/HSOF2/HSOS caps as baseline (bounded scans, spectral-selection validation, restart markers, truncation ⇒ typed error; multi-component refinement returns an explicit `UnsupportedFeature` instead of guessing); **EXIF** parsing is limited to the APP1/TIFF header/IFD0 tag 0x0112 with offset/length validation and no thumbnail/full-IFD walking, so hostile EXIF cannot allocate or recurse; **WebP** is only sniffed (RIFF/VP8L) and never decoded, degrading to a non-fatal warning. **HTTP/2** flow control validates connection/stream windows, rejects invalid zero/negative WINDOW_UPDATE deltas with `FLOW_CONTROL_ERROR`, multiplexes at most `Limits.max_concurrent_streams` (default 8, respecting peer MAX_CONCURRENT_STREAMS), validates SETTINGS values before ACK (HEADER_TABLE_SIZE/MAX_CONCURRENT_STREAMS/INITIAL_WINDOW_SIZE/MAX_FRAME_SIZE/ENABLE_PUSH), keeps the 16 KiB frame / 64 KiB header caps and RST/GOAWAY isolation. **Paint** effects fast path only skips work when no filter/blend/image-set exists (same output); **CSS** cache key is 128-bit with immutable `StyleCollection`, inline-styled nodes excluded. **Fuzzing**: no new targets; progressive/EXIF/HPACK-flow parsers should get dedicated campaigns.
- **Round 24 limits**: **JS browser APIs** add no network/FS capability: History/location providers are host callbacks (no-op without provider), Web Storage is a per-realm map capped at **1 MiB** with typed `QuotaExceededError`, and `MutationObserver` keeps a bounded queue (4,096 records) with recursive subtree matching capped by the DOM depth; records are delivered only when the host drains microtasks. **CSS/paint**: `backdrop-filter` offscreen buffers are capped at **16.7 Mpx**, blur radius **64 px** and **16 functions**; object-fit uses the existing bilinear sampler with a clip to the content box (same caps as other images). **Image**: progressive JPEG validates every scan/EOBRUN/restart against the same caps as baseline and returns typed errors on truncation; WebP only decodes the **VP8L literal subset** (bounded RIFF/header/Huffman tables; transforms/meta-Huffman/LZ77/VP8 lossy ⇒ `UnsupportedFeature`) and never trusts declared dimensions. **Shell**: session persistence caps **32 tabs/64 history entries**, writes atomically with a best-effort lock, honours `--no-session`/`--session-file`, and headless runs do not write; downloads sanitize filenames and only write PNG/HTML under the requested path. **Fuzzing**: matrix grows to **10 targets** (`image_decode`, `exif_parse`, `hpack_decode` added) with curated corpora (11/9/21) and 300 s CI smoke + PR build-only.
- **Round 25 limits**: **CSS Color 4/5** only accepts the documented spaces/functions and clamps each channel after the CSS Color 4 matrices (no HDR/arbitrary color profiles); `color-mix()` bounds both weights/alpha and hue-arc interpolation. **ES modules** are a line-based subset: resolver/loader are host callbacks with no network/FS in `js`, cycles are rejected (`ModuleCycle`), missing modules/exports are typed errors, and there is no dynamic `import()`, top-level await or `export … from`; module text is supplied by the host and evaluated in the same realm/budget. **WebP VP8L** validates RIFF/VP8L chunk sizes, Huffman tables, palette deltas and LZ77 distance maps before allocating; VP8 lossy/corrupt input degrades to a non-fatal warning. **Render providers** (`history_actions`/`location_actions`, `RenderOptions.storage`, `host_events`) are pure data channels: render never navigates or performs I/O on its own, localStorage stays capped by the js 1 MiB/realm limit, and `host_events` are host-supplied only. **Shell save-png** writes the PNG bytes from the render with signature validation and atomic replacement (no re-encode, path sanitized). **Fuzzing**: matrix unchanged at 10 targets.
- **Round 26 limits**: **Module scripts** resolve against the importing module URL with an allow-list of file/http(s)/data schemes, bare specifiers fail closed (`ModuleNotFound`), module and total-graph caps bound loader work, and network fetches honour `FetchOptions`/`--no-network`; a module failure is non-fatal and never executes the loader on untrusted paths. **Scaled decode** computes the factor with checked arithmetic, caps `max_image_dim`, and PNG unfiltering/Adam7 subsampling never allocates the full-resolution buffer; JPEG/GIF/WebP post-subsample stays within the existing decode caps. **HTTP/2 bodies** cap the request body at 16 MiB (`RequestBodyTooLarge`), respect MAX_FRAME_SIZE/flow-control windows, send END_STREAM exactly once, handle RST during upload, and parse trailers/100-continue without unbounded buffering; HTTP/1.1 POST supports CL/chunked within the same caps. **DOM/JS bug-bash** fixes fragment insertion/cloning, case-insensitive HTML attributes and reflected properties while keeping NodeList snapshots; remaining gaps are documented. **SVG** filter chains are capped at 16 primitives and bounded area/blur, SMIL caps animation elements (1,024) and ignores malformed values; `animateTransform` composes after the element transform. **Bench** rows are isolated and `--save-png-isolated` reuses the screenshot bytes (no extra I/O). **Fuzzing**: matrix unchanged at 10 targets.
- **Round 27 limits**: **PNG decode** keeps the scaled-decode caps and the `#[inline(always)]` fix is internal (no new unsafe); **HTTP/2 PUSH_PROMISE** is disabled by default (`SETTINGS_ENABLE_PUSH=0`), server push is validated, rejected with a typed error and cancelled with RST_STREAM(CANCEL) while keeping HPACK state in sync; priorities validate `depends_on`/weight 1..=256 and the scheduler is bounded; h2c is GET-only with 101/preface validation and falls back to HTTP/1.1. **DOM/JS gaps**: URL/URLSearchParams, structuredClone (depth-capped, no cycles), atob/btoa, Mouse/PointerEvent and idle callbacks stay in the JS realm with no I/O; `getBoundingClientRect`/`offsetWidth` remain unimplemented rather than faked; dataset writes are not proxied. **Effects perf** only consolidates existing computations (single-pass `compute_effects`) and does not change byte-safe output. **Shell chrome** uses an internal clipboard and deterministic caret/overflow state; `--chrome` output is deterministic. **Fuzzing**: matrix unchanged at 10 targets.
- **Round 28 limits**: **Reduced IDCT** only drops AC terms for `decode_scaled` (full decode and progressive fallback keep the existing validation/caps); `exif_thumbnail` bounds APP1/IFD1 sizes and only resizes within caps. **HTTP/2 push** is opt-in (`Limits.enable_server_push`), validates PUSH_PROMISE HPACK→URL, caps resources/body/total, cancels with RST_STREAM(CANCEL) and ignores late frames; **streaming uploads** are pull-based with backpressure, single-shot, cap 16 MiB and typed `RequestBody` errors; h1 chunked/100-continue keeps the same caps. **JS geometry** is read-only host data (no layout mutation) and returns 0 without a provider; render installs a pre-script snapshot only. **Blur/PNG byte-safe** changes internal buffers only, keeps identical bytes/goldens and respects existing area/blur caps. **Shell context menu** uses the internal clipboard, sanitizes download names and only writes after magic-byte validation; `Save image` re-fetches only `<img>` under existing network caps. **Fuzzing**: matrix unchanged at 10 targets.
- **Round 29 limits**: **streaming responses** callbacks are bounded and a callback error cancels the stream with RST_STREAM(CANCEL); h1 streaming falls back to buffered reads in 16 KiB chunks, the keep-alive pool is capped per origin (4) with idle timeout and only plain HTTP, and h2 reuses sessions without cross-load pooling. **SVG textPath/SMIL** flatten paths under the existing segment caps, ignore invalid keyTimes/keySplines/values (byte-identical fallback) and interpolate colours with channel clamping; no auto-rotate/side right/textLength. **Import maps** only accept the first map, longest-prefix exact rules and `null` blocking (`ImportMapBlocked`); dynamic `import()` is literal/eager only and top-level await fails closed with `TopLevelAwait` (no fake support); render warns non-fatally on duplicate/invalid maps. **WebP lossy** parses only RIFF/VP8 headers with caps and returns `UnsupportedFeature` (experimental) instead of mis-decoding; render shows a warning/placeholder. **Shell pages/favicons** sanitize directory/error HTML and absolute hrefs, cap favicon fetches at 64 KiB and decode at 16×16 nearest with deterministic FNV fallback. **Fuzzing** grows to 14 targets (300 s each, `js_module` with `-detect_leaks=0`) with 37 curated seeds. **Bench**: all budgets met without raising them.
- **Round 30 limits**: **VP8 lossy** decodes only keyframes with validated frame headers/segment/partition caps, typed errors for VP8X/ALPH/ANIM/interframes/corrupt data and no unbounded allocation; **container queries** only evaluate the documented size features against the nearest compatible ancestor and keep the no-rules fast path; **PSL** is a vendored MPL-2.0 snapshot (16.610 lines) parsed once behind `OnceLock`, IDN left ASCII/punycode-documented, and `tools/update_psl.sh` is the only update path; **request trailers** require a body, cap headers and send END_STREAM exactly once (framing without trailers is byte-identical); **Web Workers** are cooperative (no threads), JSON-safe FIFO messages, capped at 8 workers/256 messages, no SharedWorker/importScripts and render only loads `file://`; **shell history/chrome** caps entries (64 per tab, 10 recent), dedupes and draws the chrome band over the pixmap with identical pixels (no encoder change). **Fuzzing**: matrix unchanged at 14 targets.
- **Round 31 limits**: **Cascade layers** only affect the documented cascade order (no `layer()` imports; approximated sublayers) and the no-layers fast path is byte-identical; **PNG single-encode** keeps `ensure_png()` idempotent/byte-identical and only skips the redundant encode when the caller mutates the pixmap; overlays still require one final encode. **fetch()** is GET/POST only, caps 64 headers/8 MiB body, same-origin CORS, typed rejection without provider/`--no-network`, file misses synthetic 404 and no streaming/abort/timeout. **Proxies** are opt-in (`ProxyConfig`/env), support Basic auth only (Display hides credentials), CONNECT is validated before TLS, typed ProxyAuthRequired/ProxyConnect/ProxyTunnel errors, no PAC/WPAD/SOCKS and no proxy pooling. **VP8 perf** keeps keyframe-only/caps/tolerances (q75 max 6, q90 max 2, 1024 max 7). **Fuzzing**: matrix unchanged at 14 targets.
- **Round 32 limits**: **Bloom pruning** only rejects candidates for descendant/child relations (valid matches preserved); **PNG decode** stays pure-safe (no SIMD/unsafe) with identical raw md5, and **encode** changes are byte-safe (goldens unchanged); **render fast paths** only trigger when no style references images / no scripts or `<svg` in markup. **TLA/live bindings** use Boa native modules: await continuations are driven by the deterministic host microtask/timer loop, rejected graphs surface typed errors, and import maps are not consulted on the async path (no fake support). **h1 streaming** parses status/headers incrementally, follows redirects before the body, caps/timeouts remain, callback errors abort and close; gzip/deflate stay buffered in 16 KiB slices. **Push prioritization** parses PADDED/PRIORITY, defaults to weight 16 (FIFO), evicts the lowest weight with RST_STREAM and keeps resource/body caps. **Fuzzing**: matrix unchanged at 14 targets.
- **Round 33–36 & post-audit limits**: **Paint** (`crates/paint`): `Pixmap::new` / `new_with_background` return an empty pixmap when `width * height` overflows or exceeds `MAX_PIXMAP_PIXELS` instead of panicking (issue `#667`); per-pass `TextCache::masks` is capped at `MAX_TEXT_CACHE_ENTRIES = 4096` entries (issue `#656`). **Fonts** (`crates/fonts`): `SYSTEM_FALLBACK_DB` is initialized lazily via `OnceLock` only when a glyph is missing from bundled/web fonts (bundled golden output unchanged). **Shell** (`crates/shell`): `layout::scrollable_overflow_height` bounds vertical scroll height while skipping `display: none` / `position: fixed` subtrees and respecting inherited `clip` rects. **Fuzzing**: matrix expanded to **16 libFuzzer targets**.










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

## CVE process and disclosure timeline

### Severity classification (CVSS 3.1)

| Severity | Score | Response time | Fix timeline |
|---|---|---|---|
| Critical | 9.0-10.0 | 24 hours | 7 days |
| High | 7.0-8.9 | 48 hours | 14 days |
| Medium | 4.0-6.9 | 7 days | 30 days |
| Low | 0.1-3.9 | 14 days | 90 days |

### Disclosure timeline

1. **Day 0**: Vulnerability reported privately
2. **Day 1-7**: Acknowledgment and triage
3. **Day 7-30**: Fix developed and tested
4. **Day 30-90**: Coordinated disclosure
5. **Day 90**: Public disclosure (if not fixed, disclose anyway)

### Security contact

- **GitHub Security Advisories**: https://github.com/juanpimg/Megi/security/advisories/new
- **Email**: (see repository profile)

### Security advisories workflow

1. Reporter opens private security advisory on GitHub
2. Maintainer triages and assigns severity
3. Fix developed in private branch
4. Fix merged to main with security advisory published
5. CVE requested if applicable
6. Public disclosure after fix is available

### Bug bounty

No formal bug bounty program exists yet. Security researchers are credited
in the release notes and SECURITY.md.
