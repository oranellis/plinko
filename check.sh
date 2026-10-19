#!/usr/bin/env bash
# Lints, tests, and checks the shared crate still builds for the browser
set -euo pipefail
cd "$(dirname "$0")"
cargo clippy --workspace --all-targets
cargo test --workspace
cargo build -p plinko --lib --no-default-features --target wasm32-unknown-unknown
cargo build -p plinko-client --target wasm32-unknown-unknown
