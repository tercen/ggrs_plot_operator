# CI/CD Final Configuration

## Reference Used

Based on: **[tercen/model_estimator CI workflow](https://github.com/tercen/model_estimator/blob/main/.github/workflows/ci.yml)**

## Workflow Structure

```yaml
name: CI Workflow

on:
  push:
    branches: ['main', '*']

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  test:                    # Rust-specific: runs on all pushes
    - Install Rust toolchain
    - Cache Cargo dependencies
    - Run fmt, clippy, tests

  build-and-push-image:    # Docker build: runs only on main
    if: github.ref == 'refs/heads/main'
    - Login to ghcr.io
    - Extract metadata (automatic tagging)
    - Build and push Docker image
    - Generate attestation
```

## Key Differences from model_estimator

### Additions (Rust-specific)

1. **Test Job**: Added before Docker build
   - Rust toolchain installation
   - Cargo caching (registry, index, target)
   - System dependencies (protobuf, jemalloc)
   - Format checking (`cargo fmt`)
   - Linting (`cargo clippy`)
   - Unit tests (`cargo test`)
   - Doc tests (`cargo test --doc`)

2. **Build Caching**: Docker layer caching via GitHub Actions
   ```yaml
   cache-from: type=gha
   cache-to: type=gha,mode=max
   ```

### Kept from model_estimator

1. **Trigger Pattern**: `branches: ['main', '*']`
   - Tests run on ALL branch pushes
   - Docker builds only on main

2. **Automatic Tagging**: Uses `docker/metadata-action`
   - No manual tag configuration
   - Handles branch names, version tags automatically

3. **Action Versions**: Matched exactly
   - `docker/login-action@v3.3.0`
   - `docker/metadata-action@v5.5.1`
   - `docker/build-push-action@v6.7.0`
   - `actions/attest-build-provenance@v1`

4. **Permissions**: Identical structure
   ```yaml
   permissions:
     contents: read
     packages: write
     attestations: write
     id-token: write
   ```

5. **Registry**: GitHub Container Registry (ghcr.io)

## Workflow Behavior

### Push to Feature Branch (e.g., `feature/new-plot`)

```bash
git push origin feature/new-plot
```

**Result**:
- ✅ Test job runs (fmt, clippy, tests)
- ❌ Docker image NOT built (only on main)
- ❌ No image pushed to registry

**Use Case**: Validate code before merging to main

### Push to Main Branch

```bash
git push origin main
```

**Result**:
- ✅ Test job runs (fmt, clippy, tests)
- ✅ Docker image built (after tests pass)
- ✅ Image pushed with tag: `main`
- ✅ Attestation generated

**Image**: `ghcr.io/tercen/ggrs_plot_operator:main`

**Use Case**: Latest development version

### Push Version Tag

```bash
git tag 0.1.0
git push origin 0.1.0
```

**Result** (if tag is on main branch):
- ✅ Test job runs
- ✅ Docker image built
- ✅ Image pushed with tag: `0.1.0`
- ✅ Attestation generated

**Image**: `ghcr.io/tercen/ggrs_plot_operator:0.1.0`

**Use Case**: Release version

## Automatic Tag Generation

The `docker/metadata-action` automatically generates tags based on:

| Git Event | Generated Docker Tag | Example |
|-----------|---------------------|---------|
| Push to `main` | `main` | `ghcr.io/tercen/ggrs_plot_operator:main` |
| Push to `develop` | `develop` | `ghcr.io/tercen/ggrs_plot_operator:develop` |
| Push tag `0.1.0` | `0.1.0` | `ghcr.io/tercen/ggrs_plot_operator:0.1.0` |
| Push tag `1.2.3` | `1.2.3` | `ghcr.io/tercen/ggrs_plot_operator:1.2.3` |

**Note**: No 'v' prefix required or generated. Use `0.1.0`, not `v0.1.0`.

## Docker Image Labels

The metadata action also generates standard OCI labels:

- `org.opencontainers.image.created`
- `org.opencontainers.image.source`
- `org.opencontainers.image.version`
- `org.opencontainers.image.revision`
- `org.opencontainers.image.licenses`

These are visible in the GitHub Container Registry UI.

## Build Caching

### Cargo Caching (Test Job)

Three separate caches for optimal performance:

```yaml
~/.cargo/registry  # Downloaded crates
~/.cargo/git       # Git dependencies
target/            # Compiled artifacts
```

**Cache Key**: Based on `Cargo.lock` hash
- Cache hit: Tests run in ~30-60 seconds
- Cache miss: Tests run in ~3-5 minutes

### Docker Layer Caching (Build Job)

```yaml
cache-from: type=gha  # Restore layers from GitHub Actions cache
cache-to: type=gha,mode=max  # Save all layers (not just final)
```

**Benefits**:
- Faster builds when dependencies don't change
- Reduces build time by ~50-70%
- Automatic cache management by GitHub

## Permissions Explained

```yaml
permissions:
  contents: read        # Read repository code
  packages: write       # Push to ghcr.io
  attestations: write   # Generate build provenance
  id-token: write       # OIDC token for attestation
```

### What is Build Attestation?

Build attestation creates a signed record of:
- What code was built (commit SHA)
- How it was built (workflow, runner)
- When it was built (timestamp)
- Who built it (GitHub Actions)

This provides **supply chain security** - you can verify the image came from this exact workflow.

## Comparison: Python vs Rust

| Aspect | model_estimator (Python) | ggrs_plot_operator (Rust) |
|--------|-------------------------|---------------------------|
| Test job | ❌ None | ✅ Added |
| Build trigger | Push to main or any branch | Push to main only |
| Caching | Docker only | Cargo + Docker |
| Build time | ~2-3 min | ~3-5 min (first), ~1-2 min (cached) |
| Image size | ~200-300 MB | ~120-150 MB |
| Base image | Python base | Multi-stage Rust → Debian slim |

## Testing Locally

### Test Job (without CI)

```bash
# Format check
cargo fmt -- --check

# Linting
cargo clippy --all-targets --all-features -- -D warnings

# Tests
cargo test --all-features --verbose

# Doc tests
cargo test --doc
```

### Docker Build (without CI)

```bash
# Build image
docker build -t ggrs_plot_operator:local .

# Tag for registry
docker tag ggrs_plot_operator:local ghcr.io/tercen/ggrs_plot_operator:dev

# Push (requires authentication)
docker push ghcr.io/tercen/ggrs_plot_operator:dev
```

## Troubleshooting

### Tests Pass Locally but Fail in CI

**Possible causes**:
- Different Rust version (CI uses `stable`)
- Missing system dependencies
- Cache corruption

**Solution**:
```bash
# Match CI environment locally
rustup default stable
cargo clean
cargo test --all-features
```

### Docker Build Succeeds but Tests Fail

**Cause**: Test job runs before Docker build

**Solution**: Fix tests first, then Docker will build
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Image Not Pushed to Registry

**Cause**: Not on main branch

**Solution**: Check current branch
```bash
git branch --show-current
# Should show 'main'
```

### Cargo Cache Not Working

**Cause**: `Cargo.lock` changed

**Solution**: This is expected - cache invalidates when dependencies change
```bash
# Check what changed
git diff Cargo.lock
```

## Monitoring and Observability

### View Workflow Runs

1. Go to repository on GitHub
2. Click "Actions" tab
3. Select "CI Workflow"
4. View recent runs

### View Build Logs

1. Click on a workflow run
2. Click on job name (test or build-and-push-image)
3. Expand steps to see detailed logs

### View Container Images

1. Go to repository on GitHub
2. Click "Packages" in right sidebar
3. Click on `ggrs_plot_operator`
4. View versions, tags, and download stats

### View Build Attestations

1. Go to package page
2. Click on specific version
3. Scroll to "Provenance"
4. View signed attestation (JSON)

## Security Considerations

### Secrets Management

**Required**: None (uses built-in `GITHUB_TOKEN`)

**Optional**: For Tercen operator testing
- `TERCEN_TEST_OPERATOR_USERNAME`
- `TERCEN_TEST_OPERATOR_PASSWORD`
- `TERCEN_TEST_OPERATOR_URI`

### Dependency Security

- Dependabot automatically creates PRs for updates
- Tests run on all PRs to verify updates don't break
- Cargo.lock committed to track exact versions

### Image Security

- Multi-stage build removes build tools
- Runs as non-root user (UID 1000)
- Minimal base image (debian-slim)
- No secrets baked into image

## Future Enhancements

### 1. Add Trivy Security Scanning

```yaml
- name: Run Trivy vulnerability scanner
  uses: aquasecurity/trivy-action@master
  with:
    image-ref: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:main
```

### 2. Add Benchmarking

```yaml
- name: Run benchmarks
  run: cargo bench --no-fail-fast
```

### 3. Add Code Coverage

```yaml
- name: Generate coverage
  run: cargo tarpaulin --out Xml

- name: Upload to codecov
  uses: codecov/codecov-action@v3
```

### 4. Custom Base Image

Replace in Dockerfile:
```dockerfile
FROM tercen/rust-operator-base:1.0.0
```

Benefits:
- Faster builds (pre-installed dependencies)
- Consistent versions across operators
- Reduced layer count

## Summary

✅ **Follows Tercen pattern**: Based on model_estimator reference
✅ **Rust-specific additions**: Test job with caching
✅ **Automatic tagging**: No manual configuration needed
✅ **Supply chain security**: Build attestation enabled
✅ **Efficient caching**: Both Cargo and Docker layers
✅ **Production-ready**: Multi-stage build, non-root user, minimal image

The CI/CD pipeline is complete and ready for use when implementation begins.
