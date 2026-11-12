# Release process

This document describes how Megi releases are versioned and published.

## Versioning

- Megi follows **Semantic Versioning** in the `0.x.y` pre-1.0 range:
  - `0.x.0`: breaking changes or major feature milestones.
  - `0.x.y` (`y > 0`): fixes, hardening, and backward-compatible improvements.
- The canonical workspace version lives in `[workspace.package].version` in [`Cargo.toml`](../Cargo.toml).
  Crate `version` fields and path-dependency versions must stay in sync with it.

## Release checklist

1. **Prepare the tree**
   - Ensure `main` is green in CI (fmt, clippy, tests, conformance gate, security/audit jobs).
   - Update [`CHANGELOG.md`](../CHANGELOG.md): move relevant "Unreleased"/round notes into a new `## [x.y.z] - YYYY-MM-DD` section with Added/Changed/Fixed/Performance/Security as applicable.
   - Update [`docs/status/progress.toml`](status/progress.toml) and generated status artifacts if the release reflects a documented snapshot.
   - Verify docs that advertise counts/budgets still match reality (`README.md`, `docs/HANDOFF.md`, `docs/PERFORMANCE.md` as applicable).
2. **Version bump**
   - Bump `[workspace.package].version` (and crate/dependency versions kept in lockstep).
   - Run `cargo check --workspace --locked` and `cargo test --workspace --locked` (or the repo's required subset) to confirm the lockfile and manifests are consistent.
   - Commit with a conventional message, e.g. `chore(release): prepare v0.x.y`.
3. **Tag and publish**
   - Create an annotated tag: `git tag -a v0.x.y -m "Megi v0.x.y"`.
   - Push the commit and tag: `git push origin main && git push origin v0.x.y`.
   - The [Release workflow](../.github/workflows/release.yml) builds binaries, generates checksums and a CycloneDX SBOM, and publishes/updates the GitHub Release for the tag.
4. **Post-release**
   - Verify release assets: platform archives, `.sha256` files, and SBOM artifacts.
   - If needed, create a `release/v0.x` maintenance branch from the tag for backports.
   - Bump to the next development version only if the project adopts that convention (optional).

## Artifacts

Each tagged release publishes:
- Binary archives for Linux (`x86_64-unknown-linux-gnu`), macOS (`aarch64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`) containing `shell`, `render`, `test_harness`, and `bench` where built.
- SHA-256 checksum files (`*.sha256`) for each archive.
- A CycloneDX SBOM (`cargo-cyclonedx`) describing workspace dependencies.

## Notes

- Pre-1.0 APIs and on-disk formats may change between minor versions; see `CHANGELOG.md` for migration notes.
- Security fixes follow the disclosure process in [`SECURITY.md`](../SECURITY.md).
