# Multi-stage Dockerfile for GGRS Plot Operator
# Optimized for minimal size with jemalloc for better memory management
#
# NOTE: This can be replaced with a custom base image in the future
#       for faster builds and consistent dependencies across Tercen operators

# ============================================================================
# Builder Stage - Rust toolchain (not included in final image)
# ============================================================================
FROM rust:1.92-slim-bookworm AS builder

# Install build dependencies (including git for private dependencies)
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    libjemalloc-dev \
    make \
    git \
    libglib2.0-dev \
    libcairo2-dev \
    libpango1.0-dev \
    libfontconfig1-dev \
    libfreetype6-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml ./

# Copy build script
COPY build.rs ./

# Copy proto files (needed for build.rs)
COPY protos ./protos

# Copy source tree
COPY src ./src

# Build with jemalloc feature enabled
# Release mode with optimizations
# Configure git auth in same RUN to access private dependencies
RUN --mount=type=secret,id=gh_pat \
    if [ -f /run/secrets/gh_pat ]; then \
      GH_PAT=$(cat /run/secrets/gh_pat) && \
      git config --global url."https://${GH_PAT}@github.com/".insteadOf "https://github.com/"; \
    fi && \
    cargo build --release --features jemalloc

# ============================================================================
# Runtime Stage - Minimal runtime environment (no Rust toolchain)
# ============================================================================
FROM debian:bookworm-slim
# NOTE: Can be replaced with tercen/rust-operator-base:latest or similar

# Install runtime dependencies only
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libjemalloc2 \
    libssl3 \
    libcairo2 \
    libfontconfig1 \
    libfreetype6 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -f -g 1000 operator && \
    useradd -m -u 1000 -g operator operator && \
    mkdir -p /app && \
    chown -R operator:operator /app

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/ggrs_plot_operator /usr/local/bin/ggrs_plot_operator

# Set permissions
RUN chmod +x /usr/local/bin/ggrs_plot_operator

# Switch to non-root user
USER operator

# Configure jemalloc environment variables for optimal memory management
ENV LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libjemalloc.so.2
ENV MALLOC_CONF=background_thread:true,metadata_thp:auto,dirty_decay_ms:30000,muzzy_decay_ms:30000

# Set Rust backtrace for better debugging (can be overridden)
ENV RUST_BACKTRACE=1

# Health check endpoint (if implemented)
# HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
#   CMD ["/usr/local/bin/ggrs_plot_operator", "--health-check"]

# Entry point
ENTRYPOINT ["/usr/local/bin/ggrs_plot_operator"]
