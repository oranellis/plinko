#!/usr/bin/env bash
# Builds the client and runs a tenant locally over plain HTTP: deploy/run_debug.sh
# Settings can be overridden from the environment. The admin login is only used the first time,
# when the database does not exist yet. Open http://127.0.0.1:3000 afterwards.
set -euo pipefail
cd "$(dirname "$0")/.."
./client/build.sh
export PLINKO_DB=${PLINKO_DB:-tenant.db}
export PLINKO_TENANT=${PLINKO_TENANT:-Debug}
export PLINKO_ADMIN_EMAIL=${PLINKO_ADMIN_EMAIL:-admin@example.com}
export PLINKO_ADMIN_PASSWORD=${PLINKO_ADMIN_PASSWORD:-change-me-now}
export PLINKO_INSECURE_COOKIES=1
exec cargo run -p plinko
