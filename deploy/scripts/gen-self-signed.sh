#!/usr/bin/env bash
# deploy/scripts/gen-self-signed.sh
#
# Generates a self-signed TLS certificate for local or staging deployments.
# Not for production use. For production, run init-letsencrypt.sh instead.
#
# Usage:
#   DOMAIN=localhost ./deploy/scripts/gen-self-signed.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(dirname "$SCRIPT_DIR")"

: "${DOMAIN:=localhost}"

CERT_DIR="$DEPLOY_DIR/certs/live/$DOMAIN"
mkdir -p "$CERT_DIR"

echo "==> Generating self-signed certificate for: $DOMAIN"

openssl req -x509 -nodes -newkey rsa:4096 -days 3650 \
    -keyout "$CERT_DIR/privkey.pem" \
    -out    "$CERT_DIR/fullchain.pem" \
    -subj   "/CN=$DOMAIN" \
    -addext "subjectAltName=DNS:$DOMAIN,IP:127.0.0.1"

# For self-signed certs the leaf IS its own CA, so chain.pem = the cert itself.
# nginx's ssl_trusted_certificate needs this to locate the issuer and suppress
# the "ssl_stapling ignored, issuer certificate not found" warning.
cp "$CERT_DIR/fullchain.pem" "$CERT_DIR/chain.pem"
cp "$CERT_DIR/fullchain.pem" "$CERT_DIR/cert.pem"

echo "✓ Certificate written to $CERT_DIR"
echo "  fullchain.pem  (certificate)"
echo "  chain.pem      (CA / issuer chain — copy of cert for self-signed)"
echo "  cert.pem       (certificate — same as fullchain.pem for self-signed)"
echo "  privkey.pem    (private key)"
echo ""
echo "Start the stack with:"
echo "  DOMAIN=$DOMAIN docker compose -f deploy/docker-compose.yml up -d"
