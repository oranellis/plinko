#!/usr/bin/env bash
# deploy/scripts/init-letsencrypt.sh
#
# Obtains an initial Let's Encrypt certificate for $DOMAIN using the HTTP-01
# webroot challenge. Run this ONCE before starting the full stack.
#
# Usage:
#   DOMAIN=plinko.example.com EMAIL=admin@example.com ./deploy/scripts/init-letsencrypt.sh
#
# After this script succeeds, start the stack with:
#   docker compose -f deploy/docker-compose.yml up -d
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(dirname "$SCRIPT_DIR")"

: "${DOMAIN:?Set DOMAIN to your fully-qualified domain name, e.g. plinko.example.com}"
: "${EMAIL:?Set EMAIL to your Let's Encrypt account email}"
STAGING="${STAGING:-0}"  # Set STAGING=1 to use Let's Encrypt staging (for testing)

CERTS_DIR="$DEPLOY_DIR/certs"

echo "==> Initialising Let's Encrypt for: $DOMAIN"
echo "    Certs dir : $CERTS_DIR"
echo "    Email     : $EMAIL"
echo "    Staging   : $STAGING"

# ── 1. Create required directories ────────────────────────────────────────────
mkdir -p "$CERTS_DIR"
mkdir -p "$DEPLOY_DIR/certbot-www/.well-known/acme-challenge"

# ── 2. Create a temporary self-signed cert so nginx can start ─────────────────
# nginx refuses to start if ssl_certificate does not exist; a bootstrap cert
# allows it to answer ACME challenges on port 80 before the real cert exists.
BOOTSTRAP_DIR="$CERTS_DIR/live/$DOMAIN"
if [ ! -f "$BOOTSTRAP_DIR/fullchain.pem" ]; then
    echo "==> Generating temporary self-signed certificate..."
    mkdir -p "$BOOTSTRAP_DIR"
    docker run --rm \
        -v "$CERTS_DIR:/etc/letsencrypt" \
        --entrypoint openssl \
        certbot/certbot \
        req -x509 -nodes -newkey rsa:4096 -days 1 \
        -keyout "/etc/letsencrypt/live/$DOMAIN/privkey.pem" \
        -out    "/etc/letsencrypt/live/$DOMAIN/fullchain.pem" \
        -subj   "/CN=$DOMAIN"
fi

# ── 3. Start nginx (with bootstrap cert) and certbot webroot container ─────────
echo "==> Starting nginx to answer ACME challenges..."
docker compose -f "$DEPLOY_DIR/docker-compose.yml" up -d nginx

# Give nginx a moment to start.
sleep 3

# ── 4. Request the real certificate ────────────────────────────────────────────
STAGING_FLAG=""
if [ "$STAGING" = "1" ]; then
    STAGING_FLAG="--staging"
    echo "==> Using Let's Encrypt STAGING environment"
fi

echo "==> Requesting certificate..."
docker run --rm \
    -v "$CERTS_DIR:/etc/letsencrypt" \
    -v "$DEPLOY_DIR/certbot-www:/var/www/certbot" \
    certbot/certbot certonly \
    --webroot \
    --webroot-path=/var/www/certbot \
    $STAGING_FLAG \
    --email "$EMAIL" \
    --agree-tos \
    --no-eff-email \
    -d "$DOMAIN"

# ── 5. Reload nginx with the real certificate ──────────────────────────────────
echo "==> Reloading nginx..."
docker compose -f "$DEPLOY_DIR/docker-compose.yml" exec nginx nginx -s reload

echo ""
echo "✓ Certificate obtained. Start the full stack with:"
echo "  docker compose -f deploy/docker-compose.yml up -d"
