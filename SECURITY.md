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
  16 MiB total per page**) and downgrade failures to the background color /
  no marker plus a warning.
- **Image decoding limits** (`crates/image`): the PNG decoder enforces
  `PngLimits` (16384 px per side, 16 Mpx total, 64 MiB per chunk, 256 MiB of
  IDAT) before/while inflating, verifies chunk CRCs and never panics on
  malformed input; the inflate implementation is the crate's own and is
  fuzzed alongside the parsers.
- **JavaScript (Phase 6, ADR-001 accepted)**: the chosen engine is Boa 0.21.x (pure Rust, `Unlicense OR MIT`) with feature `js` **enabled by default**; the dependency is isolated in `crates/js`, which itself keeps `#![forbid(unsafe_code)]`, and every script must run under instruction/time/memory/stack budgets with no network/filesystem capabilities by default. The CI keeps a `--no-default-features` build so the minimal std-only engine can be audited and shipped without the JS dependency.
- **Image pipeline limits** (`crates/render`): at most 64 `<img>` resources per
  document, 8 MiB per image and 32 MiB of image bytes total; downloads honor the
  `net` caps and `--no-network`; undecodable or missing images downgrade to a
  non-fatal warning plus a gray placeholder instead of aborting the render.
- **Desktop shell** (`crates/shell`): `winit` 0.30 and `softbuffer` 0.4 are the
  only windowing dependencies (authorized by ARCHITECTURE §3.1) and their X11 /
  Wayland backends are loaded dynamically (`dlopen`); there is no GPU,
  compositor or external process. `--screenshot` runs fully headless without
  creating an `EventLoop`. Window mode renders off the event loop in a **worker thread** with generation checks, cancellation and coalescing, supports vertical scroll (pages taller than **8192 CSS px** are clipped with a visible marker), mouse/keyboard input and HiDPI scaling; it only consumes local files/URLs. There is still no navigation, DOM hit-testing, zoom or JS (Phase 6/7), so the attack surface is limited to the render pipeline and the worker boundary.
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
  No network, filesystem or wall-clock APIs are exposed. The runtime is **not
  yet wired into `render`**, so `<script>` execution, events/timers and CSSOM
  remain pending (WP-6.2–6.7).

## Out of scope

- Vulnerabilities in third-party dependencies that are fixed upstream and do
  not affect Megi's usage (report those upstream, but let us know).
- Denial of service that requires attacker control over the host machine or
  the Rust toolchain itself.
- Findings from `cargo-fuzz` that are already documented as expected behaviour
  (bounded resource exhaustion under `ParseOptions`/response caps; the 6
  scripted html5lib tree failures and 3 `xmlViolation` tokenizer gaps).
- Issues that only affect rendering fidelity without a security impact.
