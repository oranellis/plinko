# Release

One process per tenant, each run by systemd and fronted by a proxy that ends TLS. Everything is
under a prefix, `/opt/plinko` by default, and each tenant keeps its files in
`/opt/plinko/data/<name>`.

## Steps

1. `deploy/build.sh` builds the release client and server (the client is much smaller if
   `wasm-opt` is installed, and the proxy should gzip it, see the nginx example).
2. `sudo deploy/install.sh [prefix]` installs them, makes the `plinko` user and the systemd
   template `plinko@.service`. Run it again to update, then `systemctl restart 'plinko@*'`.
3. `sudo deploy/new_tenant.sh <name> <port> <admin email>` makes a tenant, starts it, and prints
   the admin's password once. Names are letters, digits, `-` and `_`.
4. Point the proxy at the port, one server block per tenant, see `deploy/nginx.conf.example`.
   It must pass `Host` on, because the server only accepts requests whose `Origin` matches it.

## What is where

- `/opt/plinko/plinko-server` and `/opt/plinko/client/dist` the program and the client.
- `/opt/plinko/data/<name>/tenant.db` a tenant's log, snapshots, schedules, passwords and sessions.
- `/etc/plinko/<name>.env` its port, and the first admin until the tenant exists.
- Logs are in the journal: `journalctl -u plinko@<name>`.

## Backups

The log is append only, so a copy of `tenant.db` (with its `-wal` file) is a backup of every
change. For a consistent copy of a running tenant use `sqlite3 data/<name>/tenant.db ".backup
'copy.db'"`.

## Settings

The unit sets `PLINKO_TENANT` from the instance name and reads the rest from the env file, see
`docs/development.md`. The session cookie is `Secure`, so a tenant must be reached over HTTPS.
