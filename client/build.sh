#!/usr/bin/env bash
# Builds the client into client/dist, which the server serves. Needs the wasm32-unknown-unknown
# target and wasm-bindgen-cli. Pass --release for an optimised build.
set -euo pipefail
cd "$(dirname "$0")/.."
profile=debug
flag=()
if [ "${1:-}" = "--release" ]; then profile=release; flag=(--release); fi
cargo build -p plinko-client --target wasm32-unknown-unknown "${flag[@]}"
rm -rf client/dist && mkdir -p client/dist
wasm-bindgen --target web --no-typescript --out-dir client/dist \
    "target/wasm32-unknown-unknown/$profile/plinko_client.wasm"
cp client/static/* client/dist/
