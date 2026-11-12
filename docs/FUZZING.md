# Fuzzing

Megi's parsers process **untrusted input** (HTML, CSS, URL strings), so they are fuzzed
with [libFuzzer](https://llvm.org/docs/LibFuzzer.html) through
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz). This document covers the offline
local workflow, the targets, the corpus and the crash-triage policy. The security policy
itself (how to report a vulnerability) lives in [`SECURITY.md`](../SECURITY.md).

> **Status**: the harness is wired into CI via
> [`.github/workflows/fuzz-nightly.yml`](../.github/workflows/fuzz-nightly.yml). The
> scheduled/dispatch run executes **17 targets** under the default libFuzzer
> **AddressSanitizer** build (10 minutes for 9 critical parsers/decoders and 5 minutes for
> the remaining 8 scheduled targets), plus a short **UndefinedBehaviorSanitizer (UBSan)**
> smoke matrix on 9 representative targets. Pull requests run a build-only job that compiles
> every `[[bin]]` fuzz target in `fuzz/Cargo.toml` on nightly (no libFuzzer run).

## Layout

```text
fuzz/
├── Cargo.toml          # standalone workspace (`[workspace]`), NOT part of the root workspace
├── fuzz_targets/       # 21 libFuzzer harnesses
│   ├── body_stream.rs        # RequestBody/BodyProducer streaming contract
│   ├── container_queries.rs  # CSS @container/@layer parsing + query evaluation
│   ├── css_parse.rs          # CSS Syntax L3 tokenizer, stylesheets and declaration lists
│   ├── dom_tree.rs           # DOM construction/mutation paths from parser-like input
│   ├── exif_parse.rs         # APP1/TIFF EXIF orientation parser
│   ├── fonts_shape.rs        # Font parsing/shaping inputs
│   ├── h2_push.rs            # PUSH_PROMISE/HEADERS/DATA server push (capped)
│   ├── hpack_decode.rs       # HPACK header blocks + HTTP/2 framing
│   ├── html_parse.rs         # WHATWG HTML tokenizer + tree builder + canonical serializer
│   ├── image_decode.rs       # PNG/JPEG/GIF/WebP sniffing + EXIF orientation (capped)
│   ├── js_eval.rs            # JavaScript evaluation + DOM/event/timer host bridge (budgeted)
│   ├── js_module.rs          # ES modules + import maps (resolver/loader, budgeted)
│   ├── layout_tree.rs        # Layout tree construction paths
│   ├── net_url.rs            # WHATWG URL subset parser + relative resolution
│   ├── paint_display_list.rs # Paint/display-list generation paths
│   ├── render_pipeline.rs    # Full render pipeline: data: HTML -> styles/layout/paint/PNG
│   ├── selectors.rs          # Selectors L4 parser/matcher + cascade over a synthetic DOM
│   ├── svg_parse.rs          # SVG XML parse + rasterization under strict anti-DoS limits
│   ├── webp_lossy.rs         # VP8 lossy keyframe decoder under anti-DoS caps
│   ├── webp_vp8.rs           # WebP sniffing + VP8/VP8L container validation path
│   └── woff2_decode.rs       # WOFF2/@font-face decode/validation path
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
| `css_parse` | `css::parse_stylesheet`, `css::parse_list_of_declarations`, `css::tokenize` | Recursive-descent component-value parsing and tokenizer escape/URL handling. |
| `selectors` | `css::parse_selectors`, matching/specificity, `css::resolve_document_styles` | Selector matching is adversarial; uses a small deterministic DOM generated from input. |
| `net_url` | `net::Url::parse`, URL helpers, percent-decode/join paths | URL strings arrive from untrusted markup; must never panic. Never opens sockets. |
| `render_pipeline` | `render::render_url` with inline `data:` HTML (parse→CSS→layout→paint→PNG) | Integration glue reached by real pages. Network/scripting disabled; tight budgets/caps. |
| `js_eval` | `js::JsRuntime` eval, microtasks, tasks, event dispatch under `js::Budget` | Untrusted JS; bounded loops/recursion/stack/timers/events so hangs become typed budget errors. |
| `svg_parse` | `svg::render_svg_with_limits` (XML parse + values + raster) | Untrusted inline/external SVG under strict anti-DoS limits. |
| `woff2_decode` | `fonts::decode_web_font` / WOFF2 container handling | `@font-face` bytes are untrusted; bounded decode/validation path. |
| `image_decode` | `image::detect_format`, `image::decode_with_limits`, EXIF orientation | Untrusted `<img>`/background bytes across PNG/JPEG/GIF/WebP sniffing with caps. |
| `exif_parse` | `image::exif::orientation` | Dedicated APP1/TIFF orientation parser campaign. |
| `hpack_decode` | `net::h2::hpack`, `net::h2::frame` parsing/validation helpers | Adversarial HTTP/2 HPACK blocks and frame streams (bounded). |
| `h2_push` | HTTP/2 PUSH_PROMISE/HEADERS/DATA parsing/reassembly helpers | Untrusted server-push/control/data frames under caps. |
| `body_stream` | `net::{RequestBody, BodyProducer, FetchError}` streaming contract | Bounded pull-based producer/backpressure/error behavior (no socket). |
| `webp_vp8` | WebP RIFF/VP8/VP8L sniff/container validation paths | WebP container arithmetic and lossless/container handling under caps. |
| `js_module` | `js` module/import-map resolve/load/eval APIs with budgets | Untrusted module/import-map inputs; bounded resolver/loader calls and no network/FS. |
| `webp_lossy` | VP8 lossy keyframe decode path via image decode APIs | Focused lossy WebP decoder campaign with anti-DoS caps. |
| `container_queries` | `css` parse/tokenize + `css::container` query parse/evaluate | `@container`/`@layer` and query evaluation without DOM/layout. |
| `dom_tree` | DOM construction/mutation-oriented parse paths | Exercises DOM tree building/mutation surfaces from fuzzer input. |
| `fonts_shape` | Font/shaping input paths | Exercises font parsing/shaping-related surfaces. |
| `layout_tree` | Layout tree construction paths | Exercises layout-tree building from parser/style inputs. |
| `paint_display_list` | Paint/display-list generation paths | Exercises display-list/paint command generation. |

All targets run on **public** APIs with release-like defaults; they look for panics, aborts
(stack overflow), ASan/UBSan findings and target timeouts, not for semantic mismatches.

## Running locally

Short smoke run (30 s per target):

```bash
cd fuzz
for t in \
  html_parse css_parse selectors net_url render_pipeline js_eval svg_parse \
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
    html_parse css_parse selectors net_url render_pipeline js_eval svg_parse \
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
- **LSan/leaks:** `js_eval` and `js_module` pass `-detect_leaks=0` for the constant Boa
  one-time allocation noted above; this does not suppress ASan/UBSan bug signals.
- **Triage:** sanitizer failures upload `fuzz/artifacts/` and corpus snapshots as workflow
  artifacts. Reproduce locally with `cargo +nightly fuzz run <target> <artifact>` (add
  `--sanitizer undefined` for UBSan), minimise with `cargo +nightly fuzz tmin`, and follow
  the policy above.

Fuzz work is **report-only** for crates outside its scope: do not fix `dom`, `html`, `css`,
`layout`, `paint`, `fonts`, `net` from a fuzzing-only change set — file the minimised
reproducer for the owning work package instead.

Known non-crashes (do not report): the 6 scripted html5lib tree-construction failures and
the 3 `xmlViolation` tokenizer failures are documented conformance gaps, not crashes.

## CI

[`.github/workflows/fuzz-nightly.yml`](../.github/workflows/fuzz-nightly.yml):

- triggers: `schedule` (daily, 03:17 UTC), `workflow_dispatch`, and `pull_request` when
  `fuzz/**`, `.github/workflows/fuzz-nightly.yml`, engine crates, or root manifests change;
- **ASan matrix** (`fuzz`, 17 targets): 600 s for `html_parse, css_parse, selectors, net_url,
  render_pipeline, js_eval, svg_parse, image_decode, hpack_decode`; 300 s for
  `woff2_decode, exif_parse, h2_push, body_stream, webp_vp8, js_module, webp_lossy,
  container_queries` (`-rss_limit_mb=2048 -timeout=25`, `-detect_leaks=0` for Boa targets);
- **UBSan smoke** (`fuzz-ubsan`, 9 targets): 120 s with `--sanitizer undefined` on
  representative targets (see Sanitizers above);
- **PR job** (`build`): `cargo +nightly fuzz build` compiles all `[[bin]]` targets in
  `fuzz/` (fast harness-compile gate; no libFuzzer run);
- on failure uploads `fuzz/artifacts/` and corpus snapshots as workflow artifacts (14 days).

Notes:
- 21 harnesses exist in `fuzz/fuzz_targets/`; 4 (`dom_tree`, `fonts_shape`, `layout_tree`,
  `paint_display_list`) are built in PRs but not yet in the scheduled ASan/UBSan matrices.
  Add them to nightly matrices when their corpus/runtime budgets are validated.
- Daily 10-minute critical / 5-minute secondary runs are a regression gate, not a coverage
  guarantee. Longer campaigns should run outside CI.

## Limitations

- Daily runs only scratch the surface; no coverage-guided corpus merging with OSS-Fuzz and
  no differential testing against other engines.
- `render_pipeline` runs with scripting disabled and `js_eval`/`js_module` use fixed/bounded
  budgets and limited event/reuse variation.
- The render target feeds HTML through inline `data:` documents; external HTTP/TLS/cache
  loading is not fuzzed by this harness.
- libFuzzer + ASan/UBSan require Linux/macOS nightly toolchains; UBSan CI coverage is a
  representative smoke pass, not a full-time campaign on every target.
- `hpack_decode` fuzzes frame/HPACK helpers but not the full `h2::client` state machine.
