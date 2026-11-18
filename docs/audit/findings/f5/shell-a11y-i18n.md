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
| A2 | History & navigation | present | `navigation.rs:1-22` (truncation, no-op same URL, `MAX_HISTORY = 256`, failed navigations keep entry), `window.rs:551-580`, `chrome.rs:1267-1321` nav buttons | Back/forward/reload incl. Alt+←/→, bounded session history | 4 |
| A3 | Address bar | partial | `chrome.rs:97-380` (`AddressBar`: caret, selection, internal clipboard, autocomplete), `chrome.rs:404-458` `resolve_address`, painted at `chrome.rs:1573-1617` | Typed URLs work, but no origin/security indicator (no lock/scheme chip), no per-tab URL bar fields, no secure-context cue | 5 |
| A4 | Find in page | present | `find.rs:1-22` (case-insensitive DOM search, fragment-level highlights, no regex/whole-word), `window.rs:1792-1890` | Ctrl+F, next/prev, highlights; no regex/word modes | 8 |
| A5 | Zoom | present | `window.rs:1046` `set_zoom`, `window.rs:1919-1940` ladder, `zoom.rs`, per-origin `site_zooms` in `session.rs` | Ctrl+=/-/0, 50–300 %, per-origin persistence | 9 |
| A6 | Reader mode | present | `reader.rs:1-13`, `window.rs:1189` `open_reader`, `pages.rs:62` `reader_page` | Ctrl+Shift+R / about:reader, 700 px serif, no scripting | 10 |
| A7 | Bookmarks | absent | no bookmark module; docs `lib.rs:148` "no bookmarking" | No way to save/recall pages; history menu is read-only | 18 |
| A8 | Downloads | partial | `download.rs:1-13` (page PNG/HTML + images, atomic writes, status via title/stdout), `window.rs:1125-1173` | Ctrl+S / Ctrl+Shift+S work; no download shelf/manager, cancel, pause, open-after-save, or resource downloads | 12 |
| A9 | Scrollbars | absent | page scroll is wheel/keys only (`window.rs:2120-2140`, `scroll.rs`); only scrollbar is the dropdown list (`chrome.rs:1685-1706`) | No visual scroll position/thumb; discoverability and a11y of scrolling poor; no drag-to-scroll | 14 |
| A10 | Keyboard shortcuts | present | `window.rs:45-56` keymap table, handler `window.rs:2071-2426` | Rich (Ctrl+L/F/T/W/S/H/D, F5, zoom, tab cycling); no Ctrl+C/V for page, no Ctrl+P, no F11, no Ctrl+N | 7 |
| A11 | Context menu | present | `window.rs:700-800`, `chrome.rs:1105-1256` | Open/copy link, save image, reload, save PNG/HTML; "copy image" explicitly unsupported | 13 |
| A12 | DevTools | absent | no inspector/console surface anywhere in `crates/browser`; only Ctrl+D prints URL/title to stdout (`window.rs:1899`) | No way to inspect DOM/CSS/computed styles or script errors beyond stderr; debugging is opaque | 19 |
| A13 | View source | absent | no `view-source` handling (`rg view-source` empty) | Cannot inspect the received/serialized source | 20 |
| A14 | Print | absent | `render/lib.rs:826,1274` always build `MediaContext::with_viewport` (`screen: true`, `css/src/media.rs:33-57`); no Ctrl+P; no print path | `@media print` never applies; no page-to-printer/PDF pipeline | 17 |
| A15 | Fullscreen | absent | no `set_fullscreen`/`Fullscreen` window events (`rg fullscreen` empty) | No F11/fullscreen kiosk mode | 21 |
| A16 | Copy/paste | partial | address bar only: `chrome.rs:92` "internal clipboard (Ctrl+C/X/V copy within the shell; the system clipboard is not touched)", `window.rs:300-301,1311-1315`; no page text selection model | Cannot select/copy page text or paste into the page; no system clipboard interop at all | 11 |
| A17 | Drag & drop | absent | no `HoveredFile`/`DroppedFile`/drag handlers in `window.rs:2071-2426` | Cannot drop a file/URL onto the window | 22 |
| A18 | file:// handling | present | `pages.rs:154-360` (`read_directory`, `listing_html`, `file_url`, `resolve_directory`), `net/src/file.rs` | Directory listings, path resolution, error pages for unreadable paths | 15 |
| A19 | about: pages | partial | `pages.rs:31` `history_page`, `pages.rs:62` `reader_page`, `megi://` aliases | about:history / about:reader only; no about:blank, version, chrome://, config | 24 |
| A20 | Session restore | present | `session.rs:367-494` (`load`/`save`, `MAX_SESSION_BYTES`, schema v1), `window.rs:332-365` (auto-restore + notice), `session.rs:529-557` lock | Tabs/history/zoom/scroll restored automatically; `--no-session`; no "restore after crash" prompt and the lock serializes instances | 16 |
| A21 | Multiple windows | absent | single `EventLoop` (`window.rs:203`) and one `create_window` (`window.rs:441`); no new-window command; `SessionLock` (`session.rs:529`) prevents a second instance sharing the session | No Ctrl+N / window.open / target=_blank window; everything is a tab | 23 |
| A22 | Favicons | present | `favicon.rs:1-14` (first `rel=icon`, 64 KiB cap, 16×16 nearest-neighbour, deterministic fallback dot), no `sizes`/`type`/SVG | Tab favicons or colour dots; crisp icons may be missing | 25 |
| A23 | Title / status bar | present | `title.rs:127` `window_title` (`title — short URL`, scroll %, `[recortado]` marker), `chrome.rs:1202-1217` status bar (hover URL, download text) | Title reflects doc/scroll/errors; status bar shows hover target and download progress | 26 |
| A24 | Error pages | present | `pages.rs:273-291` `error_html` (code + message + retry link), `pages.rs:341-364` `error_code`, caller in `window.rs` render-error path | DNS/TLS/HTTP/FILE_IO codes and "Reintentar" retry link (Spanish string, see C12) | 27 |
| A25 | Dialogs (alert/confirm/prompt) | absent | no `alert`/`confirm`/`prompt` bindings in `crates/js/src` host (`rg alert` only test `role=alert`); no modal surface in shell | Page scripts calling `alert()` throw instead of showing a dialog; forms/data flows using `confirm()` break silently or throw | 2 |
| A26 | Autofill | absent | no credential/autofill code in `crates/browser` | No password manager hints, no form autofill | 28 |
| A27 | Permission prompts | absent | no permission model/UI (only CORS/cookies via `net`); no `navigator.permissions` host | Geolocation/notifications/clipboard-read requests cannot be granted or denied | 29 |
| A28 | PDF viewer | absent | only a `.pdf` extension heuristic in `chrome.rs:441`; no PDF decoder/layout | PDFs show as unknown/binary content or fail to render | 30 |
| A29 | Media playback | absent | layout only gives placeholders (`layout/src/tree_builder.rs:511` `video` → 300×150, `:514` `audio` → 300×30); no decoder/controls pipeline | `<video>`/`<audio>` render as empty boxes; no playback, no controls | 30 |
| A30 | Smooth scrolling | absent | `links.rs:26` "no in-page smooth scrolling yet"; wheel/keys jump directly (`scroll.rs`) | Fragment/anchor navigation and wheel scrolling are abrupt | 31 |
| A31 | Dark mode | absent | `MediaFeature::Unsupported` = "always evaluates to `false`" (`css/src/media.rs:95-101`; test `media.rs:559` asserts `prefers-color-scheme: dark` false); chrome palette hardcoded (`chrome.rs:47-89`) | Pages' `@media (prefers-color-scheme: dark)` never applies; no user theme switch; chrome is fixed dark/light mix | 3 |

### A — missing → consequence → minimal implementation sketch

- **A25 dialogs (rank 2).** Missing → `alert/confirm/prompt` are reference errors or silent
  no-ops; user cannot answer. Sketch: `crates/js/src/host.rs`-style host callbacks →
  `shell/src/window.rs` modal state; reuse chrome drawing (`chrome.rs` paints a centered panel,
  `forms.rs` handles focus), ~250 LOC across shell + js host + `dom` text. Blocking semantics
  (script paused) can be synchronous read of modal result.
- **A31 dark mode (rank 3).** Missing → dark-preference pages render light. Sketch:
  extend `megi_css::MediaContext` (`css/src/media.rs:33`) with `color_scheme: Light|Dark` and
  `prefers_color_scheme_queries`; build it in `render/src/lib.rs:826,1274`; add shell flag
  `--color-scheme` + `prefers-color-scheme` in chrome palette (`chrome.rs:47-89`), ~120 LOC.
- **A16 copy/paste (rank 11).** Missing → no text selection/copy or paste into pages. Sketch:
  add text selection range model in `layout`/`shell` (hit-test → `(NodeId, Range)`), paint
  selection rects, `Ctrl+C` → system clipboard via `arboard` (new dependency), `Ctrl+V` in
  `forms.rs`; ~450 LOC.
- **A3 address-bar security UI (rank 5).** Missing → users cannot tell scheme/origin. Sketch:
  derive scheme + host from `nav.current()` and paint a lock/`https` chip before the URL in
  `paint_address_bar` (`chrome.rs:1573`), ~60 LOC.
- **A9 page scrollbars (rank 14).** Missing → no scroll affordance. Sketch: compute
  content/viewport ratio in `window.rs` (already in `scroll.rs`), paint a 8 px thumb strip in
  `present()` overlay (mirror `chrome.rs:1685-1706`), support drag, ~150 LOC.
- **A8 downloads (rank 12).** Missing → no manager/cancel. Sketch: `download.rs` progress
  queue + overlay panel in chrome, ~200 LOC.
- **A7 bookmarks / A12 devtools / A13 view-source / A14 print / A15 fullscreen / A17 DnD /
  A20-per-window / A21 multi-window / A26 autofill / A27 permissions / A28 PDF / A29 media.**
  Each is a separate subsystem (bookmarks ~150 LOC storage + menu; devtools ~600 LOC DOM/CSS
  inspector; view-source ~40 LOC re-fetch+wrap; print ~300 LOC print media + raster to printer
  or PDF writer; fullscreen ~30 LOC winit; DnD ~120 LOC; window.open ~250 LOC;
  autofill/permissions ~300 LOC model+UI; PDF ~2k LOC or embed a library; media ~1k+ LOC/day).
  Ranking: dialogs > dark mode > address-bar security > downloads > shortcuts coverage >
  scrollbars > bookmarks > print > devtools > view-source > fullscreen > multi-window > PDF >
  media.

---

## B. Accessibility

**Bottom line: there is no accessibility exposure.** A `crates/browser/src/a11y.rs` module
(321 lines) computes a *pure snapshot* — roles via implicit tag/`role` mapping, names via
`aria-label`/`aria-labelledby`/`title`/`alt`/`value`/`placeholder`, hidden/disabled state,
and a document-order focus list — but it is **never used by the window or any consumer**:
`rg 'collect_access|focus_order|AccessNode|a11y::' crates --glob '!**/a11y.rs'` matches only
the `pub use` re-export at `crates/browser/src/lib.rs:206`. Consequently:

- No a11y tree is exposed to platform APIs (AccessKit/AT-SPI/UIA/NSAccessibility); a screen
  reader sees an opaque blank window. Module doc says so plainly: "no screen-reader bridge"
  (`a11y.rs:1-20`).
- No role/name semantics are attached to anything the shell draws: chrome, tabs, address bar
  and page pixels are a raw `Pixmap` (`chrome.rs:1408` `draw_chrome`).
- `<label for>`, `aria-describedby`, `aria-expanded/checked/pressed`, `aria-live`, image
  `alt` fallback rendering, and `role=presentation` are unimplemented; `aria-*` attributes are
  stored in the DOM like any other attribute (no semantics layer outside `a11y.rs`).
- Keyboard focus is limited to **form controls**: `window.rs:1599`
  `if matches!(key, Key::Named(NamedKey::Tab)) && !self.forms.is_empty()` drives
  `forms::FocusState`/`FocusModel` (`forms.rs:543-590`); links, buttons, checkboxes outside
  forms and images are not reachable by Tab and there is no Enter/Space activation. The focus
  ring exists only for form controls (`forms.rs:797`, `830` `focus_ring: focused && !disabled`),
  painted as a UA border; `:focus`/`:focus-visible` selectors are `PseudoClass::Never`
  (`css/src/selector.rs:284`) so no CSS focus styling can apply.
- There is no focus management (`autofocus`, scroll-into-view, focus trapping in menus,
  returning focus after closing find/address bar), no live-region announcements, and the
  reduced-motion preference is unmodeled (`prefers-reduced-motion` = `MediaFeature::Unsupported`
  → always false, `css/src/media.rs:98-101`).
- Chrome itself has no keyboard traversal: Ctrl+L/Ctrl+F exist, but the tab strip, nav
  buttons, status bar and menus are mouse-only (`chrome.rs` hit tests; `window.rs:2071-2426`).

| # | Item | Status | Evidence | Consequence | Rank |
|---|------|--------|----------|-------------|------|
| B1 | Accessibility tree / screen-reader bridge | absent | `a11y.rs:1-20` doc "no screen-reader bridge"; `a11y::*` referenced only by `lib.rs:206`; `rg 'AccessKit|atspi|uia' crates` empty | Screen readers cannot read any page: total a11y blocker | 1 |
| B2 | Keyboard access to page content (links/buttons) | absent | `window.rs:1599` Tab gated on forms; `a11y::focus_order` unused; no Enter/Space link/button activation in `window_event` | Keyboard-only users cannot follow links or press buttons | 2 |
| B3 | Visible focus indicator beyond form controls | partial | `forms.rs:797,830` only; `css/src/selector.rs:284` `:focus`/`:focus-visible` never match; no outline paint for links | No visible focus for most interactive content | 3 |
| B4 | Accessible-name computation | partial | `a11y.rs:91-116` aria-label → labelledby text → title/alt/value/placeholder → text; no `<label for>`, `aria-describedby`, `role=presentation`, heading level from `aria-level` | Names are often empty/duplicated; form fields unlabeled | 4 |
| B5 | ARIA state/relations (expanded/checked/pressed/live) | absent | only `aria-hidden`, `inert`, `disabled` honored (`a11y.rs:24-60`); no `aria-live`, `aria-expanded`, `aria-checked` propagation | Dynamic UI (menus, toggles, toasts) is silent to AT | 5 |
| B6 | Focus management (autofocus, scroll-into-view, focus trap, restore) | absent | no `autofocus` handling; `window.rs:700-800` menus keep focus on page; find bar/address bar open without focus announcement | Focus gets lost/never moves; disorienting for AT and keyboard users | 6 |
| B7 | Reduced motion | absent | `css/src/media.rs:98-101` `prefers-*` = `Unsupported` → always false; no shell setting | Animations cannot be disabled; vestibular-safety gap | 7 |
| B8 | High contrast / forced colors | absent | `forced-colors` unsupported (same `Unsupported` path); chrome palette fixed (`chrome.rs:47-89`) | No OS high-contrast adaptation of page or chrome | 8 |
| B9 | Chrome accessibility (tab strip, address bar, menus) | absent | chrome is drawn via `chrome.rs:1408` `draw_chrome` into `Pixmap`; no AccessKit nodes, no keyboard traversal | AT users cannot operate the browser UI itself | 9 |
| B10 | Image `alt` fallback rendering | absent | `alt` only read by `a11y::name` (`a11y.rs:91-116`); no alt text rendered when `<img>` fails/empty | Broken/omitted images render as empty boxes even with meaningful alt | 10 |

### B — what a minimal a11y layer would need (ranked, with rough size)

1. **Expose the tree (P1, ~300 LOC).** Adopt `accesskit` + `accesskit_winit`; new
   `crates/browser/src/a11y_bridge.rs` maps `a11y::collect(doc, root)` → `accesskit::Node`
   (role, name, hidden, disabled, focusable), rebuild on frame delivery
   (`window.rs::drain_worker_messages`) and push focus/scroll events; add the crate deps in
   `crates/browser/Cargo.toml`. This alone makes pages readable by AT.
2. **Keyboard focus for all focusables (P1, ~180 LOC).** In `window.rs:1599`, replace the
   forms-only Tab with `a11y::focus_order(&doc)` union form fields; paint a focus ring for
   non-form nodes (reuse `forms.rs:830` style in `present()`); add Enter/Space activation for
   `link`/`button` via `links.rs` (already resolves click targets) — ~120 LOC window +
   60 LOC paint.
3. **Focus-visible + CSS focus states (P2, ~130 LOC).** `css/src/selector.rs:284`: track
   `:focus`, `:focus-visible`, `:focus-within` against the shell's focused node; wire the
   dynamic state into the cascade; then pages style their own focus rings.
4. **Name/description polish (P2, ~90 LOC).** Extend `a11y.rs:91-116`: `<label for>` lookup,
   `aria-describedby`, `aria-expanded/checked/pressed`, `img alt` as name, `role=presentation`.
5. **Live regions (P2, ~120 LOC).** Poll `aria-live`/`role=alert|status` subtrees after script
   runs (`render` → shell frame metadata) and emit AccessKit announcements.
6. **Chrome nodes (P2, ~150 LOC).** Build AccessKit nodes for tab strip, address bar, find
   bar, status bar from `chrome.rs` layout data; add Tab traversal and activation.
7. **Reduced motion + forced colors (P2, ~110 LOC).** Add `prefers_reduced_motion: bool` and
   `forced_colors: bool` to `megi_css::MediaContext` (`css/src/media.rs:33`), populate in
   `render/src/lib.rs:826,1274` from shell flags/OS query, and honor in animations
   (`css/src/animation_runtime.rs`) and scroll animations.
8. **Scroll-into-view on focus, skip links, caret announcements (P3, ~80 LOC).**
9. **Contrast pass on chrome palette and focus indicators (P3, ~30 LOC)** — verify WCAG AA for
   `chrome.rs:47-89` (e.g. `COLOR_STATUS_TEXT` on `COLOR_STATUS_BAR` is fine; dropdown
   selected states need checking).

---

## C. Internationalization

| # | Item | Status | Evidence | Consequence | Rank |
|---|------|--------|----------|-------------|------|
| C1 | HTML `dir` attribute / `dir=auto` → `direction` | absent | `dir` is only a generic attribute name (`html/src/tree_builder.rs:124,164`); no mapping to CSS `direction` (`css/src/properties.rs:689-701` is CSS-only); no `:dir()` (`selector.rs:18`); layout uses computed `direction` only (`layout/src/inline.rs:418-458`) | `<html dir="rtl">`/`<div dir="rtl">` render LTR unless the page's CSS sets direction — RTL sites are broken by default | 1 |
| C2 | Bidirectional text (UAX #9) | partial | `layout/src/bidi.rs:1` "Pure-Rust Unicode Bidirectional Algorithm (UAX #9 subset)"; used at `layout/src/inline.rs:418-458,514-539` | Bidi runs, but RTL paragraphs/alignment have known gaps — tracked as #1447/#1448 (do not re-file) | 2 |
| C3 | `lang` attribute semantics | partial | normalized `xml:lang → lang` (`html/src/tree_builder.rs:415`), exposed to JS (`js/src/runtime.rs:6294`); but `:lang()` is rejected as an invalid selector (`css/src/selector.rs:18,284`) and nothing else consumes `lang` | `lang` has no styling/hyphenation/font effect; pages using `:lang(en)` fail to match | 3 |
| C4 | Locale-aware numbers/dates/collation (`Intl`) | absent | `rg 'Intl|toLocale|locale|NumberFormat|DateTimeFormat|Collator' crates/js/src` = 0 | `toLocaleString()` etc. fall back to C/English formatting; no localized numbers, dates, currency, collation | 4 |
| C5 | Timezone | absent | no `Intl`/timezone API in `crates/js/src`; `Date` has no zone-conversion surface | Scripts cannot format/convert time zones; users get host-local or UTC mismatch | 6 |
| C6 | Font fallback for CJK/emoji/RTL | present (caveats) | `fonts/src/db.rs:359` "per-glyph fallback can render CJK, Arabic, Hebrew, and emoji scripts"; generic families incl. "emoji" (`db.rs:30,650`); `load_system_fonts` (`db.rs:393`); colour fonts unsupported (`fonts/src/lib.rs:74`) | System fallback covers most scripts; colour emoji/CBDT/COLR fonts render monochrome or blank; known fonts issues #1228–#1239 | 7 |
| C7 | IME / dead keys / composition | absent | no `WindowEvent::Ime` handling (`window.rs:2071-2426` handles only `KeyboardInput`); text entry consumes `Key::Character` per keystroke (address bar `chrome.rs:178`, forms `window.rs:1592-1680`) | CJK (Chinese/Japanese/Korean) and composed/dead-key input is impossible; non-Latin users cannot type | 2 |
| C8 | Line breaking (UAX #14) incl. CJK | absent | `layout/src/inline.rs:288-299` only breaks after spaces/explicit hyphens and at soft hyphen U+00AD; no CJK ideographic/keep-all rules (`rg cjk|ideograph|UAX.*14` in layout = 0) | CJK paragraphs (no spaces) overflow or wrap at wrong boundaries; Thai/Khmer breakage too | 3 |
| C9 | Hyphenation (`hyphens: auto`, `lang`-aware) | partial | soft hyphen + `hyphens` breaks (`layout/src/inline.rs:741-1176`, 288-299); no dictionary/pattern hyphenation | No automatic hyphenation, especially bad for narrow columns/Germanic/Cyrillic text | 9 |
| C10 | `:lang()` selector | absent | explicitly "Still pending" (`css/src/selector.rs:18`); selectors using it are rejected as invalid (`selector.rs:284`) | Localized pages cannot style by language; styles fail closed | 5 |
| C11 | `writing-mode: vertical-rl` | absent | no `writing-mode` anywhere in `css`/`layout`/`paint`/`render` | Vertical Japanese/Chinese/Mongolian layouts unsupported | 11 |
| C12 | UI-string localization | absent | hardcoded mix: `title.rs:25` `" [recortado]"`, `download.rs:333` `"guardando… {name}"`, `pages.rs:285` `"Reintentar"`, `tabs.rs:34` `"New Tab"`, English elsewhere | Browser UI cannot be localized; strings are inconsistent within one product | 8 |
| C13 | `lang`/`dir` on generated pages | absent | generated HTML (`pages.rs:51-56, 68-73, 208, 278`) has `<html>` with no `lang`/`dir` | Screen readers/translation engines cannot detect language/direction of built-in pages | 10 |
| C14 | Form input methods / IME combos in fields | absent | same as C7 (`window.rs:1592-1680`; no composition events) | As C7 | 2 |

### C — missing → consequence → minimal implementation sketch

- **C1 `dir` (rank 1, ~90 LOC).** In the computed-style path (where presentational hints
  like `align`/`hidden` are applied, `css` cascade used by `layout`), when `direction` has no
  authored value, consult the nearest ancestor `dir` attribute (`ltr`/`rtl`/`auto`); for
  `auto`, inspect the first strong bidi character of the element's text using the existing
  bidi class helpers in `layout/src/bidi.rs`. Also add `:dir()` matching in `selector.rs`
  (~40 LOC more). Consequence if missing: every RTL site without explicit CSS renders LTR.
- **C7 IME (rank 2, ~200 LOC).** Handle `WindowEvent::Ime(Ime::Commit|Preedit|Disabled)`
  in `window.rs:2071`; maintain a preedit string for the focused editor (`forms.rs`), paint
  the preedit underline/candidate box (`chrome.rs` draw helpers), forward committed text via
  the existing `Key::Character` insertion path (`chrome.rs:178`, `window.rs:1592-1680`).
  Also enable winit `ime` support where the platform needs it.
- **C8 UAX #14 subset (rank 3, ~160 LOC).** Add a break-opportunity function in
  `layout/src/inline.rs` (used by `741-1176`): allow breaks between ideographs/kana/hangul
  and after CJK closing/opening punctuation per UAX #14 rules; keep `keep-all`-like behavior
  for `word-break: normal` non-CJK. Consequence: CJK/Thai text currently overflows its box.
- **C4/C5 Intl + timezone (rank 4/6).** New `crates/js/src/intl.rs` backed by ICU4X crates
  (`icu_decimal`, `icu_datetime`, `icu_collator`, `icu_timezone`), wired into the JS host as
  `Intl.NumberFormat/DateTimeFormat/Collator`, plus `toLocale*` methods on String/Number/Date
  (`js/src/runtime.rs` builtins). Locale negotiation from `navigator.language` (new: also
  absent) and `--lang` flag. ~400–600 LOC + dependency weight; alternative: a minimal
  en-US-only formatter (~150 LOC) if ICU is too heavy.
- **C10 `:lang()` (rank 5, ~120 LOC).** Implement `PseudoClass::Lang(String)` parsing
  (`selector.rs:18,284` currently rejects), matching `lang` on the element or nearest
  ancestor, case-insensitive with `*`-subtring rules per Selectors §6.6.
- **C9 hyphenation (rank 9, ~200 LOC).** In `layout/src/inline.rs` hyphenation pass, use
  TeX/Hunspell pattern data keyed by the element's `lang` (a `hypher`-style resource table
  for a few languages) to insert discretionary break points.
- **C11 writing-mode (rank 11, ~800 LOC).** Add `writing-mode`/`text-orientation` to
  `css/src/properties.rs`, thread through `layout` (inline axis selection, block flow) and
  `paint` (rotate glyph runs, vertical metrics). Low priority.
- **C12 UI strings (rank 8, ~150 LOC).** New `crates/browser/src/strings.rs` with a `Key` enum
  + `en`/`es` tables, `--lang` flag in `cli.rs`, replace the hardcoded strings above
  (`title.rs:25`, `download.rs:333`, `pages.rs:285`, `tabs.rs:34`, chrome menu labels in
  `chrome.rs:1105-1200`).
- **C13 generated-page metadata (rank 10, ~15 LOC).** Add `lang="en"` (or configured) and
  `dir` to the `<html>` tags in `pages.rs:51,68,208,278`.

---

## Cross-references / dedupe

- Shell feature bugs already filed: #1163–#1178; shell test gaps #1379.
- Font fallback and emoji limits: #1228–#1239 (fonts).
- Bidi correctness: #1447, #1448; do not re-file the `layout/src/bidi.rs` subset.
- Error-swallowing findings in shell (worker, session, window present, CLI screenshot
  warnings) were reported separately in the L5 lens review; not repeated here.
- F2 lens findings #1469–#1476 cover render/paint/net/svg; this document only adds the
  shell/a11y/i18n matrix.
