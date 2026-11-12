# Getting Started with Megi

This guide helps you build, test, and run Megi locally.

## Prerequisites

- **Rust** 1.88+ (Edition 2024). Install via [rustup](https://rustup.rs/):
  ```bash
  rustup toolchain install stable
  rustup default stable
  rustc --version
  ```
- **Git** for cloning the repository.
- **Linux (for the graphical shell)**: `winit`/`softbuffer` require X11/Wayland development packages. On Debian/Ubuntu:
  ```bash
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends \
    pkg-config libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev \
    libx11-dev libx11-xcb-dev libxcursor-dev libxi-dev libxrandr-dev \
    libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
    libxcb-shm0-dev libxcb-randr0-dev
  ```
  The rest of the workspace (parsers, layout, paint, net, image, js, svg, render, tests) can be built/tested without these system libraries by excluding `shell` where needed.

## Clone

```bash
git clone https://github.com/juanpimg/Megi.git
cd Megi
```

## Build

```bash
# Check the full workspace
cargo check --workspace

# Build release binaries (recommended for rendering/shell)
cargo build --release -p render -p shell -p test_harness
```

## Run the browser shell

```bash
# Open a URL or local HTML file in the desktop window
cargo run --release -p shell -- https://example.com
cargo run --release -p shell -- tests/fixtures/render/showcase/index.html

# Headless screenshot (no window server required)
cargo run --release -p shell -- https://example.com --screenshot output.png --width 1024 --height 768
```

## Render HTML to PNG (pipeline)

```bash
cargo run --release -p render -- tests/fixtures/render/basic.html /tmp/render.png --width 800 --height 600
cargo run --release -p render -- tests/fixtures/render/basic.html /tmp/render.png --json
```

## Test

```bash
# All workspace tests
cargo test --workspace

# Format, lint, tests, docs and conformance gate (recommended before commits)
./tools/dev.sh all

# Conformance gate only
cargo run -p test_harness -- --summary
cargo run -p test_harness -- --summary --strict

# JS-free builds (minimal engine/renderer)
cargo test -p js --no-default-features
cargo test -p render --no-default-features
```

## Performance and fuzzing

```bash
# Benchmarks (release; baseline in docs/PERFORMANCE.md)
./tools/bench.sh --scale 0.25

# Fuzzing (requires nightly + cargo-fuzz)
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
cd fuzz && cargo +nightly fuzz build
```

## Useful documentation

- [README.md](../README.md) — project overview
- [docs/ARCHITECTURE.md](ARCHITECTURE.md) — crate layout and design
- [docs/ROADMAP.md](ROADMAP.md) — phases and work packages
- [CONTRIBUTING.md](../CONTRIBUTING.md) — contribution workflow, commits, DCO, PR process
- [SECURITY.md](../SECURITY.md) — security model and vulnerability reporting
- [docs/FUZZING.md](FUZZING.md) — fuzz targets and triage
- [docs/PERFORMANCE.md](PERFORMANCE.md) — benchmarks and budgets
- [PROGRESS.md](../PROGRESS.md) — current metrics and conformance status
