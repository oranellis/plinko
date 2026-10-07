#!/usr/bin/env bash
# Runs the browser checks against a fresh tenant. Needs node and a chromium (CHROMIUM=path if it
# is not on the PATH as `chromium`).
set -euo pipefail
cd "$(dirname "$0")/../.."
./client/build.sh
cargo build -p plinko
(cd client/e2e && npm install --no-audit --no-fund --silent)
dir=$(mktemp -d)
trap 'kill $server 2>/dev/null || true; rm -rf "$dir"' EXIT
PLINKO_DB="$dir/tenant.db" PLINKO_TENANT=Acme PLINKO_ADMIN_EMAIL=admin@example.com \
    PLINKO_ADMIN_PASSWORD=password123 PLINKO_INSECURE_COOKIES=1 PLINKO_LISTEN=127.0.0.1:3411 \
    PLINKO_FILES=client/dist target/debug/plinko-server &
server=$!
sleep 1
CHROMIUM=${CHROMIUM:-$(command -v chromium)} node client/e2e/flows.mjs
