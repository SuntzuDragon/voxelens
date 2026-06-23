#!/usr/bin/env bash
#
# Run the full CI check matrix locally, mirroring .github/workflows/ci.yml.
# Core checks (fmt, clippy, tests) always run; the optional-tool checks
# (wasm target, cargo-audit, cargo-llvm-cov) degrade gracefully with a hint.
#
# Usage:  ./scripts/ci-local.sh

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

step "fmt"
cargo fmt --all -- --check

step "clippy"
cargo clippy --all-targets --all-features -- -D warnings

step "tests"
if have cargo-nextest; then
    cargo nextest run --all
else
    cargo test --all
fi
cargo test --all --doc # doctests (nextest does not run these)

step "wasm32 build (core)"
if rustup target list --installed | grep -q wasm32-unknown-unknown; then
    cargo build -p voxelens-core --target wasm32-unknown-unknown
else
    echo "skipped — run: rustup target add wasm32-unknown-unknown"
fi

step "audit (RUSTSEC)"
if have cargo-audit; then
    cargo audit
else
    echo "skipped — run: cargo install --locked cargo-audit"
fi

step "coverage"
if have cargo-llvm-cov; then
    cargo llvm-cov --all --summary-only
else
    echo "skipped — run: cargo install cargo-llvm-cov"
fi

printf '\n\033[1;32mAll local CI checks complete.\033[0m\n'
