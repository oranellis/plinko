#!/usr/bin/env bash
# Times how the client copes with a lot of work: `./client/e2e/perf.sh [--release]`. Uses the
# tenant saved by `PLINKO_BENCH_SAVE=1 cargo run --release -p plinko --example bench` (data/Bench).
set -euo pipefail
cd "$(dirname "$0")/../.."
root=$PWD
[ -d data/Bench ] || { echo "no data/Bench, see docs/development.md"; exit 1; }
./client/build.sh "$@"
cargo build -p plinko
(cd client/e2e && npm install --no-audit --no-fund --silent)
dir=$(mktemp -d)
trap 'kill $server 2>/dev/null || true; rm -rf "$dir"' EXIT
mkdir "$dir/data" && cp -r data/Bench "$dir/data/Bench"
cd "$dir"
PLINKO_TENANT=Bench PLINKO_INSECURE_COOKIES=1 PLINKO_LISTEN=127.0.0.1:3412 \
    PLINKO_FILES="$root/client/dist" "$root/target/debug/plinko-server" &
server=$!
sleep 2
CHROMIUM=${CHROMIUM:-$(command -v chromium)} node "$root/client/e2e/perf.mjs"
