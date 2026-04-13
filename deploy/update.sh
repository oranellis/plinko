#!/usr/bin/env bash
# deploy/update.sh
#
# Update a running Plinko instance to the latest Docker image.
# Pulls the newest image, gracefully restarts the service, then verifies
# the application is healthy. Requires an initial deploy via setup.sh.
#
# Usage:
#   sudo ./deploy/update.sh [OPTIONS]
#
# Options:
#   --image  IMAGE    Docker image override (default: ghcr.io/oranellis/plinko:latest)
#   --domain DOMAIN   Domain for HTTPS health check (autodetected from service if omitted)
#   --help            Show this help message
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
SERVICE_NAME="plinko"

# ── Colours ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}  ==>${RESET} $*"; }
success() { echo -e "${GREEN}  ✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}  ⚠${RESET}  $*"; }
step()    { echo -e "\n${BOLD}$*${RESET}"; printf '─%.0s' {1..60}; echo; }
fail()    { echo -e "${RED}  ✗${RESET}  $*" >&2; }

# ── Argument parsing ──────────────────────────────────────────────────────────
IMAGE="${PLINKO_IMAGE:-ghcr.io/oranellis/plinko:latest}"
DOMAIN=""

usage() {
    cat <<EOF
Usage: sudo $0 [OPTIONS]

Update a running Plinko instance to the latest Docker image.

Options:
  --image  IMAGE    Docker image to pull (default: ghcr.io/oranellis/plinko:latest)
  --domain DOMAIN   Domain for HTTPS health check (autodetected if omitted)
  --help            Show this help message

Examples:
  sudo $0
  sudo $0 --image ghcr.io/oranellis/plinko:v1.2.3
  sudo $0 --domain plinko.example.com
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --image)   IMAGE="${2:?'--image requires a value'}";  shift 2 ;;
        --domain)  DOMAIN="${2:?'--domain requires a value'}"; shift 2 ;;
        --help|-h) usage ;;
        *) echo -e "${RED}Unknown argument: $1${RESET}\nRun '$0 --help' for usage." >&2; exit 1 ;;
    esac
done

# ── Preflight checks ──────────────────────────────────────────────────────────
step "Preflight Checks"

ERRORS=()

# 1. Must run as root
if [[ $EUID -ne 0 ]]; then
    ERRORS+=("Not running as root.
         Fix: Re-run with sudo:  sudo $0 $*")
fi

# 2. Docker must be installed
if ! command -v docker &>/dev/null; then
    ERRORS+=("Docker is not installed.
         Fix: Run setup.sh first to deploy the application.")
fi

# 3. Compose file must exist
if [[ ! -f "$COMPOSE_FILE" ]]; then
    ERRORS+=("docker-compose.yml not found at: $COMPOSE_FILE
         Fix: Run this script from within the cloned plinko repository.")
fi

# 4. Git must be available and repo must have a remote
if ! command -v git &>/dev/null; then
    ERRORS+=("git is not installed.
         Fix: Install git:  apt-get install -y git")
elif ! git -C "$REPO_DIR" rev-parse --git-dir &>/dev/null; then
    ERRORS+=("$REPO_DIR is not a git repository.
         Fix: Clone the repository first.")
fi

# Bail on critical failures before further checks
if [[ ${#ERRORS[@]} -gt 0 ]]; then
    echo ""
    for i in "${!ERRORS[@]}"; do
        fail "[$((i+1))] ${ERRORS[$i]}"
        echo ""
    done
    exit 1
fi

# 4. Systemd service should be registered
if systemctl list-unit-files "${SERVICE_NAME}.service" &>/dev/null \
        && systemctl list-unit-files "${SERVICE_NAME}.service" | grep -q "${SERVICE_NAME}"; then
    SVC_STATE=$(systemctl is-active "${SERVICE_NAME}" 2>/dev/null || echo "inactive")
    success "Service: ${SERVICE_NAME}.service is ${SVC_STATE}"
else
    warn "systemd service '${SERVICE_NAME}' is not registered."
    warn "The update will still pull the image and attempt a compose restart,"
    warn "but automatic startup on boot requires running setup.sh first."
fi

# 5. ghcr.io reachable
# ghcr.io always returns HTTP 401 for unauthenticated requests — that is correct
# behaviour. Only treat connection failures (curl exit / HTTP 000) as an error.
GHCR_HTTP=$(curl -s --max-time 10 -o /dev/null -w "%{http_code}" https://ghcr.io/v2/ 2>/dev/null || echo "000")
if [[ "$GHCR_HTTP" == "000" ]]; then
    ERRORS+=("Cannot reach ghcr.io (connection failed). Outbound HTTPS is required to pull the Docker image.
         Fix: Check firewall / outbound rules:  curl -v https://ghcr.io/v2/")
else
    success "Network: ghcr.io is reachable (HTTP $GHCR_HTTP)"
fi

if [[ ${#ERRORS[@]} -gt 0 ]]; then
    echo ""
    for i in "${!ERRORS[@]}"; do
        fail "[$((i+1))] ${ERRORS[$i]}"
        echo ""
    done
    exit 1
fi

# 6. Auto-detect domain from the running systemd service environment (if not provided)
if [[ -z "$DOMAIN" ]]; then
    DOMAIN=$(systemctl show "${SERVICE_NAME}" -p Environment 2>/dev/null \
             | grep -oP 'DOMAIN=\K\S+' | head -1 || true)
    if [[ -n "$DOMAIN" ]]; then
        success "Detected domain: $DOMAIN"
    else
        warn "Could not detect domain — HTTPS health check will be skipped."
        warn "Pass --domain your.domain.com to enable it."
    fi
fi

success "All preflight checks passed"

# ── Pull latest code ──────────────────────────────────────────────────────────
step "Pulling Latest Code"

info "Updating repository in $REPO_DIR ..."
GIT_OUTPUT=$(git -C "$REPO_DIR" pull --ff-only 2>&1) || {
    echo ""
    echo -e "${RED}${BOLD}git pull failed.${RESET}" >&2
    echo -e "  The repository could not be fast-forwarded. If you have local changes:" >&2
    echo -e "    git -C $REPO_DIR stash && sudo $0 $*" >&2
    exit 1
}
echo "$GIT_OUTPUT" | sed 's/^/  /'
if echo "$GIT_OUTPUT" | grep -q "Already up to date"; then
    success "Repository is already up to date"
else
    success "Repository updated"
fi

# ── Pull latest image ─────────────────────────────────────────────────────────
step "Pulling Latest Image"

# Record the current local digest so we can report whether anything changed
OLD_DIGEST=$(docker inspect --format='{{index .RepoDigests 0}}' "$IMAGE" 2>/dev/null \
             | awk -F'@' '{print $2}' || echo "none")

info "Pulling $IMAGE ..."
if ! docker pull "$IMAGE"; then
    echo ""
    echo -e "${RED}${BOLD}Failed to pull Docker image: $IMAGE${RESET}" >&2
    echo -e "  If the image is private, authenticate first with your classic PAT:" >&2
    echo -e "    echo \"\$CR_PAT\" | docker login ghcr.io -u GITHUB_USERNAME --password-stdin" >&2
    echo -e "  Then re-run this script." >&2
    exit 1
fi

NEW_DIGEST=$(docker inspect --format='{{index .RepoDigests 0}}' "$IMAGE" 2>/dev/null \
             | awk -F'@' '{print $2}' || echo "unknown")

if [[ "$OLD_DIGEST" == "$NEW_DIGEST" && "$OLD_DIGEST" != "none" ]]; then
    success "Image is already up to date"
    info "Digest: $NEW_DIGEST"
else
    success "New image downloaded"
    if [[ "$OLD_DIGEST" != "none" ]]; then
        info "Previous: $OLD_DIGEST"
    fi
    info "Current:  $NEW_DIGEST"
fi

# ── Stop running service ──────────────────────────────────────────────────────
step "Stopping Service"

if systemctl is-active --quiet "${SERVICE_NAME}" 2>/dev/null; then
    info "Stopping ${SERVICE_NAME}.service (waiting up to 60 s for graceful shutdown)..."
    systemctl stop "${SERVICE_NAME}"
    success "Service stopped"
else
    warn "Service '${SERVICE_NAME}' was not running."
    # Stop any lingering compose stack manually just in case
    if docker compose -f "$COMPOSE_FILE" ps --quiet 2>/dev/null | grep -q .; then
        info "Stopping orphaned compose stack..."
        DOMAIN="${DOMAIN:-}" PLINKO_IMAGE="$IMAGE" \
            docker compose -f "$COMPOSE_FILE" down --timeout 30 2>/dev/null || true
        success "Compose stack stopped"
    fi
fi

# ── Start updated service ─────────────────────────────────────────────────────
step "Starting Service"

info "Starting ${SERVICE_NAME}.service with updated image..."
systemctl start "${SERVICE_NAME}"
success "systemctl start ${SERVICE_NAME} — OK"

# ── Health check ──────────────────────────────────────────────────────────────
step "Health Check"

info "Waiting for plinko container to become healthy (up to 90 s)..."

MAX_WAIT=90
ELAPSED=0
CONTAINER_HEALTHY=0

printf "  "
while [[ $ELAPSED -lt $MAX_WAIT ]]; do
    STATUS=$(docker ps --filter "label=com.docker.compose.service=plinko" \
                 --format '{{.Status}}' 2>/dev/null | head -1 || echo "")

    if echo "$STATUS" | grep -qi "(healthy)"; then
        CONTAINER_HEALTHY=1
        break
    elif echo "$STATUS" | grep -qi "(unhealthy)"; then
        echo ""
        warn "Container reported unhealthy. Recent logs:"
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
    warn "Health check timed out or container is unhealthy (elapsed: ${ELAPSED}s)"
    warn "Check container status:  docker compose -f $COMPOSE_FILE ps"
    warn "Check application logs:  journalctl -u plinko -n 50"
fi

# ── HTTPS endpoint check ──────────────────────────────────────────────────────
if [[ -n "$DOMAIN" ]]; then
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
fi

# ── Summary ───────────────────────────────────────────────────────────────────
step "Update Complete"

echo -e "  ${GREEN}${BOLD}Plinko has been updated successfully.${RESET}"
[[ -n "$DOMAIN" ]] && echo -e "  Running at ${BOLD}https://${DOMAIN}/${RESET}"
echo ""
echo -e "  ${BOLD}Image${RESET}"
echo "    $IMAGE"
[[ -n "$NEW_DIGEST" && "$NEW_DIGEST" != "unknown" ]] && echo "    $NEW_DIGEST"
echo ""
echo -e "  ${BOLD}Useful commands${RESET}"
echo "    Logs:    journalctl -u plinko -f"
echo "    Status:  systemctl status plinko"
echo "    Restart: systemctl restart plinko"
echo ""
