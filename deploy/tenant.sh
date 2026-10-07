#!/usr/bin/env bash
# Runs the server for one tenant: deploy/tenant.sh <name> <port>
# Data lives in data/<name>/tenant.db. The first run of a tenant also needs PLINKO_TENANT,
# PLINKO_ADMIN_EMAIL and PLINKO_ADMIN_PASSWORD in the environment. Build first with
# `cargo build --release -p plinko` and `client/build.sh --release`.
set -euo pipefail
cd "$(dirname "$0")/.."
name=${1:?usage: tenant.sh <name> <port>}
port=${2:?usage: tenant.sh <name> <port>}
mkdir -p "data/$name"
export PLINKO_DB="data/$name/tenant.db"
export PLINKO_LISTEN="127.0.0.1:$port"
export PLINKO_FILES=client/dist
exec target/release/plinko-server
