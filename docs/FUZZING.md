# Fuzzing

Megi's parsers process **untrusted input** (HTML, CSS, URL strings), so they are fuzzed
with [libFuzzer](https://llvm.org/docs/LibFuzzer.html) through
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz). This document covers the offline
local workflow, the targets, the corpus and the crash-triage policy. The security policy
itself (how to report a vulnerability) lives in [`SECURITY.md`](../SECURITY.md).

> **Status**: the harness is wired into CI via
> [`.github/workflows/fuzz-nightly.yml`](../.github/workflows/fuzz-nightly.yml) — a daily
> run of **17 targets**: 10 minutes for the 9 critical parsers/decoders and 5 minutes for
> the rest, plus `workflow_dispatch`. This closes the "fuzzing targets are not wired into
> CI yet" item from `SECURITY.md`.

## Layout

```text
fuzz/
├── Cargo.toml          # standalone workspace (`[workspace]`), NOT part of the root workspace
├── fuzz_targets/
│   ├── html_parse.rs     # WHATWG HTML tokenizer + tree builder + canonical serializer
│   ├── css_parse.rs      # CSS Syntax L3 tokenizer, stylesheets and declaration lists
│   ├── selectors.rs      # Selectors L4 parser/matcher + cascade over a synthetic DOM
│   ├── net_url.rs        # WHATWG URL subset parser + relative resolution
│   ├── render_pipeline.rs# full render pipeline: data: HTML -> styles/layout/paint/PNG
│   ├── js_eval.rs        # JavaScript evaluation + DOM/event/timer host bridge (budgeted)
│   ├── svg_parse.rs      # SVG XML parse + rasterization under strict anti-DoS limits
│   ├── image_decode.rs   # PNG/JPEG/GIF/WebP sniffing + EXIF orientation (capped)
│   ├── exif_parse.rs     # APP1/TIFF EXIF orientation parser
│   ├── hpack_decode.rs   # HPACK header blocks + HTTP/2 framing
│   ├── h2_push.rs        # PUSH_PROMISE/HEADERS/DATA server push (capped)
│   ├── body_stream.rs    # RequestBody/BodyProducer streaming contract
│   ├── webp_vp8.rs       # WebP sniffing + VP8/VP8L container validation path
│   ├── js_module.rs      # ES modules + import maps (resolver/loader, budgeted)
│   ├── webp_lossy.rs     # VP8 lossy keyframe decoder under anti-DoS caps
│   └── container_queries.rs # CSS @container/@layer parsing + query evaluation
└── corpus/<target>/      # versioned seed inputs (small, hand-written)
```

`fuzz/` deliberately has its own empty `[workspace]` table: it must not join the root
workspace (the harness needs `libfuzzer-sys`, nightly and sanitizers, none of which the
engine build uses). The engine crates are plain path dependencies, so the fuzzers always
test the code in this checkout.

Generated directories (`fuzz/target/`, `fuzz/artifacts/`, `fuzz/coverage/`,
`fuzz/Cargo.lock`) are git-ignored; **the seed corpus is tracked**.

## Requirements

```bash
rustup toolchain install nightly --profile minimal   # cargo-fuzz needs nightly + ASan
cargo install cargo-fuzz --locked                    # 0.13+ tested
```

Verify the setup:

```bash
cd fuzz
cargo +nightly fuzz build          # builds every [[bin]] target
```

## Targets

| Target | Public API exercised | Why it matters |
|---|---|---|
| `html_parse` | `html::parse_document_with_options`, `html::parse_fragment_with_options` (18 HTML/SVG/MathML context elements derived from the input), `html::serialize_dom_for_test` | Main attack surface; covers the tokenizer, all insertion modes, foreign content, template contents and the serializer. Default `ParseOptions` limits are used on purpose — fuzz the same configuration real pages get. |
| `css_parse` | `css::parse_stylesheet`, `css::parse_list_of_declarations`, `css::tokenize` | Recursive-descent component-value parsing (audit finding F5: deeply nested blocks used to overflow the stack) and tokenizer escape/URL handling. |
| `selectors` | `css::parse_selectors`, `SelectorList::{matches,matching_specificity}`, `css::resolve_document_styles` | Selector matching is adversarial (F4: exponential backtracking on descendant combinators). A tiny deterministic DOM (≤64 elements) is generated from the input, then the input is parsed as a selector list and as an author stylesheet. |
| `net_url` | `net::Url::parse`, `Url::{as_str, request_target, cache_key, host_for_dns, port_or_default, join}`, `net::percent_decode` | URL strings arrive from untrusted markup (`href`, `src`); must never panic on arbitrary bytes. The target never opens sockets. |
| `render_pipeline` | `render::render_url` with an inline `data:text/html;base64,…` document: HTML parse → charset/MIME → author CSS collection/cascade → layout (real fonts) → display list + hit regions → PNG | The integration glue is the surface a real page reaches. `no_network=true`, scripting disabled (the JS engine has its own target), viewport 96×96 and every budget capped (256 KiB document/CSS, 4 stylesheets/imports, 4 images, 128 KiB total image bytes) so one iteration stays cheap and bounded. |
| `js_eval` | `js::JsRuntime::with_document` + `set_budget` + `eval`, `run_microtasks`, `run_tasks(64)`, `dispatch_event` (`Window`/`Document`/`Node`) | JavaScript source arrives from untrusted pages. Budget is 50k loop iterations / 128 recursion / 4 KiB VM stack, so infinite loops and runaway recursion end in a typed `JsError::Budget` instead of a hang. The fixture document exercises DOM queries, mutations (`textContent`, `innerHTML`, attributes, `style`, `classList`), listeners and timers; no network/FS is reachable. |
| `svg_parse` | `svg::render_svg_with_limits` (XML parse + values + raster) | Inline/external SVG is untrusted. Strict `svg::Limits` (64 KiB input, depth 32, 512 elements, 4k path segments, 128 px / 16K pixels, `<use>` depth 4, 4k selector checks) and a 64×64 output bound the work per iteration. |
<<<<<<< HEAD
||||||| 2a70d69
| `svg_parse` | `svg::render_svg_with_limits` (XML parse + values + raster) | Inline/external SVG is untrusted. Strict `svg::Limits` (64 KiB input, depth 32, 512 elements, 4k path segments, 128 px / 16K pixels, `<use>` depth 4) and a 64×64 output bound the work per iteration. |
=======
| `woff2_decode` | `fonts::decode_web_font` (header/directory validation + third-party WOFF2→sfnt) | `@font-face` bytes are untrusted and reach a converter that trusts the container: a crafted `totalSfntSize`/directory entry used to request gigabytes, and a bad length panicked on a slice range. Input ≤256 KiB; asserts a typed `FontError` and a decoded sfnt ≤64 MiB; seeds: the Open Sans fixture, a truncated prefix and a 55-byte font with a 4,000,000,000-byte directory entry. |
>>>>>>> pr889
| `image_decode` | `image::detect_format`, `image::decode_with_limits`, `image::exif::orientation` | `<img>`/background bytes are untrusted. Input ≤512 KiB and reduced `PngLimits` (1024 px/side, 512 Ki pixels, 128 KiB/chunk, 4 MiB IDAT) bound zlib/JPEG/GIF work per iteration. PNG (all color types/bit depths/Adam7/filters), baseline JPEG, GIF and WebP *sniffing* (WebP is detected-not-decoded → typed `UnsupportedFormat`) plus the APP1 EXIF path. |
| `exif_parse` | `image::exif::orientation` on arbitrary JPEG-ish bytes | Dedicated campaign for the APP1/TIFF orientation parser (little/big endian IFD0, malformed segments must return `None`). Input ≤64 KiB; seeds are the `exif_o1..o8.jpg` fixtures. |
| `hpack_decode` | `net::h2::hpack::Decoder::decode`, `net::h2::frame::FrameReader`/`Frame::{parse_settings, window_increment, parse_rst_stream, parse_goaway, data_payload, headers_payload}`, `frame::validate_frame` | HTTP/2 from the network is adversarial: full HPACK block decoding (static/dynamic tables, Huffman, literals, table-size updates), a raw frame stream (length/flag/stream-id validation, SETTINGS/WINDOW_UPDATE/PING/RST_STREAM/GOAWAY, HEADERS payloads decoded through a fresh HPACK table) and an arbitrary 9-byte frame header. Input ≤64 KiB, 16 KiB frame cap, ≤64 frames/iteration. |

| `h2_push` | `net::h2::frame::{FrameReader, HeaderAssembler, Frame::{parse_push_promise, headers_payload, data_payload}, validate_frame}`, `net::h2::hpack::{Decoder, huffman_decode}` | Round 28 server push: PUSH_PROMISE + promised HEADERS/DATA arrive from an untrusted peer. Caps: ≤128 KiB input, ≤48 frames, 16 KiB frame limit, ≤8 promises, ≤4 decoded header blocks (≤64 KiB each), ≤64 KiB pushed DATA. Covers push-promise stream-id/payload parsing, HEADERS+CONTINUATION reassembly, HPACK tables and frame validation. |
| `body_stream` | `net::{RequestBody, BodyProducer, FetchError}` | Round 28 streaming uploads: pull-based producers with backpressure. Drives a `RequestBody::Stream` with bounded pull sizes (chunk ≤ max, ≤96 pulls, ≤128 KiB total), checks `Bytes` helpers (`len`/`is_empty`/`Debug`) and typed producer-error aborts; never opens a socket. |
| `webp_vp8` | `image::{detect_format, decode_with_limits, PngLimits}` | WebP sniffing + RIFF/VP8/VP8L container validation: the input is decoded directly and also wrapped in minimal RIFF containers with `VP8 `/`VP8L` chunks (plus truncated variants), exercising chunk-size/RIFF arithmetic and the lossless/container paths. The focused VP8 keyframe decoder campaign is `webp_lossy`. Caps: ≤512 KiB input, 8 KiB synthesized chunks, 1024 px/side, 512 Ki pixels. |
| `js_module` | `js::{JsRuntime::{new, set_budget, set_import_map_json, set_module_resolver, set_module_loader, eval_module}, ImportMap::from_json, Budget, JsError}` | Round 27 ES modules + import maps. Input is split on NUL into import-map JSON, entry module and dependency modules; resolver/loader callbacks are bounded (8 modules, ≤8 KiB/module, ≤24 loader calls) and the budget is 2k loop iterations / 64 recursion / 2 KiB stack. Covers JSON import-map validation, specifier resolution, static imports, cycles, blocked mappings, missing exports, dynamic-import/TLA rejections and budget exhaustion; no network/FS and no page DOM fixture. |
| `webp_lossy` | `image::{detect_format, decode_with_limits, PngLimits}` | R228 VP8 lossy keyframe decoder: RIFF/WEBP inputs are decoded directly and via truncated prefixes; non-container inputs are wrapped in a synthesized `VP8 ` chunk (raw and truncated) so the keyframe header/decoder is reached from any seed. Caps: ≤512 KiB input, 2048 px/side, 2 Mi pixels, 128 KiB chunks. |
| `container_queries` | `css::{parse_stylesheet, parse_list_of_declarations, tokenize}`, `css::container::{parse_query, evaluate, ContainerSize}`, `css::ComputedStyle` | R228 CSS container queries + cascade layers: parses `@container`/`@layer` stylesheets and declaration lists, then parses the input (and each line) as a query prelude and evaluates it against fixed container sizes/inline+block combinations. No DOM/layout required; the CSS parser depth/count caps bound each iteration. |

All targets run on the **public** API with release-like defaults; they look for panics,
aborts (stack overflow), ASan/UBSan findings and target timeouts, not for semantic
mismatches.

## Running locally

Short smoke run (30 s per target, the local default in this doc):

```bash
cd fuzz
cargo +nightly fuzz run html_parse      -- -max_total_time=30
cargo +nightly fuzz run css_parse       -- -max_total_time=30
cargo +nightly fuzz run selectors       -- -max_total_time=30
cargo +nightly fuzz run net_url         -- -max_total_time=30
cargo +nightly fuzz run render_pipeline -- -max_total_time=30
cargo +nightly fuzz run js_eval         -- -max_total_time=30 -detect_leaks=0
cargo +nightly fuzz run svg_parse       -- -max_total_time=30
cargo +nightly fuzz run image_decode    -- -max_total_time=30
cargo +nightly fuzz run exif_parse      -- -max_total_time=30
cargo +nightly fuzz run hpack_decode    -- -max_total_time=30
cargo +nightly fuzz run h2_push         -- -max_total_time=30
cargo +nightly fuzz run body_stream     -- -max_total_time=30
cargo +nightly fuzz run webp_vp8        -- -max_total_time=30
cargo +nightly fuzz run js_module       -- -max_total_time=30 -detect_leaks=0
cargo +nightly fuzz run webp_lossy      -- -max_total_time=30
cargo +nightly fuzz run container_queries -- -max_total_time=30
```

`js_eval` needs `-detect_leaks=0`: Boa 0.21 leaks a **constant** ~56 bytes / 2 allocations
once per process (verified with `-runs=1/10/50`: it does not grow per iteration). It is a
third-party one-time allocation, not a host memory-safety bug; LSan would otherwise fail the
nightly job. `render_pipeline` is the most expensive target (full layout + raster + PNG per
iteration): prefer `-runs=100`/`-max_total_time=15` for a quick local smoke run.

`render_pipeline` is the most expensive target (full layout + raster + PNG per iteration):
prefer `-runs=100`/`-max_total_time=15` for a quick local smoke run. `js_eval` and
`js_module` can be slowed by pathological scripts even under the budget; `-timeout=10` is a
good local guard. `js_module` also needs `-detect_leaks=0` for the same constant Boa
one-time allocation as `js_eval`.

The nightly workflow now runs **17 targets** (`fuzz-nightly.yml` matrix): 600 s for the 9
critical parsers/decoders and 300 s for the rest, and the pull-request job builds all 17
without running libFuzzer. New in r27–r29: server push (`h2_push`), streaming request
bodies (`body_stream`), WebP VP8 lossy sniff/lossless path (`webp_vp8`) and ES modules +
import maps (`js_module`); R228 adds the focused VP8 keyframe decoder (`webp_lossy`) and
CSS container queries/layers (`container_queries`). Local R228 campaign:
`cargo +nightly fuzz build` (**16/16** targets) and `-runs=300` on every target (including
the two new ones): no crashes, panics, ASan/UBSan findings or timeouts.

Useful libFuzzer flags:

```bash
# fixed iteration count instead of wall-clock time
cargo +nightly fuzz run html_parse -- -runs=100000
# parallel workers (uses all cores; `-jobs` spawns processes)
cargo +nightly fuzz run html_parse -- -jobs=4 -workers=4 -max_total_time=120
# compact progress stats
cargo +nightly fuzz run html_parse -- -max_total_time=60 -print_final_stats=1
```

`cargo fuzz run <target>` with no explicit corpus argument uses
`fuzz/corpus/<target>/`; new interesting inputs found by libFuzzer are written there, so a
successful local session grows the tracked seed corpus (curate with `cmin`, below).

## Corpus

- `fuzz/corpus/<target>/` contains small hand-written seeds:
  - `html_parse`: normal document, nested/template/foreign-content markup, tables/entities,
    rawtext/script, Unicode;
  - `css_parse`: stylesheets, declarations, at-rules;
  - `selectors`: complex selector lists;
  - `net_url`: absolute/userinfo/IPv6/`file:`/base+relative URLs;
  - `render_pipeline`: basic/styled tables+SVG+form documents and malformed markup;
  - `js_eval`: arithmetic, DOM queries/mutations, listeners+timers, infinite loop, syntax
    error;
  - `svg_parse`: shapes, paths, gradient + `<use>`, nested groups, malformed XML;
  - `image_decode`: small PNG seeds (RGBA/palette/interlaced/filters/gray16), GIF
    (palette/animated), baseline/corrupt/progressive JPEG and a WebP sniffing seed;
  - `exif_parse`: the crate's `exif_o1..o8.jpg` EXIF fixtures plus a plain JPEG;
  - `hpack_decode`: RFC 7541 header blocks (indexed/literal/Huffman/table-size
    update/truncated) and HTTP/2 frame streams (SETTINGS(+ACK/entry), PING,
    HEADERS(+CONTINUATION/padded), DATA, RST_STREAM, WINDOW_UPDATE, GOAWAY and a
    malformed frame header);
  - `h2_push`: the shared frame seeds above plus synthesized PUSH_PROMISE →
    promised HEADERS → DATA flows, an empty promised block, a truncated promise
    and a malformed header;
  - `body_stream`: empty/zero-length/length-prefixed chunk plans, including a
    255-byte chunk;
  - `webp_vp8`: the crate's real WebP fixtures (VP8 lossy q75/q90, VP8L
    photo/alpha/palette) plus a raw payload and a truncated RIFF container, and
    `webp_lossy` (R228) adds real VP8 keyframe fixtures for the focused decoder target;
  - `image_decode` / `exif_parse` (R228): a real **1024×768 JPEG** fixture, its reduced
    EXIF-thumbnail variant and a plain JPEG keep the decoder's size/IFD paths covered;
  - `webp_lossy` (R228): real VP8 lossy fixtures (`photo_lossy.webp`, `webp_lossy.webp`)
    plus their RIFF-header mutations;
  - `svg_parse` (R228): SVG fixtures with filters (`filter_blur.svg`) and SMIL
    animations (`smil.svg`) on top of the existing XML/geometry seeds;
  - `css_parse` / `container_queries` (R228): extracted `@layer`/`@container` stylesheets
    from the render fixtures plus query-prelude seeds for the evaluator;
  - `js_module`: NUL-separated import-map JSON + entry/dependency modules
    (simple export, bare-specifier import map, prefix mappings, malformed JSON,
    cycle, blocked mapping, dynamic import, top-level await, syntax error and an
    infinite-loop budget seed).
- Keep seeds **small** (a few hundred bytes): they are inputs for mutation, not test
  fixtures. The semantic conformance corpus lives in `tests/html5lib-tests` and is not used
  here.
- Minimise a grown corpus before committing it:

  ```bash
  cd fuzz
  cargo +nightly fuzz cmin html_parse
  cargo +nightly fuzz cmin css_parse
  cargo +nightly fuzz cmin selectors
  cargo +nightly fuzz cmin net_url
  cargo +nightly fuzz cmin render_pipeline
  cargo +nightly fuzz cmin js_eval
  cargo +nightly fuzz cmin svg_parse
  cargo +nightly fuzz cmin image_decode
  cargo +nightly fuzz cmin exif_parse
  cargo +nightly fuzz cmin hpack_decode
  cargo +nightly fuzz cmin h2_push
  cargo +nightly fuzz cmin body_stream
  cargo +nightly fuzz cmin webp_vp8
  cargo +nightly fuzz cmin js_module
  cargo +nightly fuzz cmin webp_lossy
  cargo +nightly fuzz cmin container_queries
  ```

## Crashes and triage policy

When libFuzzer finds a crash it writes the reproducer to
`fuzz/artifacts/<target>/crash-<hash>` and exits non-zero.

1. **Reproduce** with the exact artifact (input is the file contents):

   ```bash
   cd fuzz
   cargo +nightly fuzz run html_parse artifacts/html_parse/crash-<hash>
   ```

2. **Minimise** to a small deterministic reproducer:

   ```bash
   cargo +nightly fuzz tmin html_parse artifacts/html_parse/crash-<hash>
   # or: -minimize_crash=1 with a reproduction run
   ```

3. **Classify**:
   - memory-safety (ASan use-after-free, heap overflow, ...) or reachable
     panic/abort/stack overflow in a parser → **security-relevant**, follow
     [`SECURITY.md`](../SECURITY.md) (private report; do not open a public issue);
   - resource exhaustion that is already bounded by `html::ParseOptions` / response caps →
     document as expected behaviour (the fuzzers use default limits on purpose);
   - unreachable invariant/`unreachable!` → normally a parser bug; report it.
4. **Record** the crash in the PR/issue that fixes it, add the minimised input as a
   regression test in the owning crate (`crates/<crate>/tests/`), and prefer adding the
   minimised input to the corpus so future runs cover it.

Fuzz work is **report-only** for crates outside its scope: do not fix `dom`, `html`,
`css`, `layout`, `paint`, `fonts`, `net` from a fuzzing change set — file the minimised
reproducer for the owning work package instead.

Known non-crashes (do not report): the 6 scripted html5lib tree-construction failures and
the 3 `xmlViolation` tokenizer failures are documented conformance gaps, not crashes.

## CI

[`.github/workflows/fuzz-nightly.yml`](../.github/workflows/fuzz-nightly.yml):

- triggers: `schedule` (daily, 03:17 UTC), `workflow_dispatch` and `pull_request` (only
  when `fuzz/**` or the workflow itself changes);
- the scheduled/dispatch matrix builds **and runs** all **16 targets**: the 9 critical
  ones (`html_parse`, `css_parse`, `selectors`, `net_url`, `render_pipeline`, `js_eval`,
  `svg_parse`, `image_decode`, `hpack_decode`) for **600 s** each, and `exif_parse`,
  `h2_push`, `body_stream`, `webp_vp8`, `js_module`, `webp_lossy`,
  `container_queries` for **300 s** (`-rss_limit_mb=2048 -timeout=25`). The job timeout
  is 30 minutes to fit the longer critical campaigns;
- pull requests run a cheap **build-only** job (`cargo +nightly fuzz build`, all targets)
  with a 20-minute job timeout. A stable-only build job is **not viable**: `cargo-fuzz`
  passes `-Zsanitizer=…` to rustc, so the build itself needs nightly;
- on failure uploads `fuzz/artifacts/` and the corpus as a workflow artifact (14 days).

Keep the workflow cheap: the matrix is one job per target with a 30-minute job timeout
(10-minute critical campaigns + build). Longer campaigns should run outside CI (dedicated
fuzzing host or OSS-Fuzz) with a larger corpus and parallelism.

## Limitations

- Daily 10-minute critical / 5-minute secondary runs only scratch the surface; they are a
  regression gate, not a coverage guarantee.
- No coverage-guided corpus merging with OSS-Fuzz and no differential testing against
  other engines.
- `render_pipeline` runs with scripting disabled and `js_eval` runs a fixed budget: the
  fuzzers bound CPU/memory, they do not attempt to reach deep page-specific semantics.
- The render target feeds HTML through an inline `data:` document; external resource
  loading (HTTP/TLS/cache) is covered by the `net_url` target and the engine test suites,
  not by this harness.
- libFuzzer + ASan require Linux/macOS nightly toolchains; the harness is not built on
  stable or Windows CI.
- `image_decode` covers PNG/JPEG/GIF/WebP sniffing plus EXIF; the dedicated
  `webp_vp8` (VP8/VP8L container + lossless) and `webp_lossy` (VP8 keyframe decoder)
  targets give WebP a focused campaign, and `exif_parse` owns the APP1/TIFF parser.
  JPEG/GIF decoding still share `image_decode`'s iterations; split them further if a
  format-specific campaign is needed later.
- `hpack_decode` fuzzes `FrameReader`/`Frame` helpers but not the full `h2::client`
  state machine (multiplexing/flow control live in loopback integration tests).

## Round 18 additions (R105)

New targets, seeds and CI wiring added in round 18:

- `render_pipeline`, `js_eval`, `svg_parse` targets (see the table above) with the local
  smoke commands listed in "Running locally".
- Seed corpora under `fuzz/corpus/{render_pipeline,js_eval,svg_parse}/`.
- `fuzz-nightly.yml` matrix extended to 7 targets and a new PR build-only job (nightly,
  because cargo-fuzz builds require `-Zsanitizer`).
- Local verification (this environment, nightly + cargo-fuzz installed):
  `cargo +nightly fuzz build` builds all 7 targets; bounded smoke runs
  (`-runs=…`, `-timeout=…`) were used per target. No crashes were found while adding the
  harnesses; any future crash should follow the triage policy above (report-only outside
  `fuzz/**`).

### Local smoke results (round 18)

All 7 targets built with `cargo +nightly fuzz build` and ran bounded libFuzzer rounds from
their seed corpora: `html_parse`/`css_parse`/`selectors`/`net_url`/`svg_parse` 400 runs,
`render_pipeline` 40 runs (expensive pipeline), `js_eval` 300 runs. All exit 0 except
`js_eval`, which triggers the constant Boa one-time LSan allocation described above; with
`-detect_leaks=0` it completes 300 runs clean. No panics, ASan memory errors, timeouts or
stack overflows were found.

### Round 19 status

Round 19 (CSS gradients, advanced grid, renderer referrer/state and shell
find-in-page) adds **no new fuzzing targets**: the 7-target matrix, seed corpora
and PR build-only job from round 18 remain current. Gradient/grid parsing is
exercised through the existing `css_parse`/`render_pipeline`/`svg_parse` targets and
their conformance suites.

### Round 20 status

Round 20 (custom-property performance fix, conic gradients/color hints/spaces,
mixed `calc()`, typography/sticky, shell tabs and bench E2E rows) adds **no new
fuzzing targets**: the 7-target matrix, seed corpora and PR build-only job from
round 18 remain current. The new conic/color-space and mixed-calc parsers are
exercised by the existing `css_parse`/`render_pipeline`/`svg_parse` targets and
their conformance suites; `layout/sticky-text-1mb` stays a visible bench SKIP.

### Round 22 status

Round 22 adds no new fuzz *targets of its own* (the matrix stayed at 7 until round 24):
the round-22 JPEG/GIF decoders and HPACK/framing parsers are now covered by the
`image_decode`/`exif_parse`/`hpack_decode` targets added in round 24; `svg_parse` covers
the clipPath/mask/filter XML/attribute surface, while `net_url` still only fuzzes URL
parsing.

### Round 24 additions (R151)

New targets and corpora for the round-22/23 image and HTTP/2 work:

- `image_decode`, `exif_parse` and `hpack_decode` targets (see the table above),
  with `image` added as a fuzz-workspace dependency and the reduced-limit defaults
  documented in each target header.
- Seed corpora under `fuzz/corpus/{image_decode,exif_parse,hpack_decode}/`
  (small image fixtures from `crates/image/tests/fixtures/`, `exif_o1..o8.jpg` and
  hand-written RFC 7541/HTTP-2 frame bytes).
- `fuzz-nightly.yml` matrix extended to **10 targets** (300 s each) and the PR
  build-only job now compiles the three new harnesses too.
- Local smoke results (this environment, nightly + cargo-fuzz):
  `cargo +nightly fuzz build` builds all 10 targets; `image_decode`/`exif_parse`/
  `hpack_decode` were run for 300 iterations from their seed corpus with
  `-rss_limit_mb=2048 -timeout=25` (peak RSS 52/48/51 MB, slowest unit 0 s each).
  No panics, ASan/UBSan findings or timeouts were found; Ok/Err results are
  discarded on purpose.

### Round 24 status

The matrix now has **10 targets**: the original seven plus `image_decode`, `exif_parse` and
`hpack_decode` (curated corpora 11/9/21 seeds). `cargo +nightly fuzz build` builds 10/10 and
300-run smoke campaigns (`-rss_limit_mb=2048 -timeout=25`) finish without panics/ASan/
timeouts. CI runs a 300 s matrix smoke plus the PR build-only job; longer campaigns remain a
future item.

## Round 34 additions (R228)

Campañas más largas, dos targets nuevos y corpus ampliado con los fixtures recientes:

- **CI**: el matrix pasa a **16 targets**; los 9 críticos (`html_parse`, `css_parse`,
  `selectors`, `net_url`, `render_pipeline`, `js_eval`, `svg_parse`, `image_decode`,
  `hpack_decode`) corren **600 s** y el resto (`exif_parse`, `h2_push`, `body_stream`,
  `webp_vp8`, `js_module`, `webp_lossy`, `container_queries`) **300 s**. El timeout del
  job sube a 30 min para absorber la campaña de 10 min por target crítico.
- **`webp_lossy`**: target dedicado al decoder **VP8 keyframe** (`image::decode_with_limits`
  con caps 2048 px/side y 2 Mi pixels); envuelve payloads arbitrarios en un chunk
  `VP8 ` sintético y prueba truncados, de modo que el parser de cabecera y el decoder
  lossy se alcanzan aunque el corpus no sea un contenedor RIFF válido.
- **`container_queries`**: parsea `@container`/`@layer` con `css::parse_stylesheet`,
  declaration lists y tokenizer, y evalúa `css::container::parse_query` +
  `container::evaluate` contra varias combinaciones de tamaño inline/block, sin DOM.
- **Corpus**: seeds reales de un JPEG **1024×768** (`jpeg_1024x768.jpg`), variantes
  EXIF-thumbnail, WebP lossy (`photo_lossy.webp`, `webp_lossy.webp`), SVG con filtros
  (`filter_blur.svg`) y SMIL (`smil.svg`), y CSS con `@layer`/`@container` extraído de
  los fixtures de render (más preludes de query para el evaluador).
- **Coste local** (this environment, nightly + cargo-fuzz 0.13.2, release + ASan):
  `cargo +nightly fuzz build` compila **17/17** targets (~2–4 min en caliente); un smoke
  de 200–400 ejecuciones por target desde el corpus tarda segundos por target y pico de
  RSS ~40–90 MB; las campañas de CI de 600 s/300 s son el coste relevante (~2 h de CPU
  en total si todos corren en paralelo por job).

### Round 34 smoke results

Verificación local de la ronda (misma máquina, `-rss_limit_mb=2048 -timeout=25`,
`-detect_leaks=0` para los targets que embeben Boa):

| Target | Runs | Resultado |
|---|---|---|
| `webp_lossy` | 400 | sin panics/ASan/timeouts, pico RSS ~60 MB |
| `container_queries` | 400 | sin panics/ASan/timeouts, pico RSS ~50 MB |
| `html_parse` | 400 | sin panics/ASan/timeouts (regresión rápida) |
| `image_decode` | 300 | sin panics/ASan/timeouts |
| `svg_parse` | 300 | sin panics/ASan/timeouts |
| `exif_parse` | 300 | sin panics/ASan/timeouts |
| `hpack_decode` | 300 | sin panics/ASan/timeouts |

Los 16 targets compilan con `cargo +nightly fuzz build`; el YAML se validó con un parser
YAML (16 entradas en el matrix, timeout 30). Los resultados Ok/Err se descartan: el signal
son panics, aborts, ASan/UBSan y timeouts.
