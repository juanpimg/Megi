# Security & Isolation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cerrar los explotos lógicos confirmados (T1/T2/T3/T5/T9), convertir el IPC en una frontera validada y añadir sandbox progresivo por plataforma sin romper `#![forbid(unsafe_code)]`.

**Architecture:** El browser privado posee cookies, grants de archivo y el broker IPC; el renderer recibe solo lo de su sitio y pide el resto. El aislamiento se refuerza en orden: lógica → IPC → sandbox Linux → network process → image/iframe. Cada fase tiene tests negativos automatizados (T1–T10 del spec).

**Tech Stack:** Rust 1.91, `serde`/`postcard` para el sobre IPC, PSL ya vendorizada en `crates/net/data/public_suffix_list.dat`, `landlock` + `rustix` para Linux, `bwrap` como fallback, fuzz con libFuzzer.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (§4.3, catálogo T1–T10, decisión de eTLD+1 y sandbox por plataforma).

## Global Constraints

- `#![forbid(unsafe_code)]` en los 17 crates `megi-*`; si hace falta un shim va en workspace separado tipo `fuzz/`.
- Semántica de sitio = **registrable domain (eTLD+1) con la PSL existente**; `SameSite` se aplica dentro del jar; `file:` por directorio canonicalizado, `data:` por documento heredero.
- `HttpOnly` nunca cruza al renderer; `cookies` se elimina del `IpcRenderResponse`.
- Los unsafe de dependencias se listan con `cargo-geiger`; **nunca** afirmar "0 unsafe en el binario".
- Sandbox real solo en Linux en 12 meses; macOS/Windows reportan `unsupported` de forma visible.
- Gate obligatorio tras cada fase: fmt, clippy `-D warnings`, tests workspace, `--no-default-features`, harness, `check_arch.py`, `check_docs_links.py`, `update_progress.py --check`, `audit_coverage.py --check --strict`, `cargo audit`, `cargo deny check`, `cargo geiger`.

## Review Focus

1. **Inyección de cookies desde el renderer** (T2) → test con `domain=bank.example` devuelto por renderer de `site-a.test` debe ser rechazado.
2. **Fuga de cookies HttpOnly de otro sitio** (T1) → test de sembrado filtrado; HttpOnly nunca aparece en `IpcRenderOptions`.
3. **Lectura de `/etc/passwd` vía symlink en FileGrant** (T3) → test con symlink que sale del grant y con `..`.
4. **Child heredando secretos del entorno** (T9) → test `env_clear` con variable `MEGI_SECRET` en el padre; no debe aparecer en `/proc/<pid>/environ`.
5. **Child colgado sin watchdog** (T5) → test con child que no responde ⇒ `Failed` en <6 s y `recoveries()==1`.

---

### Task 1: Corregir la documentación que miente

**Files:**
- Modify: `docs/ISOLATION-STRATEGY.md` (estado, FFI, modo por defecto, opacos)
- Modify: `docs/ARCHITECTURE.md` (matriz §3.1: rustix/landlock)
- Modify: `PROGRESS.md` y `docs/status/progress.toml` vía regeneración (28 targets de fuzz)

**Interfaces:**
- Consumes: lectura de `cli/args.rs`, `window/worker.rs`, `process_e2e_tests.rs`, `fuzz/fuzz_targets/`.
- Produces: documentación coherente con el código; `check_docs_links.py` limpio.

- [ ] **Step 1: Test que falla**

Test simple en `tools/test_docs_consistency.py`: leer `ISOLATION-STRATEGY.md` y assert que no contiene "modo por defecto" ni "no se puede hacer sandbox con forbid(unsafe_code)"; leer `PROGRESS.md` y assert que no contiene "14/16 targets".

- [ ] **Step 2: Verificar que falla**

Run: `python3 -m unittest tools.test_docs_consistency -v`
Expected: FAIL en los tres assertions.

- [ ] **Step 3: Corregir**

Reescribir los pasajes: `--ipc-render` es opt-in; `forbid(unsafe_code)` aplica a código propio, no a dependencias; orígenes opacos ya no se agrupan (se particionan en Task 4). Actualizar fuzz a 28 targets. Regenerar artefactos con `update_progress.py --write` (con gate, para no vaciar `verified_commit`).

- [ ] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_docs_consistency -v && python3 tools/update_progress.py --check --no-gate && python3 tools/check_docs_links.py`
Expected: tests OK, artefactos frescos, links OK.

- [ ] **Step 5: Commit**

```bash
git add docs/ISOLATION-STRATEGY.md docs/ARCHITECTURE.md tools/test_docs_consistency.py
git commit -m "docs(security): align isolation ADR with opt-in IPC and real fuzz count"
```

### Task 2: F0 — jar por sitio y eliminación de cookies del response

**Files:**
- Create: `crates/browser/src/ipc/cookie_policy.rs`
- Modify: `crates/browser/src/ipc/options.rs` (funciones `from_render_options`, `apply_to_jar`)
- Modify: `crates/browser/src/window/worker.rs` (donde se aplican las cookies del response)
- Test: `crates/browser/tests/ipc_semantic_tests.rs`

**Interfaces:**
- Consumes: `SiteKey` (`crates/browser/src/site.rs`), PSL (`crates/net` ya expone `registrable_domain`), `CookieJar`.
- Produces: `pub fn validate_cookie_from_renderer(cookie: &IpcCookie, site: &SiteKey) -> Result<(), CookiePolicyError>`, `pub fn seeded_cookies_for(site: &SiteKey, jar: &CookieJar) -> Vec<IpcCookie>`.

- [ ] **Step 1: Tests que fallan**

`ipc_semantic_tests.rs`: (a) `cookie_from_other_site_is_rejected`: renderer de `https://site-a.test`, cookie `domain=bank.example` ⇒ `Err`; (b) `httponly_never_seeded_to_renderer`: jar con cookie HttpOnly de `site-a.test` ⇒ `seeded_cookies_for` no la incluye; (c) `same_etld_plus_one_is_same_site`: `foo.example.co.uk` vs `bar.example.co.uk` ⇒ mismo sitio; (d) `different_registrable_domain_is_different_site`: `example.com` vs `example.org` ⇒ distinto.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-browser --test ipc_semantic_tests`
Expected: FAIL — `cookie_policy` no existe.

- [ ] **Step 3: Implementar `cookie_policy.rs`**

`CookiePolicyError` tipado (`DomainMismatch`, `HttpOnlyForbidden`, `InvalidDomain`). `validate_cookie_from_renderer` compara `registrable_domain(cookie.domain)` con `site.registrable_domain()` y rechaza `HttpOnly`/`Secure` marcados por el renderer. `seeded_cookies_for` filtra por sitio y excluye `HttpOnly`. Eliminar el campo `cookies` de `IpcRenderResponse` y su aplicación en `worker.rs`.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-browser --test ipc_semantic_tests`
Expected: 4 tests OK; ningún `IpcCookie` viaja de renderer a padre.

- [ ] **Step 5: Commit**

```bash
git add crates/browser/src/ipc/cookie_policy.rs crates/browser/src/ipc/options.rs crates/browser/src/window/worker.rs crates/browser/tests/ipc_semantic_tests.rs
git commit -m "fix(security): per-site cookie jar and reject renderer-supplied cookies"
```

### Task 3: F0 — spawn endurecido, watchdog y FileGrantTable

**Files:**
- Modify: `crates/browser/src/ipc/process.rs` (spawn)
- Modify: `crates/browser/src/ipc/process_pool.rs` (watchdog)
- Create: `crates/net/src/file_grant.rs`
- Test: `crates/browser/tests/watchdog_tests.rs`, `crates/net/tests/file_grant_tests.rs`

**Interfaces:**
- Consumes: `Command::env_clear`, `current_dir`, `process_group(0)` (`std::os::unix::process::CommandExt`, estable).
- Produces: `pub struct FileGrantTable { ... }`, `pub fn grant_for(dir: &Path) -> Result<FileGrant, GrantError>`, `pub fn resolve_within(grant: &FileGrant, path: &Path) -> Result<PathBuf, GrantError>`.

- [ ] **Step 1: Tests que fallan**

  - `watchdog_tests.rs`: `unresponsive_child_fails_within_six_seconds` — lanzar child que duerme 30 s ⇒ `WorkerMessage::Failed{generation,...}` en <6 s y `recoveries()==1`.
  - `file_grant_tests.rs`: `symlink_escape_is_denied`, `dotdot_is_denied`, `etc_passwd_is_denied`, `regular_file_within_grant_is_allowed`.
  - `process.rs`: test `env_is_cleared` — `MEGI_SECRET=leak` en el proceso de test; el child no lo ve.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-browser --test watchdog_tests && cargo test -p megi-net file_grant`
Expected: FAIL.

- [ ] **Step 3: Implementar**

`process.rs`: `env_clear()`, whitelist mínima (`RUST_BACKTRACE`, `PATH` al directorio del binario), `current_dir` a scratch, `process_group(0)`, `stderr(Stdio::piped())` con cap. `process_pool.rs`: deadline de 5 s por generación, hilo supervisor que `kill()` el grupo y publica `Failed`, `recoveries += 1`. `file_grant.rs`: canonicalize del directorio y del path, `starts_with`, rechazo de symlinks que salen, cap 64 MiB.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-browser --test watchdog_tests && cargo test -p megi-net file_grant && cargo test -p megi-browser --test process_e2e_tests`
Expected: todos OK; `<6 s` medido; E2E de PIDs/recovery intacto.

- [ ] **Step 5: Commit**

```bash
git add crates/browser/src/ipc crates/net/src/file_grant.rs crates/net/tests/file_grant_tests.rs crates/browser/tests/watchdog_tests.rs
git commit -m "fix(security): harden child spawn, add watchdog and file grant table"
```

### Task 4: F1 — IPC como frontera no confiable

**Files:**
- Modify: `crates/browser/src/ipc.rs` (sobre, cap, Validated)
- Create: `crates/browser/src/ipc/validated.rs`
- Modify: `crates/browser/tests/process_e2e_tests.rs` (quitar `#![cfg(feature = "a11y")]`)
- Create: `fuzz/fuzz_targets/{ipc_wire,ipc_response,net_file_url}.rs`
- Modify: `fuzz/Cargo.toml`, `.github/workflows/fuzz-nightly.yml`

**Interfaces:**
- Consumes: sobre actual `u32 BE + JSON` (`ipc.rs:24,58-117`).
- Produces: sobre `magic[4] | version:u16 | kind:u16 | request_id:u64 | site_token:[u8;32] | len:u32 | payload`; `Validated<T>` con `TryFrom<WireMessage>`; 31 targets de fuzz.

- [ ] **Step 1: Tests que fallan**

`crates/browser/tests/ipc_validation_tests.rs`: (a) `oversized_css_dimension_rejected` (9000 > 8192), (b) `stale_generation_rejected`, (c) `fake_png_bytes_rejected`, (d) `document_url_wrong_scheme_rejected`, (e) `unknown_field_rejected`.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-browser --test ipc_validation_tests`
Expected: FAIL.

- [ ] **Step 3: Implementar**

`validated.rs` con `#[serde(deny_unknown_fields)]` en todos los mensajes y `Validated<T>` que valida en el receptor: dimensiones ≤8192, generación ≤last+1, longitudes con caps, PNG magic y consistencia `w*h*4`, `document_url` ∈ {http,https,file,data} y coherente con el `site_token`. Bajar `MAX_IPC_PAYLOAD` de 256 MiB a 32 MiB para mensajes de control. Quitar el gate de feature de los E2E.

- [ ] **Step 4: Añadir fuzz targets**

Tres targets nuevos con semillas en `fuzz/corpus/{ipc_wire,ipc_response,net_file_url}/`; matriz ASan 300 s para los tres. Actualizar comentarios de `fuzz-nightly.yml` a 31 targets.

- [ ] **Step 5: Verificar**

```bash
cargo test -p megi-browser --test ipc_validation_tests
cargo test -p megi-browser --test process_e2e_tests
cd fuzz && cargo +nightly fuzz build && cargo +nightly fuzz run ipc_wire -- -max_total_time=60 -detect_leaks=0
```
Expected: tests OK; build 31/31; fuzz 60 s sin crash.

- [ ] **Step 6: Commit**

```bash
git add crates/browser/src/ipc.rs crates/browser/src/ipc crates/browser/tests fuzz
git commit -m "feat(security): versioned IPC boundary, semantic validation and 3 new fuzz targets"
```

### Task 5: F2 — Sandbox Linux

**Files:**
- Create: `crates/sandbox/Cargo.toml`, `crates/sandbox/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)
- Modify: `docs/ARCHITECTURE.md` (matriz de dependencias)
- Create: `crates/browser/tests/sandbox_e2e_tests.rs`
- Test: `crates/sandbox/src/lib.rs` tests + E2E

**Interfaces:**
- Consumes: `landlock` (API segura sobre rustix), `rustix`, `bwrap` como fallback.
- Produces: `pub enum SandboxMode { Enforced, BestEffort, Unsupported }`, `pub fn apply_renderer_sandbox(scratch: &Path, allow_network: bool) -> Result<SandboxMode, SandboxError>`.

- [ ] **Step 1: Tests negativos que fallan**

`sandbox_e2e_tests.rs` con 12 pruebas: read `/etc/passwd`, write en `$HOME`, `connect()`, `ptrace`, `exec`, socket AF_UNIX, etc. Todas deben fallar dentro del renderer sin tumbar el frame. Un test de humo verifica que el render正常 sigue funcionando.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-browser --test sandbox_e2e_tests`
Expected: FAIL — crate `megi-sandbox` no existe.

- [ ] **Step 3: Implementar**

`crates/sandbox/src/lib.rs` con `apply_renderer_sandbox`: `no_new_privs`, `PR_SET_DUMPABLE(0)`, Landlock FS (denegar todo, permitir scratch + exec propio), seccomp allowlist; si Landlock no está disponible, fallback a `bwrap` y devolver `BestEffort`; si nada, `Unsupported`. Sin `unsafe` en código propio. Añadir entrada en la matriz §3.1 y `cargo deny`/`geiger` allowlist documentado.

- [ ] **Step 4: Verificar**

```bash
cargo test -p megi-sandbox
cargo test -p megi-browser --test sandbox_e2e_tests
cargo geiger | grep -E 'megi-(sandbox|browser|core)'
cargo audit && cargo deny check
```
Expected: 13 tests OK; 0 unsafe en crates `megi-*`; audit/deny limpios.

- [ ] **Step 5: Commit**

```bash
git add crates/sandbox Cargo.toml docs/ARCHITECTURE.md crates/browser/tests/sandbox_e2e_tests.rs deny.toml
git commit -m "feat(security): Linux Landlock+seccomp sandbox with 12 negative tests"
```

### Task 6: F3 — Network process con token de sitio

**Files:**
- Create: `crates/net/src/bin/request_server.rs`
- Modify: `crates/browser/src/ipc.rs` (mensajes FetchRequest/FetchResponse)
- Create: `crates/browser/tests/network_broker_tests.rs`

**Interfaces:**
- Consumes: `ResourceLoader`, jar, caché, CORS, HSTS, PSL y proxies de `crates/net`.
- Produces: proceso `request-server`, token de sitio de 32 bytes emitido por el broker y validado en cada petición.

- [ ] **Step 1: Tests que fallan**

`network_broker_tests.rs`: con un `TcpListener` local, (a) renderer de `site-a` no puede pedir `https://site-b`; (b) no puede leer cookies de `site-b`; (c) petición legítima de `site-a` con token válido devuelve 200.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-browser --test network_broker_tests`
Expected: FAIL.

- [ ] **Step 3: Implementar**

Extraer el loader a binario con IPC propio; el renderer envía `FetchRequest{url, site_token, method, headers}`; el broker valida token contra `SiteRegistry`, aplica CORS/CSP/HSTS y es el único dueño del jar. El renderer ya no enlaza `megi-net` con sockets.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-browser --test network_broker_tests && cargo test --workspace`
Expected: 3 tests OK; workspace verde.

- [ ] **Step 5: Commit**

```bash
git add crates/net/src/bin crates/browser/src/ipc.rs crates/browser/tests/network_broker_tests.rs
git commit -m "feat(security): request-server process with per-site token"
```

### Task 7: F4 — `SiteInstance` por iframe y matriz de plataformas

**Files:**
- Modify: `crates/browser/src/site.rs` (SiteInstance)
- Modify: `crates/render/src/frames.rs`
- Create: `docs/status/security.html` (generado) y `tools/update_security_matrix.py`
- Modify: `crates/browser/src/window/frontend.rs` (degradación visible)

**Interfaces:**
- Consumes: `SiteKey`, `SiteRegistry`, SandboxMode.
- Produces: `pub struct SiteInstance { origin, frame_tree_id, opaque_inheritance }`; matriz por plataforma con `enforced|best-effort|unsupported`.

- [ ] **Step 1: Tests que fallan**

Test: dos iframes cross-site en la misma página ⇒ dos `SiteInstance` distintos; un iframe same-site ⇒ mismo instance. Test de matriz: `render_security_matrix()` produce `linux: enforced, macos: unsupported, windows: unsupported`.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-browser site_instance`
Expected: FAIL.

- [ ] **Step 3: Implementar**

Ampliar `site.rs`, enrutar cada iframe cross-site a su renderer (F3), publicar el estado del sandbox en `--json` y en el welcome page.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-browser site_instance && python3 tools/update_security_matrix.py --check && cargo run -q -p megi-browser -- --screenshot /tmp/x.png --json | grep sandbox`
Expected: tests OK; matriz fresca; JSON incluye `sandbox: {linux:"enforced",...}`.

- [ ] **Step 5: Commit**

```bash
git add crates/browser/src/site.rs crates/render/src/frames.rs docs/status/security.html tools/update_security_matrix.py
git commit -m "feat(security): site-instance per cross-site iframe and platform sandbox matrix"
```
