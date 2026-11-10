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
│   ├── html_parse.rs   # WHATWG HTML tokenizer + tree builder + canonical serializer
│   ├── css_parse.rs    # CSS Syntax L3 tokenizer, stylesheets and declaration lists
│   ├── selectors.rs    # Selectors L4 parser/matcher + cascade over a synthetic DOM
│   └── net_url.rs      # WHATWG URL subset parser + relative resolution
└── corpus/<target>/    # versioned seed inputs (small, hand-written)
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

All targets run on the **public** API with release-like defaults; they look for panics,
aborts (stack overflow), ASan/UBSan findings and target timeouts, not for semantic
mismatches.

## Running locally

Short smoke run (30 s per target, the local default in this doc):

```bash
cd fuzz
cargo +nightly fuzz run html_parse -- -max_total_time=30
cargo +nightly fuzz run css_parse  -- -max_total_time=30
cargo +nightly fuzz run selectors  -- -max_total_time=30
cargo +nightly fuzz run net_url    -- -max_total_time=30
```

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

- `fuzz/corpus/<target>/` contains small hand-written seeds: a normal HTML document,
  nested/template/foreign-content markup, tables/entities, rawtext/script, Unicode, CSS
  stylesheets/declarations/at-rules, complex selector lists and representative URLs
  (absolute, userinfo, IPv6, `file:`, base+relative pairs).
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

- triggers: `schedule` (daily, 03:17 UTC) and `workflow_dispatch`;
- installs nightly + `cargo-fuzz`, builds each target, runs it for 300 s
  (`-rss_limit_mb=2048 -timeout=25`);
- on failure uploads `fuzz/artifacts/` and the corpus as a workflow artifact (14 days).

Keep the workflow cheap: the matrix is one job per target with a 20-minute job timeout.
Longer campaigns should run outside CI (dedicated fuzzing host or OSS-Fuzz) with a larger
corpus and parallelism.

## Limitations

- Daily 5-minute runs only scratch the surface; they are a regression gate, not a
  coverage guarantee.
- No coverage-guided corpus merging with OSS-Fuzz, no differential testing against other
  engines, and no JS engine fuzzing yet (Phase 6).
- libFuzzer + ASan require Linux/macOS nightly toolchains; the harness is not built on
  stable or Windows CI.
