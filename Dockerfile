# ── Stage 1: Build React frontend ─────────────────────────────────────────────
FROM node:22-alpine AS frontend
WORKDIR /build
COPY plinko-web/package.json plinko-web/package-lock.json ./
RUN npm install --prefer-offline
COPY plinko-web/ ./
RUN npm run build

# ── Stage 2: Build Rust backend ────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS backend
WORKDIR /build
# Dependencies for rusqlite (bundled SQLite) and tungstenite (native-tls → OpenSSL).
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
# Pre-fetch crate registry for better layer caching.
COPY Cargo.toml Cargo.lock ./
COPY plinko-shared/Cargo.toml plinko-shared/
COPY plinko/Cargo.toml plinko/
RUN cargo fetch --locked
# Build with full source.
COPY plinko-shared/ plinko-shared/
COPY plinko/ plinko/
RUN cargo build --release --locked --bin plinko

# ── Stage 3: Runtime image ─────────────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# Non-root service user; /data is the persistent volume mount point.
RUN useradd -r -u 1001 -d /data -s /usr/sbin/nologin plinko && \
    mkdir -p /data && chown plinko:plinko /data
WORKDIR /app
COPY --from=backend  /build/target/release/plinko ./plinko
COPY --from=frontend /build/dist                  ./dist
RUN chown -R plinko:plinko /app
VOLUME /data
USER plinko
ENV PLINKO_PORT=7892 \
    PLINKO_WEB_DIST=/app/dist \
    XDG_DATA_HOME=/data
EXPOSE 7892 7893
ENTRYPOINT ["./plinko"]
