# Plinko

A self-hosted project management tool focused on dependency-aware scheduling.  
Tasks, milestones, and team members are defined once; the scheduler fills in the timeline automatically.

## Features

- Dependency graph scheduling with automatic timeline computation
- Multi-worker tasks with strict (synchronised) or relaxed allocation modes
- Calendar overrides, working-hours budgets, and per-user schedules
- Plan version history with admin-controlled restore
- Multi-session real-time collaboration (WebSocket broadcast)
- Monday.com integration (pull / push / full re-import)
- Role-based access: admin and non-admin login accounts, per-plan visibility controls
- Self-hosted with Docker Compose, nginx reverse proxy, and Let's Encrypt HTTPS

## Versioning

Plinko uses **semantic versioning** (`MAJOR.MINOR.PATCH`). A single version number is shared by the Rust binary, the React frontend, and the WebSocket protocol.

| Change type | Version bump |
|---|---|
| Breaking protocol or data changes | MAJOR |
| New user-visible features | MINOR |
| Bug fixes, internal improvements, or any AI-driven change | PATCH |

### Version locations — all four must always match

| File | Field |
|---|---|
| `Cargo.toml` (workspace root) | `[workspace.package] version` |
| `plinko-web/package.json` | `"version"` |
| `plinko-shared/src/protocol.rs` | `VERSION` constant |
| `plinko-web/src/protocol.ts` | `PROTOCOL_VERSION` constant |

`plinko/Cargo.toml` and `plinko-shared/Cargo.toml` inherit from the workspace root with `version.workspace = true` and do not need to be edited separately.

### Bumping the version

Use the helper script (updates all four locations atomically):

```bash
./scripts/bump-version.sh 0.4.0
cargo check
git add Cargo.toml plinko-web/package.json \
    plinko-shared/src/protocol.rs plinko-web/src/protocol.ts
git commit -m "chore: bump version to 0.4.0"
git tag v0.4.0
```

### Why app version = protocol version?

The client and server are always deployed together (single Docker image). The `Hello` handshake compares versions; a mismatch causes the browser to show a reconnect prompt rather than silently misbehaving with an old cached client. Keeping the two versions identical means any deployment automatically invalidates stale browser sessions.

## Quick start

### Development

```bash
# Backend (port 7892 WS, 7893 static)
cargo run

# Frontend dev server (port 5173, in a separate terminal)
cd plinko-web && npm run dev -- --host
```

### Docker (production)

```bash
# Generate a self-signed cert (localhost / staging)
DOMAIN=localhost ./deploy/scripts/gen-self-signed.sh

# Start the stack
DOMAIN=localhost docker compose -f deploy/docker-compose.yml up -d
```

See [`DEPLOY.md`](DEPLOY.md) for full deployment instructions including Let's Encrypt setup.

## Development

See [`AGENTS.md`](AGENTS.md) for the full architecture reference, conventions, and contribution guide.
