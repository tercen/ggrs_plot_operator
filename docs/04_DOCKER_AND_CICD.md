# Docker and CI/CD Configuration

## Overview

This document describes the Docker containerization and CI/CD pipeline for the GGRS Plot Operator. The setup is optimized for minimal image size, security, and performance using jemalloc for memory management.

---

## Docker Configuration

### Multi-Stage Build Strategy

The Dockerfile uses a two-stage build to minimize the final image size and **ensure the Rust toolchain is not included in the final image**:

1. **Builder Stage**: Uses `rust:1.75-slim-bookworm` for compilation (includes Cargo, rustc, build tools)
2. **Runtime Stage**: Uses `debian:bookworm-slim` for execution (only runtime libraries, no Rust toolchain)

This approach:
- Reduces final image size by ~1 GB (builder stage is discarded)
- Excludes all build tools and the Rust compiler from the final image
- Improves security by minimizing attack surface
- Can be replaced with custom base images in the future (e.g., `tercen/rust-operator-base:latest`)

### Base Image Selection

**Builder**: `rust:1.75-slim-bookworm`
- Official Rust image with minimal footprint
- Debian Bookworm base for stability
- Includes Cargo and Rust toolchain

**Runtime**: `debian:bookworm-slim`
- Minimal Debian image (~80MB base)
- Security updates maintained by Debian team
- Compatible with glibc-based binaries

### jemalloc Integration

**Why jemalloc?**
- **Better fragmentation handling**: Reduces memory overhead for long-running processes
- **Scalability**: Optimized for multi-threaded applications
- **Profiling capabilities**: Built-in memory profiling tools
- **Performance**: Generally faster than glibc malloc for many workloads

**Configuration**:
```dockerfile
# Build with jemalloc
RUN apt-get install -y libjemalloc-dev
RUN cargo build --release --features jemalloc

# Runtime with jemalloc
RUN apt-get install -y libjemalloc2
ENV LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libjemalloc.so.2
ENV MALLOC_CONF=background_thread:true,metadata_thp:auto,dirty_decay_ms:30000,muzzy_decay_ms:30000
```

**jemalloc Environment Variables**:
- `background_thread:true`: Enable background threads for asynchronous operations
- `metadata_thp:auto`: Use transparent huge pages for metadata when beneficial
- `dirty_decay_ms:30000`: Decay dirty pages after 30 seconds
- `muzzy_decay_ms:30000`: Decay muzzy pages after 30 seconds

### Build Optimizations

The Dockerfile sets several Cargo profile flags for size optimization:

```dockerfile
ENV CARGO_PROFILE_RELEASE_LTO=true              # Link-time optimization
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1       # Single codegen unit for better optimization
ENV CARGO_PROFILE_RELEASE_OPT_LEVEL="z"         # Optimize for size
ENV CARGO_PROFILE_RELEASE_STRIP=true            # Strip debug symbols
```

**Size Impact**:
- Without optimizations: ~50-100 MB binary
- With optimizations: ~15-30 MB binary
- Trade-off: Slightly slower compilation, no impact on runtime performance

### Security Features

1. **Non-root User**:
   ```dockerfile
   RUN useradd -m -u 1000 operator
   USER operator
   ```
   - Runs container as unprivileged user
   - Reduces attack surface
   - Follows least-privilege principle

2. **Minimal Dependencies**:
   - Only includes required runtime libraries
   - No build tools in final image
   - Reduces vulnerability exposure

3. **Security Scanning**:
   - Trivy scan in CI pipeline
   - Results uploaded to GitHub Security tab
   - Automated vulnerability detection

### Image Size Comparison

Expected sizes:
- Builder stage: ~1.2 GB (not pushed)
- Runtime image: ~120-150 MB
  - Base Debian slim: ~80 MB
  - jemalloc + SSL libraries: ~10 MB
  - Application binary: ~20-30 MB
  - Other dependencies: ~10-20 MB

Compare to alternatives:
- Alpine-based Rust: ~100-120 MB (but glibc compatibility issues)
- Ubuntu-based: ~200-250 MB (larger base)
- scratch-based: ~30-40 MB (no shell, hard to debug)

---

## CI/CD Pipeline

### Workflow Triggers

The pipeline runs on:

1. **Push to main branch**: Full build, test, and push to registry
2. **Push to semver tags** (e.g., `0.1.0`, `1.2.3`): Full build, test, and push with version tag
3. **Note**: Tests run on all pushes, but Docker builds only happen on main branch or tags

### Pipeline Stages

#### 1. Test and Lint

**Purpose**: Ensure code quality and correctness

**Steps**:
- Checkout code
- Install Rust toolchain with rustfmt and clippy
- Cache Cargo registry, index, and target directory
- Install system dependencies (protobuf, jemalloc, SSL)
- Check formatting with `cargo fmt -- --check`
- Run linting with `cargo clippy -- -D warnings`
- Run unit tests with `cargo test --all-features --verbose`
- Run doc tests with `cargo test --doc`

**Caching Strategy**:
```yaml
key: ${{ runner.os }}-cargo-registry-${{ hashFiles('**/Cargo.lock') }}
```
- Invalidates cache when dependencies change
- Speeds up builds by 2-5 minutes
- Shared across workflow runs

#### 2. Docker Build and Push

**Purpose**: Build and publish Docker images

**Steps**:
- Checkout code
- Set up Docker Buildx (multi-platform builds)
- Log in to GitHub Container Registry (ghcr.io)
- Extract metadata for tags and labels
- Build and push Docker image
- Generate artifact attestation (provenance)

**Tagging Strategy**:

| Trigger | Tags Generated | Example |
|---------|----------------|---------|
| Push to main | `main` | `main` |
| Tag 0.1.0 | `0.1.0` | `0.1.0` |
| Tag 1.2.3 | `1.2.3` | `1.2.3` |

**Notes**:
- Semver tags do NOT include 'v' prefix (use `0.1.0`, not `v0.1.0`)
- No automatic `latest` tag - use branch name (`main`) for latest
- No SHA-based tags - simpler tag strategy

**Registry**: GitHub Container Registry (ghcr.io)
- Free for public repositories
- Integrated with GitHub authentication
- Supports artifact attestation
- URL format: `ghcr.io/tercen/ggrs_plot_operator:tag`

**Build Cache**:
```yaml
cache-from: type=gha
cache-to: type=gha,mode=max
```
- Uses GitHub Actions cache
- Speeds up subsequent builds
- Persists layers between runs

#### 3. Security Scan

**Purpose**: Detect vulnerabilities in container image

**Tool**: Trivy by Aqua Security
- Scans OS packages and application dependencies
- Checks against CVE databases
- Generates SARIF format for GitHub Security

**Integration**:
- Results appear in GitHub Security tab
- Fails pipeline on critical vulnerabilities (optional)
- Provides actionable remediation advice

#### 4. Operator Installation Test (Optional)

**Purpose**: Verify operator can be installed in Tercen

**Status**: Disabled by default (`if: false`)

**Why Disabled**:
- Requires active Tercen test environment
- Needs secrets configuration
- Only useful once operator is functional

**Enable When**:
- Operator implementation is complete
- Test environment is available
- Secrets are configured in GitHub

**Required Secrets**:
- `TERCEN_TEST_OPERATOR_USERNAME`: Tercen test user
- `TERCEN_TEST_OPERATOR_PASSWORD`: Tercen test password
- `TERCEN_TEST_OPERATOR_URI`: Tercen test instance URL

---

## GitHub Container Registry Setup

### Authentication

**For Developers** (local builds):
```bash
# Create GitHub personal access token with read:packages, write:packages
export GITHUB_TOKEN=ghp_xxxxxxxxxxxxx

echo $GITHUB_TOKEN | docker login ghcr.io -u YOUR_GITHUB_USERNAME --password-stdin
```

**In CI/CD** (automatic):
```yaml
- uses: docker/login-action@v3
  with:
    registry: ghcr.io
    username: ${{ github.actor }}
    password: ${{ secrets.GITHUB_TOKEN }}
```

### Pull Images

**Public Repository** (no auth required):
```bash
docker pull ghcr.io/tercen/ggrs_plot_operator:latest
```

**Private Repository** (auth required):
```bash
# After login
docker pull ghcr.io/tercen/ggrs_plot_operator:latest
```

### Push Images

**Manual Push** (after local build):
```bash
# Build locally
docker build -t ghcr.io/tercen/ggrs_plot_operator:test .

# Push (requires authentication)
docker push ghcr.io/tercen/ggrs_plot_operator:test
```

**Automatic Push** (CI/CD):
- Only on push to branches (not PRs)
- Uses `GITHUB_TOKEN` automatically
- No manual configuration needed

---

## Local Development

### Building the Docker Image

```bash
# Standard build
docker build -t ggrs_plot_operator:local .

# Build with cache from registry
docker build \
  --cache-from ghcr.io/tercen/ggrs_plot_operator:latest \
  -t ggrs_plot_operator:local .

# Build without cache
docker build --no-cache -t ggrs_plot_operator:local .
```

### Running the Container

```bash
# Basic run
docker run --rm \
  -e TERCEN_ENDPOINT=https://tercen.com:5400 \
  -e TERCEN_USERNAME=your_username \
  -e TERCEN_PASSWORD=your_password \
  ggrs_plot_operator:local

# With volume mount (for logs)
docker run --rm \
  -e TERCEN_ENDPOINT=https://tercen.com:5400 \
  -e TERCEN_USERNAME=your_username \
  -e TERCEN_PASSWORD=your_password \
  -v $(pwd)/logs:/app/logs \
  ggrs_plot_operator:local

# Interactive shell (for debugging)
docker run --rm -it \
  --entrypoint /bin/bash \
  ggrs_plot_operator:local
```

### Testing jemalloc

Verify jemalloc is active:

```bash
# Run container with shell
docker run --rm -it --entrypoint /bin/bash ggrs_plot_operator:local

# Inside container, check LD_PRELOAD
echo $LD_PRELOAD
# Output: /usr/lib/x86_64-linux-gnu/libjemalloc.so.2

# Check malloc conf
echo $MALLOC_CONF
# Output: background_thread:true,metadata_thp:auto,dirty_decay_ms:30000,muzzy_decay_ms:30000

# Verify jemalloc is loaded
ldd /usr/local/bin/ggrs_plot_operator | grep jemalloc
# Output should show libjemalloc.so.2
```

### Memory Profiling with jemalloc

jemalloc includes built-in profiling:

```bash
# Enable profiling
export MALLOC_CONF=prof:true,prof_prefix:/tmp/jeprof

# Run application
docker run --rm \
  -e MALLOC_CONF=prof:true,prof_prefix:/tmp/jeprof \
  -v $(pwd)/profiles:/tmp \
  ggrs_plot_operator:local

# Analyze with jeprof (requires installation)
jeprof --show_bytes --pdf /usr/local/bin/ggrs_plot_operator /tmp/jeprof.*.heap > profile.pdf
```

---

## Cargo Features

### jemalloc Feature (Default)

Enabled by default in `Cargo.toml`:

```toml
[features]
default = ["jemalloc"]
jemalloc = ["tikv-jemallocator"]
```

**Enable**: `cargo build --release` (default)
**Disable**: `cargo build --release --no-default-features`

**When to Disable**:
- Development on platforms without jemalloc (Windows, some BSDs)
- Debugging memory issues (system allocator has better tools)
- Profiling with valgrind (jemalloc interferes)

### Build Profiles

**Development** (`cargo build`):
- Fast compilation
- No optimizations
- Debug symbols included
- Quick iteration

**Release** (`cargo build --release`):
- Full optimizations
- Size optimized (opt-level = "z")
- LTO enabled
- Symbols stripped
- For production deployment

**MaxPerf** (`cargo build --profile maxperf`):
- Maximum runtime performance
- Speed optimized (opt-level = 3)
- Fat LTO
- Larger binary size
- For benchmarking

---

## CI/CD Configuration

### Required GitHub Settings

**Repository Secrets** (for operator testing, optional):
- `TERCEN_TEST_OPERATOR_USERNAME`
- `TERCEN_TEST_OPERATOR_PASSWORD`
- `TERCEN_TEST_OPERATOR_URI`

**Repository Settings**:
- Enable GitHub Actions
- Enable GitHub Container Registry
- Enable vulnerability alerts
- Enable Dependabot

**Branch Protection** (recommended):
- Require status checks to pass
- Require branches to be up to date
- Require PR reviews before merging

### Workflow Permissions

Required permissions for CI workflow:

```yaml
permissions:
  contents: read        # Read repository code
  packages: write       # Push to ghcr.io
  security-events: write # Upload security scan results
```

These are configured per-job in the workflow.

### Triggering Workflows

**Manual Trigger**:
- Go to Actions tab in GitHub
- Select workflow
- Click "Run workflow"
- Choose branch

**Automatic Trigger**:
- Push to main/master/develop
- Create pull request
- Push tag (git tag v1.0.0 && git push --tags)

---

## Troubleshooting

### Build Failures

**Issue**: `error: linking with cc failed`
**Solution**: Ensure all system dependencies are installed in builder stage

**Issue**: `error: failed to compile`
**Solution**: Check Rust version compatibility (requires 1.75+)

**Issue**: `proto file not found`
**Solution**: Ensure protos/ directory is in Docker context (not in .dockerignore)

### Runtime Failures

**Issue**: `cannot open shared object file: libjemalloc.so.2`
**Solution**: Verify libjemalloc2 is installed in runtime stage

**Issue**: `permission denied`
**Solution**: Check file permissions, ensure running as correct user

**Issue**: `connection refused to Tercen`
**Solution**: Verify network connectivity, check TERCEN_ENDPOINT

### CI/CD Issues

**Issue**: `Error: buildx failed with: error: denied: permission_denied`
**Solution**: Verify GITHUB_TOKEN has packages:write permission

**Issue**: `Error: failed to solve: process "/bin/sh -c cargo build"`
**Solution**: Check build logs, likely missing dependency or Cargo.lock issue

**Issue**: Cache not working
**Solution**: Ensure Cargo.lock is committed to repository

---

## Performance Benchmarks

Expected performance characteristics:

**Image Size**:
- Target: < 150 MB
- Baseline (no optimizations): ~300 MB
- Improvement: 50% reduction

**Build Time** (GitHub Actions):
- Cold cache: ~8-12 minutes
- Warm cache: ~3-5 minutes
- Improvement: 60-70% faster with cache

**Memory Usage** (runtime):
- With jemalloc: 20-30% less fragmentation
- Peak memory: Depends on dataset size
- Baseline: System allocator

**Startup Time**:
- Container start: < 1 second
- Tercen connection: 1-2 seconds
- Total ready time: < 5 seconds

---

## Security Best Practices

1. **Never commit secrets**: Use GitHub Secrets or environment variables
2. **Regular updates**: Update base images and dependencies monthly
3. **Scan images**: Review Trivy results before production deployment
4. **Minimal attack surface**: Keep runtime image as small as possible
5. **Non-root execution**: Always run as unprivileged user
6. **Network isolation**: Use Docker networks to restrict access
7. **Resource limits**: Set memory/CPU limits in production

---

## Next Steps

1. **Phase 0**: Test Docker build locally after implementing Cargo project
2. **Phase 1**: Enable CI workflow after first commit
3. **Phase 8**: Enable security scanning before production deployment
4. **Phase 9**: Configure operator installation test when operator is functional

---

## References

- [Docker Multi-Stage Builds](https://docs.docker.com/build/building/multi-stage/)
- [GitHub Actions for Rust](https://github.com/actions-rs)
- [GitHub Container Registry](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry)
- [jemalloc Documentation](https://github.com/jemalloc/jemalloc/wiki)
- [tikv-jemallocator](https://docs.rs/tikv-jemallocator/)
- [Trivy Scanner](https://aquasecurity.github.io/trivy/)
- [Cargo Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)
