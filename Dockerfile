# syntax=docker/dockerfile:1.6
#
# redmine-rs container image — Railway / Heroku / Cloud Run / Fly ready.
#
# Build:   docker build -t redmine-rs .
# Run:     docker run --rm -p 3000:3000 -e RUST_LOG=info redmine-rs
# Deploy:  the platform sets $PORT (Railway auto-injects it); rm-server's
#          resolve_bind_addr() picks it up and binds 0.0.0.0:$PORT so the
#          platform's external 443/80 edge can reach the container.
#
# Multi-stage:
#   1. builder — rust:1.95 image, compiles a release binary.
#   2. runtime — debian:bookworm-slim, ships only the binary + CA certs.
#
# Image size target: ~80MB compressed (slim + statically-linked rust binary
# minus runtime deps).

# ── stage 1: builder ───────────────────────────────────────────────────
FROM rust:1.95-bookworm AS builder

WORKDIR /build

# Copy the workspace (intentionally simple — let Cargo own caching; the
# Dockerfile layer cache only matters between rebuilds with no source
# change, and we don't want to outsmart Cargo's incremental store).
COPY . .

# Release build of rm-server (the binary). The workspace contains:
#   rm-store, rm-auth, rm-handlers, redmine-canon, and rm-server.
# Only the bin target needs to ship.
RUN cargo build --release -p rm-server --bin rm-server

# ── stage 2: runtime ───────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# CA certificates for outbound HTTPS (e.g. OIDC token verification once
# rm-auth lands the IdP integration). Wrap with --no-install-recommends
# so we don't drag in apt suggestions; rm + apt clean keeps the layer tight.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/*

# Run as non-root — PaaS platforms reject root containers (Railway,
# Cloud Run); also the right default for any deploy. UID 10001 lives
# outside the system-account range; no entry in /etc/passwd is needed
# because the binary doesn't read it.
RUN groupadd --system --gid 10001 app \
 && useradd  --system --uid 10001 --gid app --no-create-home --shell /usr/sbin/nologin app

# Ship only the release binary. No source, no target dir, no tests.
COPY --from=builder /build/target/release/rm-server /usr/local/bin/rm-server

# PaaS-ready default. The platform overrides $PORT at deploy time;
# locally `-e PORT=3000` (or just rely on the rm-server config default
# of 3000 when $PORT is absent) binds the container's published port.
ENV PORT=3000 \
    RUST_LOG=info \
    RM_SEED=on

EXPOSE 3000

USER app:app

# `exec`-form: PID-1 is the binary itself, so SIGTERM reaches it for
# axum's graceful-shutdown handler (rm-server `shutdown_signal()` listens
# on SIGTERM + Ctrl-C).
CMD ["/usr/local/bin/rm-server"]
