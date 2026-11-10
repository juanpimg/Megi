# Security Policy

Megi is a from-scratch web engine: it parses and renders untrusted input by
design, so memory safety and denial-of-service resistance are treated as
correctness requirements, not features.

## Supported versions

The project is pre-1.0 and only the `main` branch is supported. Fixes are
landed directly on `main`; there are no maintained release branches yet.

## Reporting a vulnerability

Please report suspected vulnerabilities **privately**:

1. Open a private security advisory on GitHub
   (`Security` → `Advisories` → `New draft security advisory`) for
   `juanpimg/Megi`, or
2. Email the maintainer listed in the repository profile.

Do not open a public issue for a security report. Include:

- affected commit / version and platform,
- minimal input that triggers the issue (HTML/CSS/patch),
- observed behavior (crash, abort, hang, excessive CPU/memory) and expected
  behavior,
- whether the input is attacker-controlled in a realistic setting.

We aim to acknowledge reports within 7 days and to coordinate disclosure
after a fix is available.

## Security model and guarantees

- **Memory safety**: the engine crates (`dom`, `html`, `css`, `layout`,
  `paint`, `fonts`) contain no `unsafe` code and are built with
  `clippy -D warnings`. The dependency policy in
  [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) §3 restricts external crates
  to low-level, audited building blocks.
- **Hostile input**: parsing is bounded by `html::ParseOptions`
  (nesting depth, node count, attribute count and input size). Limits are
  reported through `LimitViolations` instead of aborting or panicking.
  Recursive post-processing and serialization are iterative so deeply nested
  documents cannot overflow the stack.
- **No unsound abstractions**: tree mutations are checked in release builds
  (`append_child` / `insert_before` reject self-insertion and ancestor
  cycles), and detached references never panic.

## Known robustness work in progress

- Remaining non-linear CPU paths (HTML tree construction, CSS selector
  matching) are tracked in [`docs/HANDOFF.md`](docs/HANDOFF.md) and measured
  by the benchmark suite; see [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md).
- Fuzzing targets are not wired into CI yet.
- There is no JavaScript engine yet (Phase 6); sandboxing and JIT hardening
  will be designed as part of that phase.

## Out of scope

- Vulnerabilities in third-party dependencies that are fixed upstream and do
  not affect Megi's usage (report those upstream, but let us know).
- Denial of service that requires attacker control over the host machine or
  the Rust toolchain itself.
- Issues that only affect rendering fidelity without a security impact.
