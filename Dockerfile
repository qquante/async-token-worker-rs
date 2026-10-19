# ============================================================================
# Four.Meme Rust Worker - Multi-stage Docker Build
# ============================================================================
# 
# 🔥 FIRE MODE ALWAYS ACTIVE!
# Broadcasts transactions to 5 RPCs simultaneously:
#   - 48Club (Privacy RPC) - ~45% of BSC blocks
#   - BlockRazor - ~37% of BSC blocks
#   - bloXroute (BDN) - ~15% of BSC blocks
#   - Ankr, Chainstack - Backup
#
# Build: docker build -t four-meme-worker .
# Run:   docker run -e PRIVATE_KEY=xxx -e REDIS_URL=redis://host:6379 four-meme-worker
#
# Test (fake):  docker run four-meme-worker test-broadcaster
# Test (real):  docker run -e TEST_PRIVATE_KEY=0x... four-meme-worker test-fire-real
#
# ============================================================================

# Stage 1: Build
# Using latest stable Rust (1.85+) for edition2024 support
FROM rust:slim-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock* ./

# Create dummy src to cache dependencies
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/bin/worker.rs && \
    echo "fn main() {}" > src/bin/cli.rs && \
    echo "fn main() {}" > src/bin/test_broadcaster.rs && \
    echo "fn main() {}" > src/bin/test_fire_real.rs && \
    echo "pub fn dummy() {}" > src/lib.rs

# Build dependencies (cached layer)
RUN cargo build --release --features worker 2>/dev/null || true
RUN cargo build --release --features cli 2>/dev/null || true

# Copy real source
COPY src ./src

# Touch files to invalidate cargo cache and force rebuild
RUN touch src/lib.rs src/broadcaster.rs src/bin/worker.rs src/bin/test_broadcaster.rs src/bin/test_fire_real.rs

# Build all binaries
RUN cargo build --release --features worker
RUN cargo build --release --features cli

# Stage 2: Runtime
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies (including libssl3 for TLS/SSL)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries from builder
COPY --from=builder /app/target/release/four-meme-worker /usr/local/bin/
COPY --from=builder /app/target/release/test-broadcaster /usr/local/bin/
COPY --from=builder /app/target/release/test-fire-real /usr/local/bin/

# Environment defaults
ENV RUST_LOG=info
ENV BSC_RPC_URL=https://bsc-dataseed.binance.org
ENV REDIS_URL=redis://redis:6379
ENV PRESALE_MIN=0.19
ENV PRESALE_MAX=0.20
ENV LABEL=Meme
ENV LANG=EN
ENV MAX_CONCURRENT=5

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD pgrep four-meme-worker || exit 1

# Run
CMD ["four-meme-worker"]

