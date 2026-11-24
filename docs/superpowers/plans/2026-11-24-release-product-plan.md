# Release & Product Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hacer que Megi se pueda descargar, verificar y reportar como una alpha creíble: arreglar el release actual, añadir canales continuos, feedback con consentimiento y gobernanza mínima, sin backend propio ni formatos prematuros.

**Architecture:** `release.yml` pasa a exigir verificación antes de publicar y produce artefactos reproducibles firmados con cosign keyless. Canales `continuous`/`beta` publican artefactos inmutables sin auto-update. El feedback (crash y sitio roto) se entrega como borrador de GitHub Issue, con el crash guardado localmente. La gobernanza se documenta y se automatiza la higiene de ramas.

**Tech Stack:** GitHub Actions, `cosign` keyless (OIDC), `actions/attest-build-provenance`, `gh` CLI, `git-cliff` ya presente, `SOURCE_DATE_EPOCH` + tar reproducible.

> **v2 (sin calendarización)**: las referencias a Q1–Q4 de este plan son hitos lógicos (M0–M9 del master plan v2), no fechas. Las cifras WPT son pisos, no techos: las épocas continúan hasta cubrir el árbol completo. Las carreras antes llamadas 'anti-objetivos' (JIT, WebGL, vídeo, SW completo) están ahora en `js-evolution`, `graphics-media` y `web-platform-extension`.

**Spec:** `docs/superpowers/specs/2026-11-24-megi-ambition-design.md` (§4.4, decisiones del builder check sobre destino de reportes).

## Global Constraints

- Sin backend propio, sin retención de datos, sin dashboards en 12 meses.
- Crash report y sitio roto: **default OFF**, sin URL ni contenido de página sin consentimiento explícito; destino = borrador de GitHub Issue.
- Artefactos reproducibles: `SOURCE_DATE_EPOCH` del commit, `tar --sort=name --mtime --owner=0 --numeric-owner`, `gzip -n`.
- Sin auto-update in-place en la alpha.
- Flatpak y macOS/Windows firmado **solo con tracción medida** (regla de decisión a 3 meses del spec).
- Coste objetivo: 10–15 h/semana nuevas; 100–300 USD/año obligatorios.

## Review Focus

1. **Release publicado con CI roja** → job `verify` añadido a `needs` de `publish`.
2. **Dos builds del mismo tag producen bytes distintos** → test de doble build byte-idéntico en CI.
3. **Crash report filtra la URL visitada** → test con `MEGI_TEST_URL` en el entorno; el archivo de crash no la contiene.
4. **Canal `continuous` deja al usuario sin versión buena** → sin auto-update; verificación de checksum antes de cualquier swap; conservar versión anterior.
5. **Higiene de ramas borra trabajo ajeno** → `git_hygiene.sh` solo lista ramas mergeadas; requiere revisión humana antes de borrar; nunca `-D`.

---

### Task 1: Verificación previa y archivado reproducible

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `docs/RELEASE.md`
- Test: `tools/test_release_reproducible.py`

**Interfaces:**
- Consumes: `cargo run -p megi-test-harness -- --summary`, `cargo test --release --locked`.
- Produces: job `verify` en `needs` de `publish`; tarball byte-idéntico entre dos builds del mismo commit.

- [ ] **Step 1: Test que falla**

`tools/test_release_reproducible.py`: (a) `test_verify_job_is_required`: parsear `release.yml` y assert que `publish.needs` contiene `verify`; (b) `test_tar_command_uses_source_date_epoch`: el script de empaquetado contiene `--sort=name`, `--mtime`, `--owner=0` y `gzip -n`.

- [ ] **Step 2: Verificar que falla**

Run: `python3 -m unittest tools.test_release_reproducible -v`
Expected: FAIL en ambos assertions.

- [ ] **Step 3: Implementar**

Añadir job `verify` con checkout `fetch-depth: 0`, toolchain fijado, `test_harness --summary`, `cargo test --workspace --release --locked`, y comprobación `git describe --tags --exact-match == github.ref_name`. Cambiar `publish.needs` a `[build, sbom, verify]`. Sustituir el paso de empaquetado por el comando reproducible con `SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)`. Documentar verificación en `docs/RELEASE.md`.

- [ ] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_release_reproducible -v && python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"`
Expected: 2 tests OK; YAML válido.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml docs/RELEASE.md tools/test_release_reproducible.py
git commit -m "fix(release): require verification and produce reproducible tarballs"
```

### Task 2: Firma keyless y attestation

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `docs/RELEASE.md`
- Test: `tools/test_release_signing.py`

**Interfaces:**
- Consumes: OIDC `id-token: write` de GitHub Actions.
- Produces: `SHA256SUMS`, `SHA256SUMS.bundle`, `SHA256SUMS.sig`, attestation de provenance.

- [ ] **Step 1: Test que falla**

`test_signing_steps_present`: el workflow contiene `cosign sign-blob`, `actions/attest-build-provenance` y permisos `id-token: write` / `attestations: write`.

- [ ] **Step 2: Verificar que falla**

Run: `python3 -m unittest tools.test_release_signing -v`
Expected: FAIL.

- [ ] **Step 3: Implementar**

Añadir permisos y pasos: `sigstore/cosign-installer@v3`, `cosign sign-blob --yes --bundle ... --output-signature ... SHA256SUMS`, `actions/attest-build-provenance@v2` con `subject-path` de tarballs/zips/SHA256SUMS. Documentar comandos de verificación para usuarios (`sha256sum -c`, `cosign verify-blob`, `gh attestation verify`) con `certificate-identity` fijado tras verificarlo una vez.

- [ ] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_release_signing -v`
Expected: OK; `docs/RELEASE.md` contiene los tres comandos.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml docs/RELEASE.md tools/test_release_signing.py
git commit -m "feat(release): keyless cosign signature and build provenance"
```

### Task 3: Canales continuous y beta

**Files:**
- Create: `.github/workflows/continuous.yml`, `.github/workflows/weekly-beta.yml`
- Create: `docs/CHANNELS.md`, `docs/status/releases.json` (generado)
- Modify: `.github/workflows/ci.yml` (job `artifact-smoke`)
- Test: `tools/test_channels.py`

**Interfaces:**
- Consumes: artefactos de `release.yml`/`continuous.yml`.
- Produces: pre-release mutable `continuous`, tag `beta`, `latest.json`, y job `artifact-smoke` que ejecuta el artefacto descargado.

- [ ] **Step 1: Test que falla**

(a) `test_continuous_has_no_auto_update`: el workflow no contiene `--auto-update` ni lógica de swap; (b) `test_beta_requires_five_of_seven`: el job beta comprueba 5/7 daily verdes y 0 issues `alpha-blocker`; (c) `test_artifact_smoke_runs_downloaded_binary`: `ci.yml` contiene descarga del tarball y ejecución `./megi --version`.

- [ ] **Step 2: Verificar que fallan**

Run: `python3 -m unittest tools.test_channels -v`
Expected: FAIL.

- [ ] **Step 3: Implementar**

`continuous.yml`: schedule diario 03:17 UTC, `concurrency` sin cancelación, build con `SOURCE_DATE_EPOCH`, `gh release upload continuous dist/* --clobber`, `latest.json` con `{channel, commit, date, sha256}`. `weekly-beta.yml`: domingo 04:17 UTC, comprueba 5/7 daily y 0 `alpha-blocker`, publica tag `beta`. `docs/CHANNELS.md` explica `stable`/`beta`/`continuous` y la regla sin auto-update. Job `artifact-smoke` en CI.

- [ ] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_channels -v && python3 -c "import yaml,glob; [yaml.safe_load(open(f)) for f in glob.glob('.github/workflows/*.yml')]; print('YAML OK')"`
Expected: 3 tests OK; YAML OK.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows docs/CHANNELS.md tools/test_channels.py
git commit -m "feat(release): continuous and beta channels without auto-update"
```

### Task 4: Crash report y sitio roto con consentimiento

**Files:**
- Create: `.github/ISSUE_TEMPLATE/crash_report.yml`, `.github/ISSUE_TEMPLATE/broken_site.yml`
- Modify: `crates/browser/src/main.rs` o el punto de arranque (panic hook)
- Modify: `crates/browser/src/chrome.rs` (menú Ayuda)
- Create: `tools/report_site.sh`
- Test: `crates/browser/tests/telemetry_privacy_tests.rs`

**Interfaces:**
- Consumes: `std::panic::set_hook`, `render --json` (`RenderStats`), `real_sites.toml`.
- Produces: archivo local en `~/.local/state/megi/crashes/` y URL de borrador de issue; nunca envío automático.

- [ ] **Step 1: Tests que fallan**

(a) `crash_file_has_no_url`: con `MEGI_TEST_URL=https://secret.example` en el entorno, el archivo de crash contiene `version`, `commit`, `target_triple`, OS/arch, mensaje y stack, pero **no** `secret.example`; (b) `report_draft_has_required_fields`: la URL generada contiene `template=broken_site.yml`, `commit`, `display_items` y hasta 5 warnings; (c) `telemetry_default_off`: la config por defecto tiene `crash_reports = false`.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-browser --test telemetry_privacy_tests`
Expected: FAIL.

- [ ] **Step 3: Implementar**

Hook de panic que guarda el archivo local con allowlist de campos (nunca URL, cookies, rutas de usuario ni argumentos CLI). Diálogo de primer arranque con checkbox sin marcar y "No enviar" tan visible como "Aceptar". Menú `Ayuda → Reportar sitio roto…` que recoge URL editable, commit, `display_items` y warnings, y abre el borrador de issue. `tools/report_site.sh` envuelve `render --json`.

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-browser --test telemetry_privacy_tests && bash -n tools/report_site.sh`
Expected: 3 tests OK; script válido.

- [ ] **Step 5: Commit**

```bash
git add .github/ISSUE_TEMPLATE crates/browser/src crates/browser/tests tools/report_site.sh
git commit -m "feat(product): consent-based crash reports and broken-site issues"
```

### Task 5: Welcome page y release notes honestas

**Files:**
- Create: `crates/browser/src/welcome.rs`
- Modify: `.github/workflows/release.yml` (paso `Known issues`)
- Modify: `docs/RELEASE.md`

**Interfaces:**
- Consumes: `docs/status/releases.json`, issues con etiqueta `alpha-blocker`,特征 declaradas y no soportadas del spec.
- Produces: `about:megi` renderizado por el motor; sección "Known issues" en cada release.

- [ ] **Step 1: Tests que fallan**

(a) `welcome_lists_supported_and_unsupported`: el HTML generado contiene "Boa", "html5lib" y también "sin JIT", "sin WebGL", "sin vídeo"; (b) `release_notes_include_alpha_blockers`: el comando de generación añade la salida de `gh issue list --label alpha-blocker`.

- [ ] **Step 2: Verificar que fallan**

Run: `cargo test -p megi-browser welcome`
Expected: FAIL.

- [ ] **Step 3: Implementar**

`welcome.rs` genera la página desde datos embebidos en el binario (dogfooding del motor). Marcador `~/.local/state/megi/first-run` para mostrarla solo la primera vez. En `release.yml`, antes de `gh release create`, añadir los `alpha-blocker` abiertos a `CHANGES.md` bajo "Known issues".

- [ ] **Step 4: Verificar**

Run: `cargo test -p megi-browser welcome && python3 -m unittest tools.test_release_signing -v`
Expected: tests OK.

- [ ] **Step 5: Commit**

```bash
git add crates/browser/src/welcome.rs .github/workflows/release.yml docs/RELEASE.md
git commit -m "feat(product): about:megi welcome page and honest release notes"
```

### Task 6: Gobernanza y higiene de ramas

**Files:**
- Create: `docs/GOVERNANCE.md`, `tools/git_hygiene.sh`
- Modify: `.github/CODEOWNERS`, `.github/workflows/community.yml`
- Create: `.github/labels.yml`
- Test: `tools/test_git_hygiene.py`

**Interfaces:**
- Consumes: ramas locales y worktrees actuales (95 ramas, 17 worktrees tras la limpieza previa).
- Produces: política BDFL + lazy consensus 72 h; script que **solo lista** ramas mergeadas; labels declarativos; workflow de comunidad.

- [ ] **Step 1: Test que falla**

(a) `test_hygiene_never_forces`: el script no contiene `git branch -D` ni `worktree remove --force` sin confirmación; (b) `test_governance_declares_sla`: `GOVERNANCE.md` contiene "72 h" y "BDFL"; (c) `test_labels_has_alpha_blocker`: `labels.yml` incluye `alpha-blocker`, `site-broken`, `crash`, `wpt`.

- [ ] **Step 2: Verificar que fallan**

Run: `python3 -m unittest tools.test_git_hygiene -v`
Expected: FAIL.

- [ ] **Step 3: Implementar**

`GOVERNANCE.md`: BDFL `@juanpimg`, lazy consensus 72 h (7 días para arquitectura), SLA de triaje 72 h, `alpha-blocker`/seguridad el mismo día, sin fundación antes de 6–12 meses ni 50k USD/año. `git_hygiene.sh` con `git worktree prune`, listado de ramas mergeadas y `for-each-ref --sort=-committerdate | head -20`; **sin borrado automático**. CODEOWNERS por área con regla "owner inactivo 30 días ⇒ vuelve a `*`". `labels.yml` con los labels del spec y workflow de sincronización. `community.yml` copiando la receta ya documentada en `CONTRIBUTOR-ONBOARDING.md` (pull_request_target sin checkout).

- [ ] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_git_hygiene -v && bash -n tools/git_hygiene.sh && python3 -c "import yaml; yaml.safe_load(open('.github/labels.yml'))"`
Expected: 3 tests OK; scripts y YAML válidos.

- [ ] **Step 5: Commit**

```bash
git add docs/GOVERNANCE.md tools/git_hygiene.sh .github tools/test_git_hygiene.py
git commit -m "docs(governance): BDFL policy, declarative labels and safe git hygiene"
```

### Task 7: Formatos Linux y puerta de tracción

**Files:**
- Create: `.github/workflows/package-linux.yml`
- Create: `build-aux/megi.desktop`, `build-aux/megi.png`, `build-aux/AppRun`
- Create: `build-aux/com.github.juanpimg.Megi.yml` (manifiesto Flatpak, **no publicado** aún)
- Test: `tools/test_package_linux.py`

**Interfaces:**
- Consumes: binario release de `release.yml`.
- Produces: AppImage, `.deb`, `.rpm` como artefactos adjuntos al Release; manifiesto Flatpak listo pero sin envío a Flathub.

- [ ] **Step 1: Test que falla**

(a) `test_deb_rpm_are_artifacts_not_repos`: el workflow usa `cargo deb`/`cargo generate-rpm` y `gh release upload`, sin `apt`/`dnf` repo; (b) `test_flatpak_not_submitted`: el manifiesto existe pero ningún workflow ejecuta `flatpak-builder --repo` ni `flathub` push; (c) `test_gpu_permission_absent`: `finish-args` no contiene `--device=dri` (Megi es CPU).

- [ ] **Step 2: Verificar que fallan**

Run: `python3 -m unittest tools.test_package_linux -v`
Expected: FAIL.

- [ ] **Step 3: Implementar**

`package-linux.yml` dominical: AppImage con `appimagetool`, `.deb` y `.rpm` con `cargo deb`/`cargo generate-rpm`, `gh release upload "$TAG" dist/*.AppImage dist/*.deb dist/*.rpm --clobber`. Manifiesto Flatpak con runtime `org.freedesktop.Platform 24.08`, `finish-args: [--share=network, --socket=x11, --filesystem=xdg-config/megi:create]`, `build-system: cargo` con `cargo vendor` + `CARGO_NET_OFFLINE=true`. Añadir a `docs/RELEASE.md` la regla de decisión: si a 3 meses hay <100 descargas/mes, no enviar a Flathub ni notarizar macOS.

- [ ] **Step 4: Verificar**

Run: `python3 -m unittest tools.test_package_linux -v && python3 -c "import yaml; yaml.safe_load(open('.github/workflows/package-linux.yml'))"`
Expected: 3 tests OK; YAML válido.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/package-linux.yml build-aux docs/RELEASE.md tools/test_package_linux.py
git commit -m "feat(packaging): AppImage and deb/rpm artifacts with Flatpak gate"
```
