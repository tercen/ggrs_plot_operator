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
COPY Cargo.lock ./

# Copy build script
COPY build.rs ./

# Copy proto files submodule (needed for build.rs)
COPY tercen_grpc_api ./tercen_grpc_api

# Copy source tree
COPY src ./src

# Copy palettes.json (used by include_str! at compile time)
COPY palettes.json ./

# Build with jemalloc feature enabled
# Using dev-release profile for faster CI builds (4-5 min vs 12+ min)
# For production releases, use --release instead
# Configure git auth in same RUN to access private dependencies
RUN --mount=type=secret,id=gh_pat \
    if [ -f /run/secrets/gh_pat ]; then \
      GH_PAT=$(cat /run/secrets/gh_pat) && \
      git config --global url."https://${GH_PAT}@github.com/".insteadOf "https://github.com/"; \
    fi && \
    cargo build --profile dev-release --features jemalloc

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

# Create app directory
RUN mkdir -p /app

WORKDIR /app

# Copy binary from builder (dev-release profile)
COPY --from=builder /app/target/dev-release/ggrs_plot_operator /usr/local/bin/ggrs_plot_operator

# Set permissions
RUN chmod +x /usr/local/bin/ggrs_plot_operator

# NOTE: Running as root (Tercen will set --user 0:0 or 1000:1000 as needed)

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
