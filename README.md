# Plinko 2

Plinko is a project management tool built around adaptive scheduling: plans are scheduled together across a whole tenant, so people's time is shared sensibly between every plan they work on, and the schedule follows every change as it is made, live for everyone viewing.

## Usage

### Debug

`./run_debug.sh` builds the debug client and server and runs a tenant on http://127.0.0.1:3000, logging in as `admin@example.com` with `change-me-now`. Its files are in `data/Debug`.

### Release

```
deploy/build.sh
sudo deploy/install.sh
sudo deploy/new_tenant.sh <name> <port> <admin email>
```

builds it, installs it under `/opt/plinko`, and runs a systemd service for the tenant, with its files in `/opt/plinko/data/<name>`. Put a proxy in front of the port.

## Docs

- `docs/development.md` running the debug build, the settings, and the checks.
- `docs/release.md` building, installing, backing up and running tenants in production.
- `docs/layout.md` the folders, the source layout of the `plinko` crate, and its conventions.
- `docs/SCHEDULER.md` the scheduler's spec, how it decides, and how it is built and measured.
- `docs/CLIENT.md` the client and server stack, how updates sync, storage, auth, and what is left.
