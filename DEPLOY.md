# Plinko Deployment Guide

## Architecture Overview

```
Internet (port 80/443)
        │
    ┌───▼────────────────────────────────────┐
    │  nginx (reverse proxy, TLS termination)│
    └───┬────────────────────────────────────┘
        │  /ws  →  WebSocket (plinko:7892)
        │  /    →  Static files (plinko:7893)
    ┌───▼──────────────┐
    │  plinko (app)    │  Port 7892: WebSocket server
    │                  │  Port 7893: Built-in static file server
    └──────────────────┘
        │
    ┌───▼──────────────┐
    │  /data volume    │  Plans (MessagePack snapshots), auth.db (SQLite)
    └──────────────────┘
```

All traffic arrives at nginx on 80/443. Port 80 redirects to HTTPS. Port 443 terminates TLS and proxies:
- `GET /ws` (WebSocket upgrade) → plinko WebSocket server (port 7892)
- All other requests → plinko static file server (port 7893), which serves the built React SPA

Neither plinko port is exposed to the host — only nginx ports are.

---

## Server Deployment (Recommended — pre-built image from ghcr.io)

Every push to `main` automatically builds and publishes `ghcr.io/oranellis/plinko:latest` via
GitHub Actions. No build toolchain is needed on the server — just Docker.

### One-Script Setup

The `deploy/setup.sh` script handles everything: Docker installation, TLS certificate
acquisition, and application startup.

```bash
# Clone the repo (just the deploy scripts are needed on the server)
git clone https://github.com/oranellis/plinko.git
cd plinko

# Run the setup script as root
sudo ./deploy/setup.sh --domain plinko.example.com --email admin@example.com
```

The script:
1. **Validates upfront** — checks root, DNS resolution, port availability, internet access, and file structure before touching anything. Reports all issues at once with actionable fixes.
2. **Installs Docker** — if not already present, installs via the official Ubuntu apt repository.
3. **Pulls the image** — `ghcr.io/oranellis/plinko:latest`
4. **Obtains a TLS certificate** — runs certbot via Docker to get a Let's Encrypt certificate for your domain.
5. **Saves configuration** — writes `deploy/.env` with `DOMAIN` and `PLINKO_IMAGE` for use by `update.sh`.
6. **Starts the application** — via `docker compose up -d`. The `restart: unless-stopped` policy means containers restart automatically when Docker restarts on boot.
7. **Verifies health** — waits for the container health check, then confirms HTTPS is responding.

Use `--staging` to avoid Let's Encrypt rate limits while testing:

```bash
sudo ./deploy/setup.sh --domain plinko.example.com --email admin@example.com --staging
# Once confirmed working, re-run without --staging to get a real certificate
```

### Prerequisites

- Ubuntu server (20.04 or later) with a public IP
- A domain pointed at the server (A record for `plinko.example.com`)
- Ports 80 and 443 open in the firewall

### Auto-restart on Boot

Docker Compose uses `restart: unless-stopped` for all services. The Docker daemon itself is
enabled as a systemd service by the installer, so containers restart automatically after a
reboot — no additional configuration is needed.

### Updating to a new version

Use the `deploy/update.sh` script for a verbose, verified update:

```bash
sudo ./deploy/update.sh
```

The script pulls the latest image, recreates containers with the new image, waits for the
health check, and confirms HTTPS is responding. The domain is read automatically from
`deploy/.env` (written by `setup.sh`).

To update to a specific image tag:
```bash
sudo ./deploy/update.sh --image ghcr.io/oranellis/plinko:v1.2.3
```

### Useful commands

```bash
# View live logs
docker compose -f deploy/docker-compose.yml logs -f

# Check container status
docker compose -f deploy/docker-compose.yml ps

# Stop the stack
docker compose -f deploy/docker-compose.yml down

# Start/restart the stack
DOMAIN=plinko.example.com docker compose -f deploy/docker-compose.yml up -d
```

---

## Local / Staging Deployment (Self-Signed Certificate)

For local testing or internal networks where Let's Encrypt is not available:

```bash
# Generate a self-signed certificate
DOMAIN=localhost ./deploy/scripts/gen-self-signed.sh

# Start the stack (pulls pre-built image from ghcr.io)
DOMAIN=localhost docker compose -f deploy/docker-compose.yml up -d
```

Your browser will show a certificate warning (expected for self-signed certs). Accept it to proceed.

---

## Building Locally

To build the image yourself instead of using the pre-built one:

```bash
docker build -t plinko:local .

# Use the locally-built image instead of ghcr.io
DOMAIN=localhost PLINKO_IMAGE=plinko:local \
  docker compose -f deploy/docker-compose.yml up -d
```

---

## CI/CD — GitHub Actions

`.github/workflows/docker.yml` builds and pushes to ghcr.io automatically:

| Trigger | Tags produced |
|---|---|
| Push to `main` | `ghcr.io/oranellis/plinko:latest` |
| Push tag `v0.4.0` | `ghcr.io/oranellis/plinko:0.4.0`, `:0.4`, `:latest` |

The workflow uses `GITHUB_TOKEN` (no extra secrets needed) and registry-based layer caching for fast rebuilds. The package visibility defaults to private on first push — make it public in the GitHub repository's **Packages** settings if you want unauthenticated pulls.

---

## Configuration

All runtime configuration is done via environment variables, passed through `docker-compose.yml`.

| Variable          | Default         | Description                                      |
|-------------------|-----------------|--------------------------------------------------|
| `DOMAIN`          | *(required)*    | Hostname used in nginx config and TLS cert paths |
| `PLINKO_IMAGE`    | `ghcr.io/oranellis/plinko:latest` | Docker image to use |
| `PLINKO_PORT`     | `7892`          | Internal WebSocket port                          |
| `PLINKO_WEB_DIST` | `/app/dist`     | Path to built React assets inside the container  |
| `XDG_DATA_HOME`   | `/data`         | Data directory (plans + auth DB)                 |

The `deploy/setup.sh` script writes a `deploy/.env` file with `DOMAIN` and `PLINKO_IMAGE` so that `update.sh` and manual `docker compose` invocations from the deploy directory auto-read the environment.

---

## Data Persistence

Plan data and the auth database are stored in the `plinko-data` Docker volume at `/data`:

```
/data/plinko/plans/
    <plan-uuid>/
        2026-04-12T10-00-00.msgpack    ← versioned snapshots (MessagePack)
        2026-04-12T11-30-00.msgpack
    auth.db                             ← SQLite auth database
```

### Backup

```bash
docker run --rm \
  -v plinko_plinko-data:/data:ro \
  -v $(pwd):/backup \
  alpine tar czf /backup/plinko-backup-$(date +%Y%m%d).tar.gz -C /data .
```

### Restore

```bash
docker run --rm \
  -v plinko_plinko-data:/data \
  -v $(pwd):/backup \
  alpine tar xzf /backup/plinko-backup-20260412.tar.gz -C /data
```

---

## Certificate Renewal

The `certbot` service runs automatically inside the stack, checking for renewal every 12 hours. Let's Encrypt certificates are renewed when less than 30 days remain.

To manually trigger renewal:

```bash
docker compose -f deploy/docker-compose.yml exec certbot certbot renew
docker compose -f deploy/docker-compose.yml exec nginx nginx -s reload
```

---

## Monitoring

```bash
# View live logs
docker compose -f deploy/docker-compose.yml logs -f

# Specific service
docker compose -f deploy/docker-compose.yml logs -f plinko
docker compose -f deploy/docker-compose.yml logs -f nginx

# Check container health
docker compose -f deploy/docker-compose.yml ps
```

---

## Security Evaluation

### Strengths

**TLS/Transport security**
- All external traffic is HTTPS-only; HTTP redirects to HTTPS with a 301.
- TLS 1.2 and 1.3 only (older protocols disabled).
- Mozilla "Modern" cipher suite: ECDHE + AES-GCM/ChaCha20 only.
- OCSP stapling enabled to reduce handshake latency and improve privacy.
- HSTS header with `max-age=63072000` (2 years), `includeSubDomains`, and `preload`.

**Isolation**
- The plinko binary runs as a non-root user (UID 1001, no shell) inside the container.
- Internal ports 7892/7893 are never published to the host — only accessible via the Docker network.
- The TLS certificate private key is mounted read-only in nginx.
- `plinko-data` volume is only accessible to the plinko container.

**Authentication**
- Passwords are stored as bcrypt hashes (cost 12).
- Sessions use opaque random UUIDs stored in SQLite, not plain files.
- Plan visibility is enforced server-side; non-admin users only see permitted plans.

**Supply chain**
- The workflow uses `GITHUB_TOKEN` with `contents: read, packages: write` — minimum required permissions.
- Multi-stage Dockerfile: the final image contains only the compiled binary and static assets, not build toolchains.

### Risks and Recommendations

**Default credentials — CRITICAL**
Change `root@plinko.local` / `root` immediately after first deployment.

**No rate limiting on WebSocket connections**
A client can open many simultaneous connections. Add nginx `limit_conn` on `/ws` or per-IP limits in the Rust server.

**Monday.com API token stored with plan data**
The token is stored alongside plans in the data volume. Anyone with volume access can read it. Consider storing it as an environment variable instead.

**No Content-Security-Policy**
The CSP header is commented out in the nginx config. Enable one once the deployment is stable to mitigate XSS.

**No automated backups**
Data lives in a Docker volume. A `docker volume prune` would destroy it. Add a cron job running the backup command above.

**Session tokens in localStorage**
Tokens are stored in `localStorage` (accessible to JS). Acceptable with a strong CSP and no XSS. HttpOnly cookies would be safer but require changes to the WebSocket auth flow.
