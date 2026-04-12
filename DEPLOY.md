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
    │  /data volume    │  Plans (JSON snapshots), auth.db (SQLite)
    └──────────────────┘
```

All traffic arrives at nginx on 80/443. Port 80 redirects to HTTPS. Port 443 terminates TLS and proxies:
- `GET /ws` (WebSocket upgrade) → plinko WebSocket server (port 7892)
- All other requests → plinko static file server (port 7893), which serves the built React SPA

Neither plinko port is exposed to the host — only nginx ports are.

---

## Prerequisites

- Docker ≥ 24 and Docker Compose v2 (`docker compose`)
- A domain name pointed at your server's public IP (A record)
- Ports 80 and 443 open in your firewall

---

## Quick Start (Production with Let's Encrypt)

### 1. Build the image

From the repository root:

```bash
docker build -t plinko:latest .
```

### 2. Obtain a TLS certificate

```bash
DOMAIN=plinko.example.com \
EMAIL=admin@example.com \
  ./deploy/scripts/init-letsencrypt.sh
```

This will:
1. Create a temporary self-signed cert so nginx can start
2. Start nginx to serve the ACME HTTP-01 challenge
3. Run certbot to obtain a real certificate from Let's Encrypt
4. Reload nginx with the real certificate

Set `STAGING=1` to use the Let's Encrypt staging environment while testing (avoids rate limits):

```bash
DOMAIN=plinko.example.com EMAIL=admin@example.com STAGING=1 \
  ./deploy/scripts/init-letsencrypt.sh
```

### 3. Start the full stack

```bash
DOMAIN=plinko.example.com \
  docker compose -f deploy/docker-compose.yml up -d
```

### 4. Change the default admin password

On first run, a default account is created:

| Field    | Value                |
|----------|----------------------|
| Email    | `root@plinko.local`  |
| Password | `root`               |

**Change this immediately** via Settings → Change Password.

---

## Local / Staging Deployment (Self-Signed Certificate)

For local testing or internal networks where Let's Encrypt is not available:

```bash
# Generate a self-signed certificate
DOMAIN=localhost ./deploy/scripts/gen-self-signed.sh

# Start the stack
DOMAIN=localhost docker compose -f deploy/docker-compose.yml up -d
```

Your browser will show a certificate warning (expected for self-signed certs). Accept it to proceed.

---

## Configuration

All runtime configuration is done via environment variables, passed through `docker-compose.yml`.

| Variable          | Default         | Description                                      |
|-------------------|-----------------|--------------------------------------------------|
| `DOMAIN`          | *(required)*    | Hostname used in nginx config and TLS cert paths |
| `PLINKO_PORT`     | `7892`          | Internal WebSocket port                          |
| `PLINKO_WEB_DIST` | `/app/dist`     | Path to built React assets inside the container  |
| `XDG_DATA_HOME`   | `/data`         | Data directory (plans + auth DB)                 |

To use a custom port mapping (e.g. if 80/443 are taken locally):

```yaml
# In docker-compose.yml, change:
ports:
  - "8080:80"
  - "8443:443"
```

---

## Data Persistence

Plan data and the auth database are stored in the `plinko-data` Docker volume at `/data`:

```
/data/plinko/plans/
    <plan-uuid>/
        2026-04-12T10-00-00.json    ← versioned snapshots
        2026-04-12T11-30-00.json
    auth.db                          ← SQLite auth database
```

### Backup

```bash
# Back up the data volume to a tar archive
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

## Updates

```bash
# Pull latest code
git pull

# Rebuild the image
docker build -t plinko:latest .

# Restart the app (data volume is preserved)
docker compose -f deploy/docker-compose.yml up -d --no-deps plinko
```

---

## Monitoring

```bash
# View live logs
docker compose -f deploy/docker-compose.yml logs -f

# View logs for a specific service
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
- Passwords are stored as bcrypt hashes (cost 12 via `bcrypt::DEFAULT_COST`).
- Sessions use opaque random UUIDs; tokens are stored in SQLite, not plain files.
- Plan visibility is enforced server-side; non-admin users only see permitted plans.

**Dependencies**
- `rusqlite` uses a bundled SQLite build — no system SQLite version dependency.
- The runtime image is `debian:bookworm-slim`, a small surface area image.
- Multi-stage build: the final image contains only the compiled binary and static assets, not build toolchains.

### Risks and Recommendations

**Default credentials — CRITICAL**
The default account `root@plinko.local` / `root` must be changed on first deployment. Consider adding a forced password-change flow or pre-generating a random password during first boot.

**No rate limiting on WebSocket connections**
The WebSocket endpoint currently has no connection-level rate limiting. A client can open many simultaneous WebSocket connections. Mitigation options:
- Add nginx `limit_conn` on the `/ws` location.
- Add per-IP connection limits in the Rust WebSocket server.

**Monday.com API token stored in plaintext**
The Monday.com API token is stored in the plan's JSON file alongside other plan data. Anyone with read access to the `/data` volume can extract it. Recommendation: store secrets separately (environment variable or a dedicated secrets file with tighter permissions).

**No Content-Security-Policy header**
A CSP header is commented out in the nginx config. Enabling one would prevent XSS attacks from loading external resources. It requires knowing all legitimate script/style/connect sources first. Recommended once deployment is stable.

**WebSocket message size**
There is no per-message size limit in the Rust WebSocket handler. A malicious client could send a very large JSON payload. Recommendation: add a `max_message_size` check in `ws_server.rs`.

**Session token storage**
Session tokens are stored in `localStorage` (`plinko_session_token`). This is accessible to JavaScript running on the page. With a strong CSP and no XSS, this is acceptable. The alternative (HttpOnly cookies) would require changes to the WebSocket auth flow.

**Single-instance — no high availability**
This setup is a single instance with no redundancy. A crash or restart causes all connected sessions to disconnect. This is acceptable for small teams but not suitable for large or mission-critical deployments.

**Backup not automated**
Data persistence relies on the `plinko-data` Docker volume. There is no automated backup. A volume loss event (e.g. `docker volume prune`) would destroy all plan data. Strongly recommended: add a cron job or CI pipeline to run the backup command above on a schedule.
