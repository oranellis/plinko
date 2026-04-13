#!/usr/bin/env bash
# deploy/setup.sh
#
# One-shot server setup for Plinko.
# Installs Docker, obtains a TLS certificate, starts the application, and
# registers a systemd service for automatic startup on boot.
#
# Usage:
#   sudo ./deploy/setup.sh --domain plinko.example.com --email admin@example.com
#
# Options:
#   --domain DOMAIN   Fully-qualified domain name (required)
#   --email  EMAIL    Email for Let's Encrypt notifications (required)
#   --staging         Use Let's Encrypt staging (avoids rate limits when testing)
#   --image  IMAGE    Docker image override (default: ghcr.io/oranellis/plinko:latest)
#   --help            Show this help
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
SERVICE_NAME="plinko"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

# ── Colours ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}  ==>${RESET} $*"; }
success() { echo -e "${GREEN}  ✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}  ⚠${RESET}  $*"; }
step()    { echo -e "\n${BOLD}$*${RESET}"; printf '─%.0s' {1..60}; echo; }
fail()    { echo -e "${RED}  ✗${RESET}  $*" >&2; }

# ── Argument parsing ──────────────────────────────────────────────────────────
DOMAIN=""
EMAIL=""
STAGING=0
IMAGE="${PLINKO_IMAGE:-ghcr.io/oranellis/plinko:latest}"

usage() {
    cat <<EOF
Usage: sudo $0 --domain DOMAIN --email EMAIL [OPTIONS]

Required:
  --domain DOMAIN   Fully-qualified domain name  (e.g. plinko.example.com)
  --email  EMAIL    Email address for Let's Encrypt certificate notifications

Options:
  --staging         Use Let's Encrypt staging environment (for testing)
  --image  IMAGE    Docker image to use instead of ghcr.io/oranellis/plinko:latest
  --help            Show this help message

Example:
  sudo $0 --domain plinko.example.com --email admin@example.com
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --domain)  DOMAIN="${2:?'--domain requires a value'}"; shift 2 ;;
        --email)   EMAIL="${2:?'--email requires a value'}";   shift 2 ;;
        --staging) STAGING=1; shift ;;
        --image)   IMAGE="${2:?'--image requires a value'}";   shift 2 ;;
        --help|-h) usage ;;
        *) echo -e "${RED}Unknown argument: $1${RESET}\nRun '$0 --help' for usage." >&2; exit 1 ;;
    esac
done

# ── Preflight checks (collect ALL issues then report once) ────────────────────
step "Preflight Checks"

ERRORS=()

# 1. Must run as root
if [[ $EUID -ne 0 ]]; then
    ERRORS+=("Not running as root.
         Fix: Re-run with sudo:  sudo $0 $*")
fi

# 2. Required arguments
[[ -z "$DOMAIN" ]] && ERRORS+=("--domain is required.
         Fix: sudo $0 --domain your.domain.com --email you@example.com")
[[ -z "$EMAIL" ]]  && ERRORS+=("--email is required.
         Fix: sudo $0 --domain $DOMAIN --email you@example.com")

# Bail immediately on root/arg failures — remaining checks won't be meaningful
if [[ ${#ERRORS[@]} -gt 0 ]]; then
    echo ""
    for i in "${!ERRORS[@]}"; do
        fail "[$((i+1))] ${ERRORS[$i]}"
        echo ""
    done
    exit 1
fi

# 3. OS check (warn-only on non-Ubuntu; Docker install step uses Ubuntu apt method)
if command -v lsb_release &>/dev/null; then
    DISTRO=$(lsb_release -si 2>/dev/null || echo "unknown")
    DISTRO_VER=$(lsb_release -sr 2>/dev/null || echo "")
    if [[ "$DISTRO" == "Ubuntu" ]]; then
        success "OS: Ubuntu $DISTRO_VER"
    else
        warn "This script targets Ubuntu. Detected: $DISTRO $DISTRO_VER"
        warn "Docker installation uses the Ubuntu apt method — it may fail on other distros."
        warn "If Docker is already installed, the rest of the script will proceed normally."
    fi
else
    warn "Cannot detect OS (lsb_release not found). Continuing anyway."
fi

# 4. Deploy directory structure is intact
if [[ ! -f "$COMPOSE_FILE" ]]; then
    ERRORS+=("docker-compose.yml not found at: $COMPOSE_FILE
         Fix: Run this script from within the cloned plinko repository.")
fi

# 5. DNS — domain must resolve to this machine's public IP
PUBLIC_IP=$(curl -sf --max-time 8 https://api.ipify.org 2>/dev/null \
         || curl -sf --max-time 8 https://ifconfig.me  2>/dev/null \
         || echo "")

RESOLVED_IP=""
if command -v dig &>/dev/null; then
    RESOLVED_IP=$(dig +short "$DOMAIN" A 2>/dev/null | grep -E '^[0-9]+\.' | tail -1 || true)
elif command -v getent &>/dev/null; then
    RESOLVED_IP=$(getent hosts "$DOMAIN" 2>/dev/null | awk '{print $1}' | head -1 || true)
fi

THIS_IP="${PUBLIC_IP:-unknown}"
if [[ -z "$RESOLVED_IP" ]]; then
    ERRORS+=("Domain '$DOMAIN' does not resolve to any IP address.
         Fix: Create a DNS A record pointing $DOMAIN to this server's IP ($THIS_IP).
         Note: DNS propagation can take up to 24 h after creating the record.")
elif [[ -n "$PUBLIC_IP" && "$RESOLVED_IP" != "$PUBLIC_IP" ]]; then
    ERRORS+=("Domain '$DOMAIN' resolves to $RESOLVED_IP, but this server's IP is $PUBLIC_IP.
         Fix: Update the DNS A record for $DOMAIN to point to $PUBLIC_IP.
         Note: DNS propagation can take up to 24 h after updating the record.")
else
    success "DNS: $DOMAIN resolves to $RESOLVED_IP"
fi

# 6. Ports 80 and 443 must be free (docker-proxy holding them for our own stack is OK)
for port in 80 443; do
    RAW=$(ss -tlnp 2>/dev/null | grep ":${port} " || true)
    if [[ -n "$RAW" ]]; then
        # Extract the PID (ss format: users:(("proc",pid=NNN,...)))
        PID=$(echo "$RAW" | grep -oP 'pid=\K[0-9]+' | head -1 || true)
        PROC=""
        if [[ -n "$PID" ]]; then
            PROC=$(cat /proc/"$PID"/comm 2>/dev/null || echo "unknown")
        fi
        if [[ "$PROC" == "docker-proxy" ]]; then
            warn "Port $port is held by docker-proxy (existing container) — will be released on stack restart."
        elif [[ -n "$PROC" ]]; then
            ERRORS+=("Port $port is already in use by '$PROC' (PID ${PID:-?}).
         Fix: Stop the conflicting service before running setup:
              systemctl stop $PROC  (or: kill $PID)")
        fi
    fi
done

# 7. Internet access — must reach ghcr.io to pull the image.
# ghcr.io always returns HTTP 401 for unauthenticated requests; that is correct
# behaviour and means the host is reachable. Only flag an error for connection
# failures (curl exit codes 6/7/28 etc.) — not for HTTP 4xx responses.
GHCR_HTTP=$(curl -s --max-time 10 -o /dev/null -w "%{http_code}" https://ghcr.io/v2/ 2>/dev/null || echo "000")
if [[ "$GHCR_HTTP" == "000" ]]; then
    ERRORS+=("Cannot reach ghcr.io (connection failed). Outbound HTTPS is required to pull the Docker image.
         Fix: Check firewall / outbound rules:  curl -v https://ghcr.io/v2/")
else
    success "Network: ghcr.io is reachable (HTTP $GHCR_HTTP)"
fi

# ── Report all preflight errors ───────────────────────────────────────────────
if [[ ${#ERRORS[@]} -gt 0 ]]; then
    echo ""
    echo -e "${RED}${BOLD}Found ${#ERRORS[@]} issue(s) that must be resolved before setup can continue:${RESET}"
    echo ""
    for i in "${!ERRORS[@]}"; do
        fail "[$((i+1))] ${ERRORS[$i]}"
        echo ""
    done
    exit 1
fi

success "All preflight checks passed"

# ── Install Docker ────────────────────────────────────────────────────────────
step "Docker"

if command -v docker &>/dev/null && docker compose version &>/dev/null 2>&1; then
    DOCKER_VER=$(docker --version | awk '{print $3}' | tr -d ',')
    COMPOSE_VER=$(docker compose version --short 2>/dev/null || echo "unknown")
    success "Already installed — Docker $DOCKER_VER, Compose $COMPOSE_VER"
else
    info "Installing Docker via official apt repository..."

    # Prerequisites
    apt-get update -q
    apt-get install -y -q ca-certificates curl gnupg

    # Add Docker's GPG key and repository
    install -m 0755 -d /etc/apt/keyrings
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
        | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
    chmod a+r /etc/apt/keyrings/docker.gpg

    ARCH=$(dpkg --print-architecture)
    CODENAME=$(lsb_release -cs)
    echo "deb [arch=${ARCH} signed-by=/etc/apt/keyrings/docker.gpg] \
https://download.docker.com/linux/ubuntu ${CODENAME} stable" \
        > /etc/apt/sources.list.d/docker.list

    apt-get update -q
    apt-get install -y -q \
        docker-ce docker-ce-cli containerd.io \
        docker-buildx-plugin docker-compose-plugin

    systemctl enable docker
    systemctl start docker

    DOCKER_VER=$(docker --version | awk '{print $3}' | tr -d ',')
    success "Installed Docker $DOCKER_VER"
fi

# ── Pull Docker image ─────────────────────────────────────────────────────────
step "Docker Image"

info "Pulling $IMAGE ..."
if ! docker pull "$IMAGE"; then
    echo ""
    echo -e "${RED}${BOLD}Failed to pull Docker image: $IMAGE${RESET}" >&2
    echo -e "  If the image is private, authenticate first with your classic PAT:" >&2
    echo -e "    echo \"\$CR_PAT\" | docker login ghcr.io -u GITHUB_USERNAME --password-stdin" >&2
    echo -e "  Then re-run this script." >&2
    exit 1
fi
success "Image ready: $IMAGE"

# ── TLS Certificate ───────────────────────────────────────────────────────────
step "TLS Certificate"

CERT_DIR="$SCRIPT_DIR/certs/live/$DOMAIN"
CERT_FILE="$CERT_DIR/fullchain.pem"
CERTBOT_WWW="$SCRIPT_DIR/certbot-www"

_needs_cert=0

if [[ -f "$CERT_FILE" ]]; then
    ISSUER=$(openssl x509 -in "$CERT_FILE" -noout -issuer 2>/dev/null || echo "")
    EXPIRY_DATE=$(openssl x509 -in "$CERT_FILE" -noout -enddate 2>/dev/null \
                  | cut -d= -f2 || echo "")
    # Expiry as unix timestamp
    EXPIRY_TS=$(date -d "$EXPIRY_DATE" +%s 2>/dev/null || echo 0)
    NOW_TS=$(date +%s)
    DAYS_LEFT=$(( (EXPIRY_TS - NOW_TS) / 86400 ))

    if echo "$ISSUER" | grep -qi "let's encrypt\|letsencrypt" && [[ $DAYS_LEFT -gt 30 ]]; then
        success "Existing Let's Encrypt certificate found (expires in ${DAYS_LEFT} days)"
        info "Skipping certificate issuance."
    elif [[ $DAYS_LEFT -le 0 ]]; then
        warn "Existing certificate has expired. Re-issuing..."
        _needs_cert=1
    elif [[ $DAYS_LEFT -le 30 ]]; then
        warn "Certificate expires in ${DAYS_LEFT} days. Re-issuing..."
        _needs_cert=1
    else
        # Present but not from LE (self-signed / staging)
        warn "Existing certificate is not from Let's Encrypt. Replacing with real certificate..."
        _needs_cert=1
    fi
else
    info "No existing certificate found."
    _needs_cert=1
fi

if [[ $_needs_cert -eq 1 ]]; then
    info "Obtaining Let's Encrypt certificate for: $DOMAIN"
    [[ $STAGING -eq 1 ]] && warn "Using Let's Encrypt STAGING environment"

    # Remove stale certbot state for this domain so certbot starts fresh.
    # We remove the live dir and renewal config but preserve accounts/ so we
    # don't unnecessarily re-register with Let's Encrypt on every run.
    rm -rf "$CERT_DIR"
    rm -f  "$SCRIPT_DIR/certs/renewal/$DOMAIN.conf"

    # Create required directories
    mkdir -p "$CERT_DIR" "$CERTBOT_WWW/.well-known/acme-challenge"

    # Bootstrap: create a temporary self-signed cert so nginx can start
    info "Creating temporary bootstrap certificate..."
    openssl req -x509 -nodes -newkey rsa:2048 -days 1 \
        -keyout "$CERT_DIR/privkey.pem" \
        -out    "$CERT_DIR/fullchain.pem" \
        -subj   "/CN=$DOMAIN" 2>/dev/null

    # chain.pem must exist for nginx ssl_trusted_certificate
    cp "$CERT_DIR/fullchain.pem" "$CERT_DIR/chain.pem"
    cp "$CERT_DIR/fullchain.pem" "$CERT_DIR/cert.pem"

    # Start nginx to serve the ACME challenge
    info "Starting nginx for ACME HTTP-01 challenge..."
    DOMAIN="$DOMAIN" PLINKO_IMAGE="$IMAGE" \
        docker compose -f "$COMPOSE_FILE" up -d nginx

    # Wait for nginx to become ready
    for _i in $(seq 1 10); do
        if curl -sf --max-time 3 -o /dev/null "http://$DOMAIN/" 2>/dev/null; then
            break
        fi
        sleep 2
    done

    # Request the real certificate
    STAGING_FLAG=""
    [[ $STAGING -eq 1 ]] && STAGING_FLAG="--staging"

    # Diagnostic: show certbot's view of the certs directory before running
    info "--- certbot pre-run diagnostics ---"
    info "certs/live/ contents:"
    ls -la "$SCRIPT_DIR/certs/live/" 2>/dev/null || echo "    (directory does not exist)"
    info "certs/archive/ contents:"
    ls -la "$SCRIPT_DIR/certs/archive/" 2>/dev/null || echo "    (directory does not exist)"
    info "certs/renewal/ contents:"
    ls -la "$SCRIPT_DIR/certs/renewal/" 2>/dev/null || echo "    (directory does not exist)"
    info "certbot-www/ contents:"
    find "$CERTBOT_WWW" 2>/dev/null || echo "    (empty or missing)"
    info "--- end diagnostics ---"

    # Mount a persistent log dir so we can read it after the container exits.
    CERTBOT_LOGS="$SCRIPT_DIR/certbot-logs"
    mkdir -p "$CERTBOT_LOGS"

    info "Running certbot (verbose)..."
    if ! docker run --rm \
        -v "$SCRIPT_DIR/certs:/etc/letsencrypt" \
        -v "$CERTBOT_WWW:/var/www/certbot" \
        -v "$CERTBOT_LOGS:/var/log/letsencrypt" \
        certbot/certbot:latest certonly \
        --webroot \
        --webroot-path=/var/www/certbot \
        --force-renewal \
        $STAGING_FLAG \
        --email "$EMAIL" \
        --agree-tos \
        --no-eff-email \
        -d "$DOMAIN" \
        --non-interactive \
        -v; then
        echo ""
        echo -e "${RED}${BOLD}certbot failed to obtain a certificate.${RESET}" >&2
        echo -e "  Common causes and fixes:" >&2
        echo -e "    • DNS not propagated yet: verify with:  dig +short $DOMAIN A" >&2
        echo -e "    • Port 80 blocked by firewall: ensure TCP/80 is open inbound" >&2
        echo -e "    • Let's Encrypt rate limit: re-run with --staging to test, then retry later" >&2
        # Print the full certbot log
        CERTBOT_LOG_FILE=$(ls -t "$CERTBOT_LOGS"/*.log 2>/dev/null | head -1 || echo "")
        if [[ -n "$CERTBOT_LOG_FILE" ]]; then
            echo ""
            echo -e "${YELLOW}${BOLD}--- certbot debug log ($CERTBOT_LOG_FILE) ---${RESET}" >&2
            cat "$CERTBOT_LOG_FILE" >&2
            echo -e "${YELLOW}${BOLD}--- end certbot log ---${RESET}" >&2
        else
            echo -e "  (No certbot log found at $CERTBOT_LOGS)" >&2
        fi
        # Stop nginx before exiting
        DOMAIN="$DOMAIN" PLINKO_IMAGE="$IMAGE" \
            docker compose -f "$COMPOSE_FILE" down --timeout 10 2>/dev/null || true
        exit 1
    fi

    # After certbot, chain.pem may not exist — create it if missing
    [[ ! -f "$CERT_DIR/chain.pem" ]] && \
        cp "$CERT_DIR/fullchain.pem" "$CERT_DIR/chain.pem"
    [[ ! -f "$CERT_DIR/cert.pem" ]]  && \
        cp "$CERT_DIR/fullchain.pem" "$CERT_DIR/cert.pem"

    success "Certificate obtained for $DOMAIN"

    # Stop the bootstrap nginx — it will be started properly by systemd below
    info "Stopping bootstrap nginx..."
    DOMAIN="$DOMAIN" PLINKO_IMAGE="$IMAGE" \
        docker compose -f "$COMPOSE_FILE" down --timeout 15 2>/dev/null || true
fi

# ── Systemd service ───────────────────────────────────────────────────────────
step "Systemd Service"

# Stop any running stack before handing control to systemd
if docker compose -f "$COMPOSE_FILE" ps --quiet 2>/dev/null | grep -q .; then
    info "Stopping existing stack (systemd will manage it from here)..."
    DOMAIN="$DOMAIN" PLINKO_IMAGE="$IMAGE" \
        docker compose -f "$COMPOSE_FILE" down --timeout 30 2>/dev/null || true
fi

cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=Plinko project-management server
Documentation=https://github.com/oranellis/plinko
Requires=docker.service
After=docker.service network-online.target
Wants=network-online.target

[Service]
Type=simple
Restart=on-failure
RestartSec=15

Environment="DOMAIN=${DOMAIN}"
Environment="PLINKO_IMAGE=${IMAGE}"

# Pull latest image before starting (ensures updates are applied on restart)
ExecStartPre=/usr/bin/docker compose -f ${COMPOSE_FILE} pull --quiet --no-deps plinko

# Run in the foreground so systemd tracks the process and captures logs
ExecStart=/usr/bin/docker compose -f ${COMPOSE_FILE} up --remove-orphans

# Graceful shutdown
ExecStop=/usr/bin/docker compose -f ${COMPOSE_FILE} down --timeout 30

StandardOutput=journal
StandardError=journal
SyslogIdentifier=plinko

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable "$SERVICE_NAME"
success "Service installed: $SERVICE_FILE"
success "Service enabled — will start automatically on boot"

# ── Start via systemd ─────────────────────────────────────────────────────────
step "Starting Application"

info "Starting plinko via systemd..."
systemctl start "$SERVICE_NAME"
success "systemctl start $SERVICE_NAME — OK"

# ── Health check ──────────────────────────────────────────────────────────────
step "Health Check"

info "Waiting for plinko container to become healthy (up to 90 s)..."

MAX_WAIT=90
ELAPSED=0
CONTAINER_HEALTHY=0

printf "  "
while [[ $ELAPSED -lt $MAX_WAIT ]]; do
    # Get health status via docker inspect (works regardless of compose project name)
    STATUS=$(docker ps --filter "label=com.docker.compose.service=plinko" \
                 --format '{{.Status}}' 2>/dev/null | head -1 || echo "")

    if echo "$STATUS" | grep -qi "(healthy)"; then
        CONTAINER_HEALTHY=1
        break
    elif echo "$STATUS" | grep -qi "(unhealthy)"; then
        echo ""
        warn "Container reported unhealthy. Checking logs..."
        docker compose -f "$COMPOSE_FILE" logs --tail=20 plinko 2>/dev/null || true
        break
    fi

    printf "."
    sleep 3
    ELAPSED=$((ELAPSED + 3))
done
echo ""

if [[ $CONTAINER_HEALTHY -eq 1 ]]; then
    success "Plinko container is healthy"
else
    warn "Health check timed out or failed (elapsed: ${ELAPSED}s)"
    warn "Check container status: docker compose -f $COMPOSE_FILE ps"
    warn "Check logs:             journalctl -u plinko -n 50"
fi

# Test the HTTPS endpoint
info "Testing HTTPS endpoint at https://$DOMAIN/ ..."
sleep 2  # Allow nginx a moment after plinko becomes healthy
HTTP_CODE=$(curl -sk --max-time 15 -o /dev/null -w "%{http_code}" "https://$DOMAIN/" 2>/dev/null || echo "000")

if [[ "$HTTP_CODE" =~ ^(200|301|302|304)$ ]]; then
    success "HTTPS responded with HTTP $HTTP_CODE — application is reachable"
elif [[ "$HTTP_CODE" == "000" ]]; then
    warn "Could not reach https://$DOMAIN/ — nginx may still be starting."
    warn "Try in a moment:  curl -sk https://$DOMAIN/"
else
    warn "HTTPS responded with HTTP $HTTP_CODE (may be normal — check manually)"
fi

# ── Summary ───────────────────────────────────────────────────────────────────
step "Setup Complete"

echo -e "  ${GREEN}${BOLD}Plinko is running at https://${DOMAIN}/${RESET}"
echo ""
echo -e "  ${BOLD}Default admin account${RESET}"
echo "    Email:    root@plinko.local"
echo "    Password: root"
echo -e "  ${RED}${BOLD}  → Change this password immediately in Settings!${RESET}"
echo ""
echo -e "  ${BOLD}Useful commands${RESET}"
echo "    Status:   systemctl status plinko"
echo "    Logs:     journalctl -u plinko -f"
echo "    Stop:     systemctl stop plinko"
echo "    Restart:  systemctl restart plinko"
echo "    Update:   systemctl restart plinko  (pulls latest image automatically)"
echo ""
echo -e "  ${BOLD}Backup${RESET}"
echo "    docker run --rm -v plinko_plinko-data:/data:ro -v \$(pwd):/backup \\"
echo "      alpine tar czf /backup/plinko-backup-\$(date +%Y%m%d).tar.gz -C /data ."
echo ""
