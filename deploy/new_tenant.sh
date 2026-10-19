#!/usr/bin/env bash
# Starts a tenant, as root: deploy/new_tenant.sh <name> <port> <admin email> [prefix]
# The name is letters, digits, - and _. Prints the admin password it made, which is the only
# time it is shown. Point a proxy at the port, see deploy/nginx.conf.example.
set -euo pipefail
name=${1:?usage: new_tenant.sh <name> <port> <admin email> [prefix]}
port=${2:?usage: new_tenant.sh <name> <port> <admin email> [prefix]}
email=${3:?usage: new_tenant.sh <name> <port> <admin email> [prefix]}
prefix=${4:-/opt/plinko}
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 1; }
[[ $name =~ ^[A-Za-z0-9_-]{1,64}$ ]] || { echo "bad name" >&2; exit 1; }
[ ! -e "/etc/plinko/$name.env" ] || { echo "$name already exists" >&2; exit 1; }
password=$(head -c 18 /dev/urandom | base64 | tr '+/' '-_')
umask 077
cat > "/etc/plinko/$name.env" <<ENV
PLINKO_LISTEN=127.0.0.1:$port
PLINKO_ADMIN_EMAIL=$email
PLINKO_ADMIN_PASSWORD=$password
ENV
systemctl enable --now "plinko@$name"
# the admin login is only read when the tenant is made, so it need not stay on disk
for _ in $(seq 20); do
    [ -f "$prefix/data/$name/tenant.db" ] && break
    sleep 0.5
done
sed -i '/^PLINKO_ADMIN_PASSWORD=/d' "/etc/plinko/$name.env"
echo "tenant $name on port $port, log in as $email with password $password"
