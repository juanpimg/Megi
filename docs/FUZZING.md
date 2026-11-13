# Fuzzing

Megi's parsers process **untrusted input** (HTML, CSS, URL strings), so they are fuzzed
with [libFuzzer](https://llvm.org/docs/LibFuzzer.html) through
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz). This document covers the offline
local workflow, the targets, the corpus and the crash-triage policy. The security policy
itself (how to report a vulnerability) lives in [`SECURITY.md`](../SECURITY.md).

> **Status**: the harness is wired into CI via
> [`.github/workflows/fuzz-nightly.yml`](../.github/workflows/fuzz-nightly.yml). The
> scheduled/dispatch run executes **18 targets** under the default libFuzzer
> **AddressSanitizer** build (10 minutes for 9 critical parsers/decoders and 5 minutes for
> the remaining 8 scheduled targets), plus a short **UndefinedBehaviorSanitizer (UBSan)**
> smoke matrix on 9 representative targets. Pull requests run a build-only job that compiles
> every `[[bin]]` fuzz target in `fuzz/Cargo.toml` on nightly (no libFuzzer run).

## Layout

```text
fuzz/
├── Cargo.toml          # standalone workspace (`[workspace]`), NOT part of the root workspace
├── fuzz_targets/       # 26 libFuzzer harnesses
│   ├── body_stream.rs        # RequestBody/BodyProducer streaming contract
│   ├── container_queries.rs  # CSS @container/@layer parsing + query evaluation
│   ├── css_parse.rs          # CSS Syntax L3 tokenizer, stylesheets and declaration lists
│   ├── dom_mutate.rs         # DOM arena mutation & invariant checks
│   ├── dom_tree.rs           # DOM construction/mutation paths from parser-like input
│   ├── exif_parse.rs         # APP1/TIFF EXIF orientation parser
│   ├── font_decode.rs        # Direct SFNT/WOFF2 font decoding & table validation
│   ├── fonts_shape.rs        # Font parsing/shaping inputs
│   ├── h2_push.rs            # PUSH_PROMISE/HEADERS/DATA server push (capped)
│   ├── hpack_decode.rs       # HPACK header blocks + HTTP/2 framing
│   ├── html_parse.rs         # WHATWG HTML tokenizer + tree builder + canonical serializer
│   ├── image_decode.rs       # Full PNG/JPEG/GIF/WebP (VP8/VP8L) decode + EXIF orientation (capped)
│   ├── js_eval.rs            # JavaScript evaluation + DOM/event/timer host bridge (budgeted)
│   ├── js_module.rs          # ES modules + import maps (resolver/loader, budgeted)
│   ├── layout_resolve.rs     # Style/layout calc() & constraint resolution paths
│   ├── layout_tree.rs        # Layout tree construction paths
│   ├── net_surface.rs        # URL, header, cookie, range, and HTTP parser surfaces
│   ├── net_url.rs            # WHATWG URL subset parser + relative resolution
│   ├── paint_display_list.rs # Paint/display-list generation paths
│   ├── render_pipeline.rs    # Full render pipeline: data: HTML -> styles/layout/paint/PNG
│   ├── selectors.rs          # Selectors L4 parser/matcher + cascade over a synthetic DOM
│   ├── svg_parse.rs          # SVG XML parse + rasterization under strict anti-DoS limits
│   ├── text_boundary.rs      # byte -> code-point preprocessor (raw surrogates, >U+10FFFF)
│   ├── webp_lossy.rs         # VP8 lossy keyframe decoder under anti-DoS caps
│   ├── webp_vp8.rs           # WebP sniffing + VP8/VP8L container validation path
│   └── woff2_decode.rs       # WOFF2/@font-face decode/validation path
├── src/lib.rs          # shared helpers (`megi_fuzz`): RIFF/WebP envelope, lossy
│                       #   char-boundary-safe text, length-prefixed part framing
└── corpus/<target>/    # versioned seed inputs (small, hand-written where present)
```

`fuzz/` deliberately has its own empty `[workspace]` table: it must not join the root
workspace (the harness needs `libfuzzer-sys`, nightly and sanitizers, none of which the
engine build uses). The engine crates are plain path dependencies, so the fuzzers always
test the code in this checkout.

Generated directories (`fuzz/target/`, `fuzz/artifacts/`, `fuzz/coverage/`,
`fuzz/Cargo.lock`) are git-ignored; **seed corpora are tracked** when present.

## Requirements

```bash
rustup toolchain install nightly --profile minimal   # cargo-fuzz needs nightly + sanitizers
cargo install cargo-fuzz --version 0.13.2 --locked  # tested pin; keep in sync with CI
```

Verify the setup:

```bash
cd fuzz
cargo +nightly fuzz build          # builds every [[bin]] target
```

## Targets

| Target | Public API / surface exercised | Why it matters |
|---|---|---|
| `html_parse` | `html::parse_document_with_options`, `html::parse_fragment_with_options`, serializer | Main attack surface; tokenizer, insertion modes, foreign content, templates. Default `ParseOptions` limits are used. |
| `text_boundary` | `html::input_stream::preprocess_code_points` (raw `u32` code points) and `preprocess_str`, then the tokenizer over the normalized stream | Bytes-native: the input is read as 4-byte LE code points, so lone surrogates, values above U+10FFFF, CR/CRLF and control characters reach the preprocessor as raw code points instead of being normalised to U+FFFD by `from_utf8_lossy` first (#711). |
| `css_parse` | `css::parse_stylesheet`, `css::parse_list_of_declarations`, `css::tokenize` | Recursive-descent component-value parsing and tokenizer escape/URL handling. |
| `selectors` | `css::parse_selectors`, matching/specificity, `query_selector`/`query_selector_all`, `try_*` variants, `matches_with_state`/`try_matches_with_state`, `css::resolve_document_styles` | Selector matching is adversarial; small deterministic DOM from input. Document-order querying (`document.querySelector`) and the `StateContext` interaction layer are fuzzed too (#700). |
| `net_url` | `net::Url::parse`, URL helpers, percent-decode/join paths | URL strings arrive from untrusted markup; must never panic. Never opens sockets. |
| `render_pipeline` | `render::render_url` with inline `data:` HTML (parse→CSS→layout→paint→PNG) | Integration glue reached by real pages. Viewport (`32..=256`), cap profile (tight vs `RenderOptions::default`), `no_network` and scripting are input-derived; scripting only for documents ≤32 KiB (#842). |
| `js_eval` | `js::JsRuntime` eval, microtasks, tasks, event dispatch under `js::Budget`, with a hostile host-provider bundle installed (fetch/storage/layout/viewport/media/history/location/computed-style/worker) | Untrusted JS plus attacker-shaped provider glue (#704): providers return typed errors, `status: 0`, conflicting `content-length`, CRLF header values, NUL storage keys, `NaN`/`±∞` layout rects and viewports, and a worker loader capped at two calls. Bounded loops/recursion/stack/timers/events turn hangs into typed budget errors. |
| `svg_parse` | `svg::render_svg_with_limits`, `svg::intrinsic_size_with_limits` (XML parse + values + raster) | Untrusted inline/external SVG. Four `SvgOptions` (absent/zero/derived/`u32::MAX`) × two `Limits` profiles (128 px tight, 1024 px roomy) so the caps are exercised on both sides of the boundary (#705). |
| `woff2_decode` | `fonts::decode_web_font` / WOFF2 container handling | `@font-face` bytes are untrusted; bounded decode/validation path. |
| `image_decode` | `detect_format`, `decode_with_limits` (tight + input-derived roomy profile), `decode`, `decode_scaled`, `decode_scaled_with_limits`, `decode_animated`, `decode_animated_with_limits`, `png::decode_png`, `exif::orientation` | Every public decode entry point, not just one capped call: default caps, scaled decode and the animated (ANIM/ANMF) path were unfuzzed before (#853). |
| `exif_parse` | `image::exif::orientation` | Dedicated APP1/TIFF orientation parser campaign. |
| `hpack_decode` | `net::h2::hpack`, `net::h2::frame` parsing/validation helpers | Adversarial HTTP/2 HPACK blocks and frame streams (bounded). |
| `h2_push` | HTTP/2 PUSH_PROMISE/HEADERS/DATA parsing/reassembly helpers | Untrusted server-push/control/data frames under caps. `FrameReader`/`validate_frame` run against every legal max frame size (`64`, `DEFAULT_MAX_FRAME_SIZE`, `MAX_FRAME_SIZE`, `MAX_FRAME_SIZE_LIMIT`), and promised stream id `0` (protocol error) is exercised (#702). |
| `body_stream` | `net::{RequestBody, BodyProducer, FetchError}` streaming contract | Bounded pull-based producer/backpressure/error behavior (no socket). |
| `webp_vp8` | WebP RIFF/VP8/VP8L sniff/container validation paths | WebP container arithmetic and lossless/container handling under caps; the RIFF envelope comes from `megi_fuzz::riff_webp`, shared with `webp_lossy` so the two cannot drift (#859). |
| `js_module` | `js` module/import-map resolve/load/eval APIs with budgets | Untrusted module/import-map inputs; bounded resolver/loader calls and no network/FS. Parts are **length-prefixed** (`[u16 LE len][bytes]…`) and truncated on char boundaries, so JSON strings and module sources keep embedded NULs/high bytes instead of being clipped (#701). |
| `webp_lossy` | VP8 lossy keyframe decode path via image decode APIs | Focused lossy WebP decoder campaign with anti-DoS caps: RIFF inputs decoded as-is and truncated, plus payloads re-wrapped as `VP8 ` **and** `VP8L`, under a 2048 px and a 64 px cap profile (#859). |
| `container_queries` | `css` parse/tokenize + `css::container` query parse/evaluate | `@container`/`@layer` without DOM/layout. Container sizes are input-derived and include the degenerate set (`NaN`, `±∞`, negative, `0`, `f32::MAX`, missing axis), and style queries run against a `ComputedStyle` carrying input-derived custom properties (#710). |
| `dom_mutate` | `dom::Document` arena mutations + `validate_invariants()` | Exercises DOM insert/remove/reparent/text-coalesce invariants on arbitrary mutation sequences. |
| `dom_tree` | DOM construction/mutation-oriented parse paths | Exercises DOM tree building/mutation surfaces from fuzzer input. |
| `font_decode` | `fonts` SFNT/WOFF2 table parsing & decoding | Exercises direct font decoding paths against malformed font tables. |
| `fonts_shape` | Font/shaping input paths | Exercises font parsing/shaping-related surfaces. |
| `layout_resolve` | `layout` `calc()` & constraint resolution paths | Exercises layout length/constraint resolution under adversarial inputs. |
| `layout_tree` | Layout tree construction paths | Exercises layout-tree building from parser/style inputs. |
| `net_surface` | `net` URL/header/cookie/range/HTTP parser surfaces | Exercises network header, cookie, range, and HTTP parsing boundaries. |
| `paint_display_list` | Paint/display-list generation paths | Exercises display-list/paint command generation. |

All targets run on **public** APIs with release-like defaults; they look for panics, aborts
(stack overflow), ASan/UBSan findings and target timeouts, not for semantic mismatches.

## Running locally

Short smoke run (30 s per target):

```bash
cd fuzz
for t in \
  html_parse text_boundary css_parse selectors net_url render_pipeline js_eval svg_parse \
  woff2_decode image_decode exif_parse hpack_decode h2_push body_stream \
  webp_vp8 js_module webp_lossy container_queries dom_tree fonts_shape \
  layout_tree paint_display_list; do
  extra=""
  case "$t" in js_eval|js_module) extra="-detect_leaks=0" ;; esac
  cargo +nightly fuzz run "$t" -- -max_total_time=30 -timeout=25 $extra
done
```

Notes:
- `js_eval` and `js_module` need `-detect_leaks=0` because of a constant third-party Boa
  one-time allocation (not a host leak that grows per iteration); this does not suppress
  ASan/UBSan signals for actual bugs.
- `render_pipeline` is expensive (full layout+raster+PNG per iteration): prefer
  `-runs=100`/`-max_total_time=15` for a quick local smoke run.
- `js_eval`/`js_module` can be slowed by pathological scripts even under budget; `-timeout=10`
  is a good local guard for interactive runs.

Useful libFuzzer flags:

```bash
cargo +nightly fuzz run html_parse -- -runs=100000
cargo +nightly fuzz run html_parse -- -jobs=4 -workers=4 -max_total_time=120
cargo +nightly fuzz run html_parse -- -max_total_time=60 -print_final_stats=1
```

`cargo fuzz run <target>` with no explicit corpus argument uses `fuzz/corpus/<target>/`;
new interesting inputs found by libFuzzer are written there (curate with `cmin` before
committing large growth).

## Corpus

- `fuzz/corpus/<target>/` contains small hand-written seeds for the targets that have
  curated inputs (parser/decoder/protocol/script targets). Keep seeds **small** (a few
  hundred bytes): they are inputs for mutation, not test fixtures. Semantic conformance
  corpora live in `tests/html5lib-tests` and are not used here.
- Minimise a grown corpus before committing it:

  ```bash
  cd fuzz
  for t in \
    html_parse text_boundary css_parse selectors net_url render_pipeline js_eval svg_parse \
    woff2_decode image_decode exif_parse hpack_decode h2_push body_stream \
    webp_vp8 js_module webp_lossy container_queries dom_tree fonts_shape \
    layout_tree paint_display_list; do
    cargo +nightly fuzz cmin "$t" || true
  done
  ```

## Crashes and triage policy

When libFuzzer finds a crash it writes the reproducer to
`fuzz/artifacts/<target>/crash-<hash>` and exits non-zero.

1. **Reproduce** with the exact artifact:

   ```bash
   cd fuzz
   cargo +nightly fuzz run html_parse artifacts/html_parse/crash-<hash>
   ```

2. **Minimise** to a small deterministic reproducer:

   ```bash
   cargo +nightly fuzz tmin html_parse artifacts/html_parse/crash-<hash>
   ```

3. **Classify**:
   - memory-safety (ASan use-after-free, heap overflow, ...) or reachable panic/abort/stack
     overflow in a parser → **security-relevant**, follow [`SECURITY.md`](../SECURITY.md)
     (private report; do not open a public issue);
   - resource exhaustion already bounded by parser/runtime limits → document as expected
     behaviour (fuzzers use default/reduced limits on purpose);
   - unreachable invariant / `unreachable!` → parser/runtime bug; report it.
4. **Record** the crash in the fixing PR/issue, add the minimised input as a regression test
   in the owning crate (`crates/<crate>/tests/`), and prefer adding it to the corpus so
   future runs cover it.

### Sanitizers

- **ASan (AddressSanitizer):** enabled by default in the nightly scheduled matrix via
  `cargo +nightly fuzz run` on supported hosts.
- **UBSan (UndefinedBehaviorSanitizer):** a dedicated `fuzz-ubsan` smoke matrix runs
  `--sanitizer undefined` (120 s) on representative parser/decoder targets
  (`html_parse, css_parse, selectors, net_url, image_decode, hpack_decode, svg_parse,
  webp_lossy, container_queries`), excluding Boa-embedding JS targets with known
  third-party leak/compat constraints under sanitizers.
- **UTF-8-valid-only text targets:** `html_parse`, `css_parse`, `selectors`, `container_queries`,
  `net_url`, `js_eval` and `js_module` decode with `String::from_utf8_lossy` by design — their
  parsers take `&str`. The raw byte → code-point layer (lone surrogates encoded as `u32`,
  values above U+10FFFF, CR/CRLF, control characters) is covered instead by the bytes-native
  `text_boundary` target (#711), so a fix in `html::input_stream` cannot hide behind lossy
  decoding.
- **LSan/leaks:** `js_eval` and `js_module` pass `-detect_leaks=0` for the constant Boa
  one-time allocation noted above; this does not suppress ASan/UBSan bug signals.
- **Triage:** sanitizer failures upload `fuzz/artifacts/` and corpus snapshots as workflow
  artifacts. Reproduce locally with `cargo +nightly fuzz run <target> <artifact>` (add
  `--sanitizer undefined` for UBSan), minimise with `cargo +nightly fuzz tmin`, and follow
  the policy above.

Fuzz work is **report-only** for crates outside its scope: do not fix `dom`, `html`, `css`,
`layout`, `paint`, `fonts`, `net` from a fuzzing-only change set — file the minimised
reproducer for the owning work package instead.

Known non-crashes (do not report): the 3 `xmlViolation` tokenizer failures are documented conformance gaps, not crashes (all 3,904 tree-construction tests pass).

## CI

[`.github/workflows/fuzz-nightly.yml`](../.github/workflows/fuzz-nightly.yml):

- triggers: `schedule` (daily, 03:17 UTC), `workflow_dispatch`, and `pull_request` when
  `fuzz/**`, `.github/workflows/fuzz-nightly.yml`, engine crates, or root manifests change;
- **ASan matrix** (`fuzz`, 18 targets): 600 s for `html_parse, css_parse, selectors, net_url,
  render_pipeline, js_eval, svg_parse, image_decode, hpack_decode`; 300 s for
  `woff2_decode, exif_parse, h2_push, body_stream, webp_vp8, js_module, webp_lossy,
  container_queries` (`-rss_limit_mb=2048 -timeout=25`, `-detect_leaks=0` for Boa targets);
- **UBSan smoke** (`fuzz-ubsan`, 9 targets): 120 s with `--sanitizer undefined` on
  representative targets (see Sanitizers above);
- **PR job** (`build`): `cargo +nightly fuzz build` compiles all `[[bin]]` targets in
  `fuzz/` (fast harness-compile gate; no libFuzzer run);
- on failure uploads `fuzz/artifacts/` and corpus snapshots as workflow artifacts (14 days).

Notes:
- 26 harnesses exist in `fuzz/fuzz_targets/`; `dom_tree`, `dom_mutate`, `font_decode`,
  `fonts_shape`, `layout_resolve`, `layout_tree`, `net_surface`, and `paint_display_list`
  are built in PRs and can be promoted to scheduled ASan/UBSan matrices when their
  corpus/runtime budgets are validated.
- Daily 10-minute critical / 5-minute secondary runs are a regression gate, not a coverage
  guarantee. Longer campaigns should run outside CI.

## Soak campaigns

The nightly matrix gives each harness a handful of minutes, which finds shallow
bugs; the deep ones need hours in the same target with a grown corpus. Two ways to
get that:

* **CI**: `.github/workflows/fuzz-soak.yml` runs every Saturday at 02:11 UTC and on
  demand (`hours` and `targets` inputs), rotating through all harnesses for the
  requested wall-clock budget. Hosted runners cap a job at six hours, so the
  schedule asks for 5 h; `hours: 24` needs a self-hosted runner
  (`runs-on: [self-hosted, soak]`). The corpus is cached between runs
  (`fuzz-corpus-soak-*`), so each week starts where the previous one stopped, and
  logs plus crash artifacts are uploaded even when the campaign fails.
* **Locally**: `tools/soak_fuzz.sh --hours 24` runs the same rotation on this
  machine. Useful flags: `--slice-seconds 600` (time per target per rotation, default
  900), `--targets "html_parse css_parse"`, `--out target/soak`. It keeps
  `fuzz/corpus/<target>` between rotations, mirrors any libFuzzer artifact into
  `target/soak/crashes/<target>/`, and exits non-zero if a target run failed.
  `js_eval`/`js_module` run with `-detect_leaks=0` for the Boa reason above.

Memory/runtime signals during a soak: libFuzzer already aborts on
`-rss_limit_mb=4096` and `-timeout=25`, so a runaway allocation or an infinite loop
shows up as a normal artifact to triage. Leak detection stays on for the non-Boa
targets; `valgrind`/`heaptrack` on a single reproducer is the next step when an
artifact looks like a leak rather than a crash:

```bash
cargo +nightly fuzz run html_parse fuzz/artifacts/html_parse/crash-<hash> -- -runs=1
valgrind --leak-check=full target/x86_64-unknown-linux-gnu/release/html_parse fuzz/artifacts/html_parse/crash-<hash>
```

## Limitations

- Daily runs only scratch the surface; no coverage-guided corpus merging with OSS-Fuzz and
  no differential testing against other engines.
- `render_pipeline` enables scripting only for documents ≤32 KiB and `js_eval`/`js_module` use fixed/bounded
  budgets and limited event/reuse variation.
- The render target feeds HTML through inline `data:` documents; external HTTP/TLS/cache
  loading is not fuzzed by this harness.
- libFuzzer + ASan/UBSan require Linux/macOS nightly toolchains; UBSan CI coverage is a
  representative smoke pass, not a full-time campaign on every target.
- `hpack_decode` fuzzes frame/HPACK helpers but not the full `h2::client` state machine.
