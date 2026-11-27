#!/usr/bin/env bash
# Checks formatting, lints, tests, and checks the shared crate still builds for the browser
set -euo pipefail
cd "$(dirname "$0")"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p plinko-client --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace
cargo build -p plinko --lib --no-default-features --target wasm32-unknown-unknown
cargo build -p plinko-client --target wasm32-unknown-unknown
