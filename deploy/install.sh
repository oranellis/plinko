#!/usr/bin/env bash
# Installs what build.sh made, as root: deploy/install.sh [prefix, default /opt/plinko]
# The server runs from the prefix as the plinko user, so each tenant's files are in
# <prefix>/data/<name>. Run it again to update, tenants are restarted with `systemctl restart`.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:-/opt/plinko}
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }
[ -x target/release/plinko-server ] && [ -d client/dist ] || { echo "run deploy/build.sh first" >&2; exit 1; }
id plinko >/dev/null 2>&1 || useradd --system --home-dir "$prefix" --shell /usr/sbin/nologin plinko
install -d "$prefix" "$prefix/client" /etc/plinko
install -d -o plinko -g plinko -m 700 "$prefix/data"
install -m 755 target/release/plinko-server "$prefix/plinko-server"
rm -rf "$prefix/client/dist"
cp -r client/dist "$prefix/client/dist"
sed "s#/opt/plinko#$prefix#g" deploy/plinko@.service > /etc/systemd/system/plinko@.service
systemctl daemon-reload
echo "installed in $prefix, add a tenant with deploy/new_tenant.sh <name> <port> <admin email>"
