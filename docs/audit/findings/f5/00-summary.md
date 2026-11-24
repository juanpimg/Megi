# Hallazgos F5 — resumen consolidado (2026-11-24)

Detalle completo en `git log -- docs/audit/findings/f5/*.md`.

## html-css
# HTML + CSS gap matrix — Megi @ 9c94e86

Read-only analysis of upstream `main` (9c94e86) (upstream main, commit 9c94e86). No cargo
was run; every claim is from source reading plus targeted `rg` probes (commands quoted
inline where the evidence is an *absence*). Status legend: **full** = spec-conformant
enough for real pages; **partial** = implemented for a subset or headless-only;
**absent** = no handling at all; **wrong** = handled but observably off-spec.

Capability manifest: there is no `docs/status/capabilities.*`; the hand-maintained source
of truth is [`docs/status/coverage.toml`](`docs/status/progress.toml`) (71
`[[capability]]` rows, `state = done|partial|missing` with `issue` ids), rendered into
`PROGRESS.md` by `tools/update_coverage.py`. Its self-reported 77% (54/70) is
*feature-bucket* coverage, not page-realism coverage — hence this matrix.

HTML conformance is genuinely strong at the parser layer: vendored html5lib suites live
in `tests/html5lib-tests` (169 files: 15 tokenizer, 62 tree-construction `.dat` + 62 JSON,
5 serializer, 5 encoding) and drive `crates/test_harness/src/main.rs`; CSS Syntax WPT
lives in `tests/wpt/css/css-syntax`. The gaps below are *rendering
## js-web-apis
# JavaScript + Web API gap matrix — Megi engine

Repo: upstream `main` (9c94e86) @ `9c94e86` ("Merge pull request #1491 … fix/issue-1373-normalize"), read-only.
Subject: `crates/js/` (Boa 0.22 embedded runtime) and the host wiring in `crates/render/src/scripts.rs` and
`crates/browser/`.
Method: read `crates/js/src/{runtime,host,events,import_map,error,lib}.rs` (10,175 lines; there is no
`module.rs` — module loading lives in `runtime.rs` + `import_map.rs`), the 20 files in `crates/js/tests/`,
the render script host providers, and the shell worker. No cargo was run; every claim below is a code
citation.

**Headline.** The runtime is a *pre-render* JS environment: it installs ~45 globals, runs page scripts
during a deterministic render pass (lifecycle events + a virtual-time timer loop) and then drops the
realm. It has a credible ES-module story (import maps, top-level await) and a useful `fetch` shim, but it
has **no `navigator`, no `performance`, no `document.cookie`, no `TextEncoder`, no `scrollY`, no
`Element.querySelector`**, and the winit shell never dispatches user input into JS. That is the exact
opposite of what mainstream sites assume, so the gap list below is dominated by c
## net-security
# F5 gap matrix — networking + security (Megi, upstream `main` @ 9c94e86)

Read-only static review of `crates/net/src/**` plus the call sites in `crates/render`, `crates/js`,
`crates/html`, `crates/browser` that decide what a page can do over the network. No cargo was run and
nothing under upstream `main` (9c94e86) was modified.

Method: read every module header of `net` (they document scope and deliberate limits), then verified
each claim against the code (grep + targeted reads of `http.rs`, `h2/client.rs`, `hpack.rs`,
`loader.rs`, `cookies.rs`, `cors.rs`, `cache.rs`, `psl.rs`, `proxy.rs`, `socks.rs`, `pac.rs`,
`referrer.rs`, `range.rs`, `mime.rs`, `url.rs`, `data.rs`, `file.rs`, `compression.rs`, `error.rs`)
and against the JS/render side (`render/src/scripts.rs` fetch provider, `js/src/runtime.rs` fetch
prelude, `js/src/host.rs` FetchRequest). Cross-crate greps for the security-control keywords
(CSP, integrity/SRI, mixed content, secure context, HSTS, CT, COOP/COEP/CORP, X-Frame-Options,
sandbox, Content-Disposition, WebSocket, EventSource, ServiceWorker, multipart/FormData, DoH,
WWW-Authenticate, Sec-Fetch, PNA) are the evidence for every `absent` row: **sixteen of those greps

## shell-a11y-i18n
# Missing-feature matrix: shell/UX, accessibility, i18n

Repository: `megi` browser engine, read-only checkout of upstream `main` at
commit `9c94e86` ("Merge pull request #1491 from juanpimg/fix/issue-1373-normalize"),
worktree upstream `main` (9c94e86). Method: static reading only (no edits, no cargo), grep plus
targeted reads. Evidence lines are relative to that commit.

Deduped against already-filed work: shell #1163–#1178, shell tests #1379, fonts #1228–#1239,
bidi #1447/#1448, F2 lenses #1469–#1476. Those areas are cross-referenced, not re-filed.

Status legend: `present` (works, documented), `partial` (works with material limits),
`absent` (no implementation found).

---

## A. Shell / UX (`crates/browser/src/**`, 28 modules, 16,501 lines)

| # | Feature | Status | Evidence | Consequence | Rank |
|---|---------|--------|----------|-------------|------|
| A1 | Tabs | present | `window.rs:1680-1797` (`open_new_tab`, `close_active_tab`, `cycle_tab`, `activate_tab`), `tabs.rs:29` `MAX_TABS = 32`, `chrome.rs:512-610` tab-strip layout + hit testing, `chrome.rs:743-870` overflow menu | Ctrl+T/W/Tab, Alt+1..9, close button, overflow; no reorder/pin/duplicate/mute | 6 |
| A2 | History
