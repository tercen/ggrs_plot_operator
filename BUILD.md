# Build and Test Guide

Quick reference for building, testing, and deploying the GGRS Plot Operator.

## Prerequisites

### System Requirements

- Rust 1.75 or later
- Docker 20.10 or later
- Protocol Buffer compiler (protoc)

### Install Dependencies

**Ubuntu/Debian**:
```bash
sudo apt-get update
sudo apt-get install -y \
  pkg-config \
  libssl-dev \
  protobuf-compiler \
  libjemalloc-dev \
  build-essential
```

**macOS**:
```bash
brew install protobuf jemalloc openssl
```

**Windows** (WSL2 recommended):
```bash
# Use Ubuntu/Debian instructions in WSL2
```

## Local Development

### Build

```bash
# Debug build (fast compilation)
cargo build

# Release build (optimized)
cargo build --release

# Without jemalloc (for platforms without support)
cargo build --release --no-default-features

# Maximum performance build
cargo build --profile maxperf
```

### Test

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run doc tests
cargo test --doc

# Run ignored tests (integration tests)
cargo test -- --ignored

# Run with coverage (requires cargo-tarpaulin)
cargo tarpaulin --out Html
```

### Lint and Format

```bash
# Check formatting
cargo fmt -- --check

# Format code
cargo fmt

# Run clippy
cargo clippy

# Run clippy with all features
cargo clippy --all-targets --all-features
```

### Run Locally

```bash
# Set environment variables
export TERCEN_ENDPOINT=https://tercen.com:5400
export TERCEN_USERNAME=your_username
export TERCEN_PASSWORD=your_password

# Run debug build
cargo run

# Run release build
cargo run --release

# With arguments
cargo run -- --endpoint https://tercen.com:5400 --username user --password pass
```

## Docker

### Build Image

```bash
# Standard build
docker build -t ggrs_plot_operator:local .

# Build with specific tag
docker build -t ghcr.io/tercen/ggrs_plot_operator:dev .

# Build with no cache
docker build --no-cache -t ggrs_plot_operator:local .
```

### Run Container

```bash
# Basic run
docker run --rm \
  -e TERCEN_ENDPOINT=https://tercen.com:5400 \
  -e TERCEN_USERNAME=your_username \
  -e TERCEN_PASSWORD=your_password \
  ggrs_plot_operator:local

# With logs
docker run --rm \
  -e TERCEN_ENDPOINT=https://tercen.com:5400 \
  -e TERCEN_USERNAME=your_username \
  -e TERCEN_PASSWORD=your_password \
  -e RUST_LOG=debug \
  ggrs_plot_operator:local

# Interactive shell
docker run --rm -it \
  --entrypoint /bin/bash \
  ggrs_plot_operator:local
```

### Test Docker Build

```bash
# Build
docker build -t ggrs_plot_operator:test .

# Check image size
docker images ggrs_plot_operator:test

# Inspect layers
docker history ggrs_plot_operator:test

# Run security scan
docker run --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  aquasec/trivy image ggrs_plot_operator:test
```

## GitHub Container Registry

### Authentication

```bash
# Create token at https://github.com/settings/tokens
# Required scopes: read:packages, write:packages

# Login
echo $GITHUB_TOKEN | docker login ghcr.io -u YOUR_GITHUB_USERNAME --password-stdin
```

### Pull Images

```bash
# Pull latest
docker pull ghcr.io/tercen/ggrs_plot_operator:latest

# Pull specific version
docker pull ghcr.io/tercen/ggrs_plot_operator:1.0.0

# Pull development branch
docker pull ghcr.io/tercen/ggrs_plot_operator:develop
```

### Push Images

```bash
# Tag image
docker tag ggrs_plot_operator:local ghcr.io/tercen/ggrs_plot_operator:dev

# Push
docker push ghcr.io/tercen/ggrs_plot_operator:dev
```

## CI/CD

### Trigger Workflow

**Automatic**:
- Push to main/master/develop
- Create pull request
- Push version tag

**Manual**:
1. Go to repository on GitHub
2. Click "Actions" tab
3. Select "CI/CD Pipeline"
4. Click "Run workflow"
5. Choose branch

### Create Release

```bash
# Tag release (NO 'v' prefix - use semver format)
git tag -a 0.1.0 -m "Release version 0.1.0"

# Push tag
git push origin 0.1.0

# CI will automatically:
# - Build and test
# - Create Docker image with tag: 0.1.0
# - Run security scan
# - Generate release artifacts
```

**Important**: Use semver format WITHOUT 'v' prefix: `0.1.0`, `1.2.3`

### Check Build Status

1. Go to repository on GitHub
2. Click "Actions" tab
3. View recent workflow runs
4. Click run to see detailed logs

### View Container Images

1. Go to repository on GitHub
2. Click "Packages" in right sidebar
3. View published versions
4. Check download statistics

## Performance Profiling

### Memory Profiling (jemalloc)

```bash
# Enable profiling
export MALLOC_CONF=prof:true,prof_prefix:/tmp/jeprof

# Run application
cargo run --release

# Analyze profile (requires jeprof tool)
jeprof --show_bytes --pdf target/release/ggrs_plot_operator /tmp/jeprof.*.heap > profile.pdf
```

### CPU Profiling (perf)

```bash
# Record performance data
perf record --call-graph=dwarf cargo run --release

# View report
perf report

# Generate flamegraph (requires cargo-flamegraph)
cargo flamegraph
```

### Benchmarking

```bash
# Run benchmarks (when implemented)
cargo bench

# Run specific benchmark
cargo bench benchmark_name

# Compare benchmarks
cargo bench --save-baseline before
# Make changes
cargo bench --baseline before
```

## Troubleshooting

### Build Issues

**Proto files not found**:
```bash
# Ensure proto files are in place
ls -la protos/

# If missing, copy from sci repository
cp -r ../sci/tercen_grpc/tercen_grpc_api/protos/ ./protos/
```

**Linking errors**:
```bash
# Install missing system libraries
sudo apt-get install libssl-dev pkg-config

# Check installed libraries
pkg-config --libs openssl
```

**Cargo.lock conflicts**:
```bash
# Update dependencies
cargo update

# Regenerate lock file
rm Cargo.lock
cargo build
```

### Runtime Issues

**Cannot connect to Tercen**:
```bash
# Test connectivity
curl -v https://tercen.com:5400

# Check DNS resolution
nslookup tercen.com

# Test with telnet
telnet tercen.com 5400
```

**Authentication failures**:
```bash
# Verify credentials
echo $TERCEN_USERNAME
echo $TERCEN_PASSWORD

# Test authentication manually
# (Use gRPC client tool or implement test)
```

**Memory issues**:
```bash
# Check jemalloc is loaded
ldd target/release/ggrs_plot_operator | grep jemalloc

# Monitor memory usage
/usr/bin/time -v cargo run --release

# Use smaller dataset for testing
```

### Docker Issues

**Build fails**:
```bash
# Check Docker version
docker --version

# Build with verbose output
docker build --progress=plain -t ggrs_plot_operator:debug .

# Check disk space
df -h
```

**Container crashes**:
```bash
# Check logs
docker logs <container_id>

# Run with debug logging
docker run --rm -e RUST_LOG=debug ggrs_plot_operator:local

# Check container resources
docker stats
```

## Development Workflow

### Feature Development

```bash
# 1. Create feature branch
git checkout -b feature/new-feature

# 2. Make changes
# ... edit files ...

# 3. Test locally
cargo test
cargo clippy
cargo fmt

# 4. Build Docker image
docker build -t ggrs_plot_operator:test .

# 5. Commit changes
git add .
git commit -m "Add new feature"

# 6. Push branch
git push origin feature/new-feature

# 7. Create pull request on GitHub
# CI will run automatically

# 8. After merge, CI builds and publishes
```

### Release Process

```bash
# 1. Update version in Cargo.toml
# 2. Update CHANGELOG.md
# 3. Commit changes
git add Cargo.toml CHANGELOG.md
git commit -m "Bump version to 1.0.0"

# 4. Create and push tag
git tag -a v1.0.0 -m "Release 1.0.0"
git push origin main
git push origin v1.0.0

# 5. CI builds and publishes automatically
# 6. Create GitHub release from tag
# 7. Add release notes
```

## Useful Commands

```bash
# Check Rust version
rustc --version
cargo --version

# Update Rust
rustup update

# Check for outdated dependencies
cargo outdated

# Update dependencies
cargo update

# Audit dependencies for vulnerabilities
cargo audit

# Generate documentation
cargo doc --open

# Clean build artifacts
cargo clean

# Check binary size
ls -lh target/release/ggrs_plot_operator

# Strip binary manually (already done in release)
strip target/release/ggrs_plot_operator

# Check dependencies tree
cargo tree

# Show feature flags
cargo tree --format "{p} {f}"
```

## Resources

- [Cargo Book](https://doc.rust-lang.org/cargo/)
- [Rust Documentation](https://doc.rust-lang.org/)
- [Docker Documentation](https://docs.docker.com/)
- [GitHub Actions Documentation](https://docs.github.com/actions)
- [jemalloc Documentation](https://github.com/jemalloc/jemalloc/wiki)
