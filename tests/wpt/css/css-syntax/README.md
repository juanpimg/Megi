# Vendored WPT CSS Syntax fixtures

These three files are unmodified copies of the upstream [web-platform-tests](https://github.com/web-platform-tests/wpt)
CSS Syntax tests. They are **vendored as conformance fixtures** so that a clean clone of Megi can run
the CSS Syntax closed-loop gate offline (`cargo test -p test_harness`) with no network access.

- Upstream URL base: <https://raw.githubusercontent.com/web-platform-tests/wpt/master/css/css-syntax/>
- Upstream branch: `master`
- Fetched on: **2026-11-09** (UTC)
- Retrieved with: `curl -fsSL <url>` (plain `master` snapshot, no pin; see hashes below)
- SHA-256 of the vendored copies:

| File | Bytes | SHA-256 |
|---|---|---|
| `declarations-trim-whitespace.html` | 1975 | `af6e5414e18c8ce42c3d0f8614837493e7495b5a7f8f84f8bddb9d2654d4c62d` |
| `input-preprocessing.html` | 1311 | `8792806053c83b90517e2c4b64fec98ba8b6146e15f19e1fa1078694aace6aa0` |
| `anb-parsing.html` | 3336 | `e11b74d37e03deda5595ac3a26ecfcf10c4efeb08acfed9371ca56fffe5695db` |

Only this directory is tracked under `tests/wpt/`; the rest of any local WPT sparse checkout stays
ignored (see the `tests/wpt/*` block in the repository `.gitignore`).

## What the harness executes (`crates/test_harness`)

The fixture files are read from disk at test time; if any of them is missing the gate **fails**
(it does not silently skip). The CSS section contributes exactly **30 results**:

| Fixture | Results | How it is exercised |
|---|---|---|
| `declarations-trim-whitespace.html` | 9 | The `<style id="style-el">` block is parsed with `html::parse_document`, the stylesheet is parsed with the `css` crate, and `--foo-1`..`--foo-9` must trim to `bar`. |
| `input-preprocessing.html` | 13 | The 10 upstream `testParsing(...)` cases are extracted from the file's script and replayed through `css::preprocess_css_codepoints` (5 NULL and 5 lone-surrogate cases), plus 3 newline-normalization cases (CRLF/CR/FF) that upstream cannot express in JS. |
| `anb-parsing.html` | 8 | 8 curated `testANB(...)` inputs are located in the file, their upstream expected `selectorText` strings are parsed, and the engine's `AnPlusB { a, b }` is canonicalized to `n`/`2n+1`-style text and compared against the upstream expectation. |
| **Total** | **30** | |

This is deliberately a 30-check gate, not a claim of full WPT coverage: `anb-parsing.html` contains
67 upstream assertions (33 of which expect parse errors) and the engine currently gates 8 curated
valid cases. Expanding An+B coverage (error cases and `selectorText` roundtrip) is tracked as
follow-up work, as is comparing full `:nth-child()` serialization.

Commands (from the repository root):

```bash
cargo test -p test_harness                 # closed-loop tests, including the CSS gate
cargo run -p test_harness -- --summary     # CLI report; exit 0 only if failures are allowlisted
cargo run -p test_harness -- --summary --strict   # any failure (even allowlisted) => exit 1
```

## Refreshing the fixtures

```bash
base=https://raw.githubusercontent.com/web-platform-tests/wpt/master/css/css-syntax
for f in declarations-trim-whitespace.html input-preprocessing.html anb-parsing.html; do
  curl -fsSL "$base/$f" -o "tests/wpt/css/css-syntax/$f"
done
sha256sum tests/wpt/css/css-syntax/*.html
```

After refreshing, update the date/hashes above and re-run `cargo test -p test_harness`; the
extractors in `crates/test_harness/src/css_harness.rs` fail loudly if the expected number of
upstream cases changes (10 `testParsing` cases, 67 `testANB` cases, the 8 curated An+B inputs).
