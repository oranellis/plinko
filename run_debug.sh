#!/usr/bin/env bash
# Builds the debug client and server and runs a tenant locally over plain HTTP, then open
# http://127.0.0.1:3000. Its files are in data/Debug. The admin login is only used the first time,
# when that folder is empty. Settings can be overridden from the environment, see docs/development.md.
set -euo pipefail
cd "$(dirname "$0")"
./client/build.sh
cargo build -p plinko
export PLINKO_TENANT=${PLINKO_TENANT:-Debug}
export PLINKO_ADMIN_EMAIL=${PLINKO_ADMIN_EMAIL:-"admin@example.com"}
export PLINKO_ADMIN_PASSWORD=${PLINKO_ADMIN_PASSWORD:-"change-me-now"}
export PLINKO_INSECURE_COOKIES=1
exec target/debug/plinko-server
