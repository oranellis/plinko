#!/usr/bin/env bash
# Builds the release client and server: deploy/build.sh. install.sh puts them in place.
set -euo pipefail
cd "$(dirname "$0")/.."
./client/build.sh --release
cargo build --release -p plinko
