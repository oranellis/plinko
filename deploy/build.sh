#!/usr/bin/env bash
# Builds the release client and server: deploy/build.sh. install.sh puts them in place.
set -euo pipefail
cd "$(dirname "$0")/.."
# a release is only built from code that is formatted and lint free
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p plinko-client --target wasm32-unknown-unknown -- -D warnings
./client/build.sh --release
cargo build --release -p plinko
