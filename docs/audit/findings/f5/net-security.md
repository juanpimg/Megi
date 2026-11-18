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
return zero matches in `crates/*/src`** (see §3).

Legend: `present` = implemented and wired for pages; `partial` = present but missing a piece that
matters (named); `absent` = no code path found.

---

## 1. Executive summary

The `net` crate is a serious, well-documented HTTP client: HTTP/1.1 with chunked/trailers/
`Expect: 100-continue`, a real HTTP/2 client (HPACK incl. Huffman, framing, flow control,
multiplexing, SETTINGS, opt-in push + priorities, opt-in h2c), TLS 1.2/1.3 via rustls with
webpki-roots, RFC 9111-subset cache with revalidation, cookies with SameSite + the real vendored
PSL, CORS response validation, referrer policy (all 8 values), range requests, compression with
anti-bomb caps, HTTP/HTTPS proxies + SOCKS5 + PAC, and a uniform `ResourceLoader` with caps. That is
much more than a toy.

The gaps are almost all on the **browser policy** side, not the transport side:

1. **No content-security policy at all.** No CSP parsing, no nonce/hash/`strict-dynamic`, no report-only.
   Every script the HTML references runs, and `document.write`/inline handlers are unrestricted.
2. **No SRI.** `integrity=` is ignored everywhere (even `import_map.rs` documents that `integrity` is
   ignored), so a compromised CDN or a MITM of a classic script executes immediately. CSP + SRI are
   the two defences a browser has against this, and neither exists.
3. **No mixed-content blocking.** An `https:` document can load `http:` scripts, stylesheets, images
   and fonts, and there is no upgrade-insecure-requests; combined with (1) and (2) a passive network
   attacker defeats TLS entirely.
4. **No HSTS** (no `Strict-Transport-Security` handling, no preload), no CT, no revocation, no
   system trust store, and a first-visit http→https upgrade only happens because the shell prefixes
   `https://` for bare domains.
5. **The page-facing API surface is far behind the Fetch/HTML standards**: `fetch()` is
   **same-origin only** (cross-origin rejects with "CORS: cross-origin fetch denied"), there is **no
   XMLHttpRequest**, **no `document.cookie`**, **no WebSocket/EventSource/ServiceWorker**, no
   `FormData`/multipart, and no 401/WWW-Authenticate handling (auth is limited to `user:pass@` in the
   URL). Consequently the CORS implementation is only reachable by embedders, never by page script.

Everything below is ordered by risk to a user browsing a hostile or compromised page.

---

## 2. Matrix

`evidence` cites the owning file:line; "missing" is what a browser does that Megi does not.

| # | Bucket | Status | Evidence | Missing part |
|---|--------|--------|----------|--------------|
| 1 | HTTP/1.1 core (chunked, trailers, 100-continue, keep-alive) | present | `net/src/http.rs:1-63` (features), `:2136` chunked, `:1236` trailers, `:737` `fetch_with_trailers`, `:404` `fetch_pooled`, `:297` keep-alive doc | No pipelining, no `103 Early Hints`, no HTTP/1.1 `Upgrade: websocket`, no priority model, `Connection: keep-alive` pooling only for a narrow opt-in subset (`http.rs:420-430`) |
| 2 | HTTP/2 | present | `net/src/h2/client.rs:1-50` (docs), `:1636` `get_many`, `:1751` `h2c_request`; `h2/hpack.rs:1-17`; `h2/frame.rs`; `h2/mod.rs:60-65` caps | No extended CONNECT (WebSocket over h2), no alt-svc/connection coalescing, no 421/Retry-Other, one GOAWAY only, encoder is static-table-only (`hpack.rs:6-8`), server push opt-in and cancelled by default |
| 3 | HTTP/3 / QUIC | absent | No QUIC dependency (`net/Cargo.toml:14-27`), no `Alt-Svc` handling anywhere (`rg` zero) | Whole transport; also no alt-svc discovery, no 0-RTT |
| 4 | TLS: versions/ALPN/SNI/validation | present | `http.rs:1655-1740`; `:1670` ALPN `h2`,`http/1.1`; `:1704-1740` rustls + webpki-roots; `:32` SNI+chain validation | No system trust store, no client certificates, no session-ticket persistence across processes (in-memory rustls store only), no ECH, no key-log |
| 5 | TLS: revocation / CT / pinning | absent | `rg -i 'ocsp|crl|transparency|pinning' net/src` → only CRLF text | No OCSP/CRLite/CRL, no SCT verification, no HPKP-equivalent |
| 6 | HSTS / preload / upgrade-insecure-requests | absent | `rg -i 'strict-transport-security|hsts' crates/*/src` → 0 matches | No STS store, no preload list, no upgrade-insecure-requests, no `block-all-mixed-content` |
| 7 | Cookies: parse/store/match | present | `cookies.rs:1-37`, `:266` parser, `:709` header build, `:124-134` caps (4096/180/4096 B) | No `__Host-`/`__Secure-` prefixes, no CHIPS/partitioned, no priority/`SameParty`, no non-IMF date formats, `HttpOnly` stored but no JS surface to hide from yet |
| 8 | Cookies: JS surface (`document.cookie`) | absent | `rg 'document.cookie' js/src` → 0; only the forbidden-header list (`js/src/runtime.rs:5798`) | No `document.cookie` getter/setter, so JS-set cookies, cookie-based feature flags and many auth flows break |
| 9 | SameSite / PSL | present | `cookies.rs:391` (`None` requires `Secure`), `:689` site rules, `:709`; `psl.rs:1-24` real vendored PSL 2026-11-11 | SameSite model is approximate (top-level vs subresource only), PSL is ASCII-only because URL has no IDNA; `cookies.rs:20-24` docs still call it a "reduced PSL" (stale) |
| 10 | CORS: response + preflight validation | partial | `cors.rs:196` `validate_response`, `:255` `validate_preflight_response`, `:167` `requires_preflight`; `http.rs:1051` `preflight` | No `Access-Control-Max-Age` preflight cache (`cors.rs:25`), no `Access-Control-Expose-Headers`, no `null`/opaque origins, no `Vary: Origin`, no private-network-access; and page fetch never uses it (row 19) |
| 11 | CSP (script-src, nonces, hashes, report) | absent | `rg -ni 'content-security-policy|script-src|nonce' crates/` → 0 matches | Entire CSP: parsing, per-document policy set, enforcement on inline/external scripts, styles, images, connect-src, report-only/report-uri |
| 12 | SRI (`integrity=`) | absent | `js/src/import_map.rs:12` ("`integrity` … ignored"); no `integrity` handling in `render/src/scripts.rs`, `stylesheets.rs`, `webfonts.rs` | Subresource integrity for classic/module scripts, stylesheets, preloads; hash verification before execution |
| 13 | COOP / COEP / CORP / cross-origin isolation | absent | `rg -i 'cross-origin-(opener|embedder|resource)-policy|crossOriginIsolated' crates/` → 0 matches | All three policies + `crossOriginIsolated`; no iframes exist yet so COOP/COEP have no consumer, but CORP would still apply to subresources |
| 14 | Referrer-Policy | present | `referrer.rs:1-40`, `:88` header, `:101` meta, `:155` `referer_for`; wired in `render/src/lib.rs:798-802` | No element-level `referrerpolicy`, no multiple-header precedence (first valid token wins), redirect hops keep the original document as the referrer source rather than the previous hop (`http.rs:810-935`), no `Referrer-Policy` on subresource responses |
| 15 | Redirect policy | partial | `http.rs:810-935` (max 10, 303→GET, POST 301/302→GET, cross-origin strips Authorization/Proxy-Authorization/Cookie), `:1116` `is_redirect` | No redirect mode (`follow`/`manual`/`error`), no https→http downgrade blocking, CORS validated only on the final response, streaming bodies cannot replay 307/308, no per-hop referrer |
| 16 | Auth: Basic/Bearer/Digest/NTLM, 401, WWW-Authenticate | partial | `http.rs:1587` `url_basic_authorization`, `:513-517` header injection; `error.rs:141` `ProxyAuthRequired` | No 401/407 challenge handling or retry, no `WWW-Authenticate` parsing, no Bearer/Digest/NTLM/Negotiate, no credential store/prompt, no `fetch(credentials:)` |
| 17 | Proxies: HTTP/HTTPS/SOCKS5/PAC | present | `proxy.rs:129`/`:146`; `socks.rs:1-20`; `pac.rs:1-30`; `http.rs:1606` CONNECT tunnel | HTTPS proxy cannot tunnel https targets (nested TLS out of scope, `http.rs:1260-1268`), proxy auth is Basic-only with no 407 retry, PAC consumes only the first `PROXY` token and ignores SOCKS directives, no WPAD |
| 18 | Cache (RFC 9111 subset) | partial | `cache.rs:1-30`, `:179-182`; `loader.rs:380-460` (store/revalidate/`Vary`/`Age`) | No `stale-while-revalidate`/`stale-if-error`, no `immutable`, one variant per URL, no `only-if-cached`/`max-stale`, `206` never cached, no `Age` emitted on reuse, range requests bypass cache |
| 19 | `fetch()` for pages | partial | `js/src/runtime.rs:5066-5098` prelude, `:5850-5920` native; `js/src/host.rs:168-173` `FetchRequest`; `render/src/scripts.rs:780-920` provider | **Cross-origin is rejected outright** (`scripts.rs:824-827`), so no CORS path; `Request` ignores `mode`/`credentials`/`redirect`/`referrer`/`integrity`/`keepalive`/`cache`/`priority`; response has no streaming (`body` is a 3-chunk fake), `redirected` always false, `type` missing, body is text-only (no Blob/FormData/ArrayBuffer), method whitelist, synchronous blocking call |
| 20 | XMLHttpRequest | absent | `rg 'XMLHttpRequest' crates/` → 0 matches | Entire XHR (most legacy libraries and many sites still use it, often as the only transport) |
| 21 | WebSocket | absent | `net/src/lib.rs:72` ("no WebSocket"), `rg -i 'sec-websocket' crates/` → 0 | `ws:`/`wss:` schemes, handshake, framing, masking, ping/pong, close codes, backpressure |
| 22 | EventSource / SSE | absent | `rg 'EventSource|text/event-stream' crates/` → 0 | Whole API + streaming line parser |
| 23 | Service workers | absent | `rg 'ServiceWorker|serviceWorker' crates/` → 0 | Registration, install/activate, cache API, fetch interception (worth explicitly deferring) |
| 24 | Forms: submission, `multipart/form-data`, `application/x-www-form-urlencoded` | absent | `shell/src/forms.rs:1-24` ("no backend, `Enter` never submits"), `rg 'multipart/form-data|FormData' crates/` → 0 | Form submission (GET/POST), enctype serialization, `FormData`/file inputs, `URLSearchParams` bodies |
| 25 | Request priorities / Early Hints / client hints | absent | no `fetchpriority`/`Priority` wiring in `render` (h2 `Priority` type exists but `H2Request::new_with_body` never sets it, `h2/client.rs:127-196`); `rg 'Sec-Fetch|Sec-CH'` → 0 | Priority hints, `103 Early Hints` preload, `Sec-Fetch-*`, UA client hints — sites that gate behaviour on `Sec-Fetch-Site`/CH see a non-browser |
| 26 | DNS | partial | `http.rs:1539` `connect_tcp` uses `ToSocketAddrs` (system resolver only) | No DoH/DoT, no DNS cache, no `Alt-Svc`-driven connection coalescing, no ECH |
| 27 | URL / IDNA / schemes | partial | `url.rs:31-40` `Scheme` = http/https/file/data only; `psl.rs:16-18` "IDNA is not implemented" | No IDNA/punycode (non-ASCII host names fail to parse at all), no `ws`/`wss`/`blob`/`about`/`mailto`/`ftp`, legacy URL forms rejected by design |
| 28 | `data:`/`file:`/MIME | present | `data.rs:37`, `file.rs:36,54`, `mime.rs:1-30`; caps `MAX_FILE_BYTES` 64 MiB, `MAX_DATA_URL_PAYLOAD_BYTES` 16 MiB | No `multipart/byteranges` (multi-range rejected, `range.rs`), sniffing has no font/audio/video/UTF-16 signatures, no `X-Content-Type-Options: nosniff` handling |

---

## 3. Security controls: does Megi enforce them?

| Control | Enforced? | Evidence |
|---|---|---|
| Content Security Policy | **no** | zero matches for any CSP token in `crates/`; documents render whatever the HTML references |
| Subresource Integrity | **no** | `import_map.rs:12` says `integrity` is ignored; no hash checks in script/style/font loaders |
| Mixed content blocking / upgrade-insecure-requests | **no** | `render/src/lib.rs:742-780` builds subresource options without any scheme check; `ensure_supported_scheme` (`stylesheets.rs:256-259`) accepts `http` from any document; only `--no-network` blocks (opt-in) |
| HSTS / preload | **no** | zero matches; the shell merely prepends `https://` for bare hosts (`chrome.rs` address bar), which is not a policy |
| Secure-context gating | **no** (moot) | no `isSecureContext`, no WebCrypto, no geolocation/notifications/SW to gate; `localStorage` works on http as in browsers |
| Same-origin policy for page fetch/XHR | **enforced (stricter than browsers)** | `render/src/scripts.rs:811-827`: scheme+host+port equality or reject; there is no no-cors/opaque mode, so cross-origin reads are impossible (no XHR at all) |
| Cookie flags | **partially** | `Secure`/`HttpOnly`/`SameSite` stored and `Secure`/SameSite enforced on send (`cookies.rs:391,689,709`); `HttpOnly` is inert because no JS cookie API exists; no `__Host-`/`__Secure-` prefix rules |
| CORS | **partially** | validation exists (`cors.rs:196,255`) but is only reached by embedders; page `fetch()` rejects cross-origin before any request is made |
| Referrer policy | **yes (approximate)** | `referrer.rs` all 8 policies, header > meta > default, applied to every hop; element-level attribute unsupported |
| Sandboxing (`<iframe sandbox>`) | **no** | no iframes are rendered at all (`render/src/lib.rs:186`: "No forms, no iframes, no web fonts, no srcset" — the webfont part is stale) |
| Download safety | **partial** | `shell/src/download.rs:1-20`: only the page snapshot can be saved, filenames sanitized and capped, never overwrites; no `Content-Disposition`/attachment handling (zero matches), no MIME-based danger checks, no download of subresources |
| Cross-origin isolation (COOP/COEP) | **no** | zero matches; no `crossOriginIsolated`, no `SharedArrayBuffer` gating (no SAB API either) |
| Revocation (OCSP/CRL) / CT | **no** | rustls/webpki verifies the chain at handshake time only; no revocation or SCT check |
| Certificate/transport trust | **yes** | `http.rs:1704-1740`: webpki-roots Mozilla bundle, hostname + chain validation, SNI, TLS 1.2/1.3 only |
| Anti-DoS on untrusted responses | **yes** | header/body/decompression caps (`http.rs:88-95`, `compression.rs:41-43`), h2 frame/header caps (`h2/mod.rs:60-65`), cookie/jar caps, cache caps |

---

## 4. Top 15 by risk/impact (with the missing → consequence → sketch rule)

Ranking is "risk to a user of the current engine", security first, then compatibility breakage, then
transport quality.

1. **No CSP enforcement (`absent`, critical).** Any inline/external script, `on*` handler,
   `javascript:` URL or `document.write` runs unconditionally, so one HTML-injection point anywhere
   in a page or template is full code execution with no policy stop. → *sketch:* `net`/`render`:
   parse `Content-Security-Policy`(+`-Report-Only`) headers and `<meta http-equiv>` (≈400 lines in a
   new `render/src/csp.rs` or `net/src/csp.rs`), build an `Origin`-scoped `Policy` on `LoadedDocument`,
   pass it through `ScriptingOptions`/`SubresourceFetchOptions`, gate every script creation site in
   `render/src/scripts.rs` (`execute_scripts`, module loader, inline handlers, `eval`) plus style/img/
   connect fetches; add nonce/hash matching and violation reporting. Est. 900–1500 lines + tests.
2. **No SRI (`absent`, high).** `integrity=` is silently ignored, so a compromised third-party script
   host, a MITM of a classic script (no mixed-content blocking either) or a stale CDN serves code that
   executes. → *sketch:* `render/src/scripts.rs` + `stylesheets.rs`: parse `integrity` into
   `(alg, digest)` pairs, fetch resource, verify SHA-256/384/512 before decode/execute, fail closed
   with a warning; also verify preload/`<link>` and module imports (`js/src/import_map.rs:12`).
   Est. 250–350 lines + fixtures.
3. **No mixed-content blocking (`absent`, high).** An `https:` page happily loads `http:` scripts,
   CSS, images and fonts on the same origin; with no CSP/SRI an on-path attacker chooses the JS. →
   *sketch:* `render/src/lib.rs` (where `subresource_fetch_options` is built): a
   `MixedContentPolicy` computed from `loaded.url.scheme()`; block/upgrade `http` for script/style/
   font/fetch destinations, allow image/audio/video as "optionally blockable" (block by default like
   Chrome, upgrade behind a flag); plus `upgrade-insecure-requests` support. Est. 150–250 lines.
4. **No HSTS (`absent`, high).** First request to `http://bank.example` stays http unless the user
   typed a scheme; no STS store means every later navigation can be silently downgraded and
   `Secure`/SameSite=None cookies cannot be assumed protected. → *sketch:* new
   `net/src/hsts.rs` (~150 lines, RFC 6797 host store with max-age/expiry, persisted by the shell
   session), hook in `http.rs` before request framing (rewrite scheme, send no plaintext) and in
   `render` navigation; ship the Chromium preload list as `include_str!`+hash set (≈100k domains,
   one file, ~50 lines to load). Note browsers also need it for the mixed-content upgrade path.
5. **`fetch()` is same-origin only (`partial`, high compat).** Every cross-origin API call fails with
   "CORS: cross-origin fetch denied"; the CORS code in `net` is unreachable from page script, so
   almost every real site that talks to its backend on `api.` or a SaaS endpoint breaks. → *sketch:*
   `render/src/scripts.rs` fetch provider: stop pre-rejecting; build a `megi_net::FetchOptions` with the
   document origin and mode (cors/no-cors/same-origin), run simple requests, run `megi_net::http::preflight`
   for non-simple ones, validate responses via `cors::validate_response`, map to Fetch terms
   (`type: "cors"|"opaque"`, `redirected`), expose `Access-Control-Expose-Headers`; extend
   `megi_js::host::FetchRequest` with `mode`/`credentials`/`redirect`. Est. 500–800 lines.
6. **No XMLHttpRequest (`absent`, high compat).** Countless libraries and legacy sites feature-detect
   XHR as their only transport; no XHR means those pages render but their data never loads. →
   *sketch:* `js/src/xhr.rs` (~600–900 lines) as a thin shim over the (fixed) fetch provider:
   `open/send/abort`, `onreadystatechange`/events, `responseType` (text/arraybuffer/json/blob→text),
   `setRequestHeader`, timeout, sync mode may throw. Lower cost alternative: a minimalshim in the JS
   prelude (no Rust) covering GET/POST + `responseText`/`response`.
7. **No `document.cookie` (`absent`, high compat/security).** JS cannot read or set cookies, which
   breaks session bootstrap for SPAs, consent/privacy banners, A/B test assignment and CSRF-token
   reading (very common double-submit patterns), and means `HttpOnly` has nothing to protect. →
   *sketch:* `js/src/host.rs` + `render/src/scripts.rs`: `CookieProvider` closure with
   `get(url)->String`/`set(url, cookie)`, wired to the render's `Arc<Mutex<CookieJar>>`; serialize
   `document.cookie` in `js/src/runtime.rs` prelude; reject `HttpOnly` writes from script (needs a
   `http_only` check in the jar) and enforce the synchronous-visibility quirks. Est. 250–400 lines.
8. **Auth is URL-userinfo Basic only (`partial`, medium-high).** Sites behind 401 (`WWW-Authenticate`)
   show the 401 body as content, there is no credential prompt or retry, no Bearer token flow (SPA
   login breaks), no Digest/NTLM for intranet servers, and `fetch(credentials:)` doesn't exist. →
   *sketch:* `net/src/http.rs`: on 401/407 parse `WWW-Authenticate`, consult a
   `CredentialStore`/callback in `FetchOptions`, retry once with `Authorization` (Basic/Digest/Bearer);
   shell: prompt dialog reusing the address-bar text editor; JS `fetch` credentials mapping to the jar
   mode. Est. 400–700 lines.
9. **No WebSocket (`absent`, medium-high compat).** Realtime apps (chat, dashboards, streaming
   push) that only speak WS show empty panes. → *sketch:* `net/src/websocket.rs` (~800–1200 lines):
   `ws:`/`wss:` URL parsing, HTTP/1.1 Upgrade handshake over the existing `connect_tls` path,
   RFC 6455 framing with masking, ping/pong, close codes, fragmentation, max payload caps; expose to
   JS as a `WebSocket` class whose events are pumped by the render event loop. (h2 extended CONNECT is
   a follow-up, skip for v1.)
10. **Redirect policy is a blunt instrument (`partial`, medium).** No `redirect: "manual"|"error"`,
    https→http downgrades followed silently (no mixed-content check on redirects), CORS validated only
    on the final response, and the `Referer` for every hop is computed from the original document
    rather than the previous hop. → *sketch:* `net/src/http.rs:810-935`: add `RedirectMode` to
    `FetchOptions`, block scheme downgrades for non-GET/navigational requests unless the initiator is
    http, validate CORS on each redirect response when a `Origin` is set, and update the referrer
    source per hop (`ReferrerConfig::source = previous_url`). Est. 120–200 lines.
11. **No cookie prefixes / partitioning (`partial`, medium).** `__Host-`/`__Secure-` cookies can be
    set from insecure or subdomain contexts and shadow the protected ones; no CHIPS means
    third-party contexts (once iframes exist) would share one jar. → *sketch:* `cookies.rs`:
    reject `__Host-` unless `Secure`+host-only+`Path=/`, reject `__Secure-` without `Secure`;
    add `PartitionKey` when iframes land. Est. 100–180 lines.
12. **No `FormData`/multipart/urlencoded form submission (`absent`, medium compat).** `<form
    action=... method=post>` does nothing today (`shell/src/forms.rs:1-24`), and `fetch` bodies are
    text-only, so logins/uploads written as plain HTML forms cannot work at all. → *sketch:*
    `net/src/multipart.rs` (~250 lines, boundary generation + streamed parts), `html`+`render`: a
    form-submission path collecting named controls, honoring GET/POST + enctype, and a
    `FormData`/`URLSearchParams` body in the fetch prelude. Est. 500–800 lines.
13. **No-cors destinations are fetched in CORS mode (`partial`, medium compat/perf).** `stylesheets`,
    `scripts`, `@font-face`, CSS background images and worker scripts all call
    `ResourceLoader::load` with the document `Origin` set (`render/src/*.rs:196,524,1067,283`,
    `lib.rs:1483`), so a cross-origin CSS/JS/font without `Access-Control-Allow-Origin` is dropped —
    real browsers send those as no-cors requests (only `crossorigin` images opt in, as `images.rs`
    correctly does). → *sketch:* choose `load` vs `load_no_cors` per destination using the fetch
    "request mode" table (script/style/font/img/background = no-cors unless `crossorigin`), then use
    `load_no_cors` and keep CORS only for module scripts/fonts where the spec requires it. Est.
    100–150 lines.
14. **No HTTP/3/QUIC and no alt-svc (`absent`, medium perf).** Some origins serve h3 only; all others
    lose h3's handshake/head-of-line benefits on lossy links. → *sketch:* `net/Cargo.toml` +
    `net/src/h3/` (~1500–2500 lines using `quinn`+`h3` crates, keeping `#![forbid(unsafe_code)]` at
    our level), `Alt-Svc` parsing in `http.rs` with a per-origin cache, fallback to h2/h1. This is a
    whole milestone, not a patch — defer.
15. **No request priorities / Early Hints / Sec-Fetch / client hints (`absent`, medium-low).** No
    `fetchpriority`, CSS/JS/image priority classes or `103` preload hooks, and servers that branch on
    `Sec-Fetch-Site`/`Sec-CH-UA` see a non-browser UA, which can change what they send (feature
    gating, bot walls) and removes server-side CSRF heuristics. → *sketch:* `render`: priority fields
    on `ResourceLoader` requests feeding the h2 `Priority`; document-order → priority mapping for
    render-blocking CSS/JS; `103` handling in `http.rs` surfacing `Link` preloads; add
    `Sec-Fetch-Site/Mode/Dest/User` and a static CH set in `http.rs` request framing. Est. 300–500.

Also worth filing but below the line: no DoH/DoT (`http.rs:1539`), no IDNA (`url.rs`), no
`Content-Disposition` handling for attachments (`rg` zero), no meta-refresh (`<meta http-equiv=
"refresh">` not honoured anywhere), no `EventSource`, no service workers, and stale module docs
(`lib.rs:56-70` still claims "no HTTP/2 or streaming", `net/src/lib.rs:72` claims no WebSocket while
h2c is documented below; `cookies.rs:20` calls the full PSL "reduced").

---

## 5. Proposed issue backlog (12, new — deduped against #1252-#1265, #1284-#1298, #1343-#1352,

#1405, #1453, #1473, #1475, #1478-#1487)

| Sev | Title | Where the work lands |
|---|---|---|
| P1 | No CSP enforcement: implement parsing + enforcement for all script/style/resource loads | `net`/`render` (+ js hooks) |
| P1 | `integrity=` (SRI) is ignored; verify hashes before executing/decoding subresources | `render` (+ `js` import maps) |
| P1 | No mixed-content blocking: https documents load http subresources and run their scripts | `render` (+ `net` upgrade policy) |
| P1 | No HSTS: add RFC 6797 store + preload list; nothing prevents silent https→http downgrade | `net` (+ `shell` persistence) |
| P1 | `fetch()` rejects all cross-origin requests; CORS is unreachable from page script (no no-cors/opaque modes) | `render`/`js` (+ `net` FetchOptions) |
| P1 | `XMLHttpRequest` is entirely absent, breaking most legacy/non-module sites | `js` (shim over fetch) |
| P1 | No `document.cookie`, so page script cannot read/set cookies and `HttpOnly` has no consumer | `js`/`render` (+ `net` jar) |
| P2 | Only URL-userinfo Basic auth exists: no challenge handling (401/407), no Bearer/Digest/NTLM, no credential store | `net`/`shell` (+ js credentials) |
| P2 | WebSocket absent (no `ws:`/`wss:`, handshake or framing) | `net` + `js` event loop |
| P2 | Cookie prefixes (`__Host-`/`__Secure-`) and partitioning are not enforced | `net/cookies.rs` |
| P2 | Redirect handling lacks redirect modes, downgrade protection, per-hop CORS validation and per-hop referrer | `net/http.rs` |
| P2 | No-cors destinations (classic script/style/font/background) are fetched in CORS mode and dropped cross-origin | `render` + `net/loader.rs` |

Deferred explicitly (too large for a batch): HTTP/3/QUIC, Service Workers, EventSource, FormData/
multipart form submission, DoH, client hints, extended CONNECT.

---

## 6. Files read (line ranges)

`net/src/lib.rs:1-176`; `http.rs:1-260,404-560,610-935,1051-1130,1123-1400,1539-1760,2100-2280`;
`loader.rs:1-140,340-470,516-600`; `cookies.rs:1-140,266-400,520-720`; `cors.rs:1-60,160-300`;
`cache.rs:1-220`; `h2/client.rs:1-140,270-320,900-960,1600-1820`; `h2/frame.rs:1-80`;
`h2/hpack.rs:1-40`; `h2/mod.rs:1-65`; `proxy.rs:1-160`; `socks.rs:1-40`; `pac.rs:1-70`;
`referrer.rs:1-180`; `range.rs:1-60`; `mime.rs:1-40`; `url.rs:1-80`; `data.rs:1-60`;
`file.rs:1-70`; `compression.rs:1-90`; `psl.rs:1-60`; `error.rs:1-150`; `body.rs:1-40`;
`crates/render/src/scripts.rs:490-540,740-930,1000-1100`; `render/src/lib.rs:730-815,1450-1500,
1900-1930`; `render/src/stylesheets.rs:240-260`; `render/src/images.rs:140-200`;
`render/src/webfonts.rs:180-200`; `js/src/runtime.rs:4918-5100,5840-5960,5786-5835`;
`js/src/host.rs:160-200`; `html/src/tree_builder.rs:3430-3445`; `shell/src/forms.rs:1-30`;
`shell/src/download.rs:1-30`; `shell/src/refresh.rs:1-20`; `crates/net/Cargo.toml:1-30`;
`docs/ROADMAP.md` (Fase 5/6 status rows); `docs/audit/findings/wave1.json` (dedupe).

Lines read ≈ 4.1k (targeted ranges; every module header of `net` + every call site listed above).
