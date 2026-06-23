# voxelens

[![CI](https://github.com/SuntzuDragon/voxelens/actions/workflows/ci.yml/badge.svg)](https://github.com/SuntzuDragon/voxelens/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Reconstruct a Minecraft schematic from a screenshot.

`voxelens` detects individual block faces in a screenshot, classifies each face
against the known Minecraft texture set, and back-projects them onto the voxel
grid to rebuild the structure as a Sponge `.schem` file. The goal is to automate
the last manual step in screenshot/panorama-based seed reverse-engineering.

> **Status:** early development. See [`docs/ROADMAP.md`](docs/ROADMAP.md) for the
> full plan, design decisions, and milestone breakdown.

## Workspace layout

```
crates/core   Pure, WASM-compatible engine (all algorithms; no I/O). Testable in Node-free Rust.
crates/cli    Native CLI: screenshot in -> .schem out, plus per-stage debug PNGs.
fixtures/     Committed test screenshots + golden outputs, documented in manifest.toml.
textures/     Block textures (gitignored; extracted from a local MC install). stand-in/ holds CI tiles.
docs/         ROADMAP and design notes.
```

The engine is built and tested natively first (Rust core + CLI); a WebAssembly
wrapper and a browser front-end (deployed free on Cloudflare Pages) come later.

## Development

```sh
cargo test --all      # run the suite (test-first: write the failing test, then fix)
cargo fmt --all       # format
cargo clippy --all-targets --all-features -- -D warnings   # lint (CI-enforced)
```

### Git hooks

A committed pre-commit hook (`.githooks/pre-commit`) runs the same gates as CI
— `fmt --check`, `clippy -D warnings`, and `cargo test` — and is skipped for
commits that touch no Rust/build files. Enable it once per clone:

```sh
git config core.hooksPath .githooks
```

Bypass in an emergency with `git commit --no-verify`.

## License

MIT OR Apache-2.0. Mojang textures are **not** redistributed; they are loaded
from a local Minecraft install at runtime.
