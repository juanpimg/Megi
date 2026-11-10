# Fuzzing

Megi's parsers process **untrusted input** (HTML, CSS, URL strings), so they are fuzzed
with [libFuzzer](https://llvm.org/docs/LibFuzzer.html) through
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz). This document covers the offline
local workflow, the targets, the corpus and the crash-triage policy. The security policy
itself (how to report a vulnerability) lives in [`SECURITY.md`](../SECURITY.md).

> **Status**: the harness is wired into CI via
> [`.github/workflows/fuzz-nightly.yml`](../.github/workflows/fuzz-nightly.yml) — a daily
> 5-minute-per-target smoke run plus `workflow_dispatch`. This closes the "fuzzing targets
> are not wired into CI yet" item from `SECURITY.md`.

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
│   └── svg_parse.rs      # SVG XML parse + rasterization under strict anti-DoS limits
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
| `svg_parse` | `svg::render_svg_with_limits` (XML parse + values + raster) | Inline/external SVG is untrusted. Strict `svg::Limits` (64 KiB input, depth 32, 512 elements, 4k path segments, 128 px / 16K pixels, `<use>` depth 4) and a 64×64 output bound the work per iteration. |

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
```

`js_eval` needs `-detect_leaks=0`: Boa 0.21 leaks a **constant** ~56 bytes / 2 allocations
once per process (verified with `-runs=1/10/50`: it does not grow per iteration). It is a
third-party one-time allocation, not a host memory-safety bug; LSan would otherwise fail the
nightly job. `render_pipeline` is the most expensive target (full layout + raster + PNG per
iteration): prefer `-runs=100`/`-max_total_time=15` for a quick local smoke run.

`render_pipeline` is the most expensive target (full layout + raster + PNG per iteration):
prefer `-runs=100`/`-max_total_time=15` for a quick local smoke run. `js_eval` can be
slowed by pathological scripts even under the budget; `-timeout=10` is a good local guard.

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
  - `svg_parse`: shapes, paths, gradient + `<use>`, nested groups, malformed XML.
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
- the scheduled/dispatch matrix builds **and runs** every target for 300 s
  (`-rss_limit_mb=2048 -timeout=25`): `html_parse`, `css_parse`, `selectors`, `net_url`,
  `render_pipeline`, `js_eval`, `svg_parse`;
- pull requests run a cheap **build-only** job (`cargo +nightly fuzz build`, all targets)
  with a 20-minute job timeout. A stable-only build job is **not viable**: `cargo-fuzz`
  passes `-Zsanitizer=…` to rustc, so the build itself needs nightly;
- on failure uploads `fuzz/artifacts/` and the corpus as a workflow artifact (14 days).

Keep the workflow cheap: the matrix is one job per target with a 20-minute job timeout.
Longer campaigns should run outside CI (dedicated fuzzing host or OSS-Fuzz) with a larger
corpus and parallelism.

## Limitations

- Daily 5-minute runs only scratch the surface; they are a regression gate, not a
  coverage guarantee.
- No coverage-guided corpus merging with OSS-Fuzz and no differential testing against
  other engines.
- `render_pipeline` runs with scripting disabled and `js_eval` runs a fixed budget: the
  fuzzers bound CPU/memory, they do not attempt to reach deep page-specific semantics.
- The render target feeds HTML through an inline `data:` document; external resource
  loading (HTTP/TLS/cache) is covered by the `net_url` target and the engine test suites,
  not by this harness.
- libFuzzer + ASan require Linux/macOS nightly toolchains; the harness is not built on
  stable or Windows CI.

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
