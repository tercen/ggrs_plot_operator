# Docker and CI/CD Updates

## Final CI/CD Configuration

The workflow is now based on the Tercen reference: [model_estimator CI](https://github.com/tercen/model_estimator/blob/main/.github/workflows/ci.yml)

## Changes Made Based on Feedback

### 1. CI/CD Workflow Triggers (Now Matches Tercen Pattern)

**Before**:
- Complex trigger conditions
- Separate handling for PRs, tags, and branches
- Custom semver tag patterns

**After** (following Tercen model_estimator pattern):
- ✅ **Push to any branch** triggers tests
- ✅ **Push to main branch** triggers tests + Docker build
- ✅ **Automatic tagging** via `docker/metadata-action` (handles branch names, tags, etc.)
- ✅ **Single workflow file** with two jobs: `test` and `build-and-push-image`

**Location**: `.github/workflows/ci.yml`

```yaml
on:
  push:
    branches: ['main', '*']
```

**Behavior**:
- Tests run on ALL branch pushes
- Docker images built and pushed ONLY on main branch pushes
- Uses `docker/metadata-action` for automatic tag generation

### 2. Docker Image Tagging Strategy (Automatic via Metadata Action)

**Before**:
- Manual tag configuration
- Complex pattern matching
- Multiple tag formats

**After** (following Tercen pattern):
- ✅ **Automatic tagging** via `docker/metadata-action`
- ✅ **No manual tag configuration** - metadata action handles it
- ✅ **Standard Docker conventions** applied automatically

**Location**: `.github/workflows/ci.yml`

```yaml
- name: Extract metadata (tags, labels) for Docker
  id: meta
  uses: docker/metadata-action@v5.5.1
  with:
    images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
```

**Generated Tags** (by metadata action):
- Push to main → `main` tag
- Push tag `0.1.0` → `0.1.0` tag
- Automatic labels for org.opencontainers.image.* metadata

### 3. Multi-Stage Docker Build (Already Correct)

**Confirmation**: The Dockerfile already uses a proper multi-stage build:
- ✅ **Builder stage**: Contains Rust toolchain (rust:1.75-slim-bookworm)
- ✅ **Runtime stage**: Contains ONLY runtime libraries (debian:bookworm-slim)
- ✅ **Rust toolchain is NOT in final image** - discarded after build

**Size Breakdown**:
- Builder stage: ~1.2 GB (includes Cargo, rustc, build tools) - **NOT PUSHED**
- Runtime stage: ~120-150 MB (only app binary + runtime libs) - **PUSHED**

**Added comments** to Dockerfile clarifying:
- Builder stage doesn't make it to final image
- Runtime stage is minimal
- Can be replaced with custom base image later

### 4. Custom Base Image Support (Future)

**Added notes** in Dockerfile about replacing with custom base images:

```dockerfile
# NOTE: This can be replaced with a custom base image in the future
#       for faster builds and consistent dependencies across Tercen operators

# Runtime Stage
FROM debian:bookworm-slim
# NOTE: Can be replaced with tercen/rust-operator-base:latest or similar
```

**Benefits of custom base image**:
- Faster builds (pre-installed dependencies)
- Consistent versions across all Rust operators
- Shared layer caching
- Centralized dependency management

**When to create custom base**:
- After this operator is stable
- When multiple Rust operators exist
- Include: jemalloc, SSL, protobuf, common Rust deps

## Tag Examples

### ✅ Correct Tag Format

```bash
# Create release
git tag -a 0.1.0 -m "Release version 0.1.0"
git push origin 0.1.0

# Result: Docker image tagged as ghcr.io/tercen/ggrs_plot_operator:0.1.0
```

```bash
# Push to main
git push origin main

# Result: Docker image tagged as ghcr.io/tercen/ggrs_plot_operator:main
```

### ❌ Incorrect Tag Format

```bash
# Don't use 'v' prefix
git tag v0.1.0  # WRONG

# Don't use partial versions
git tag 0.1     # WRONG (won't trigger workflow)

# Don't use pre-release tags
git tag 0.1.0-beta  # WRONG (won't trigger workflow)
```

## Pulling Images

```bash
# Latest development version
docker pull ghcr.io/tercen/ggrs_plot_operator:main

# Specific release version (no 'v' prefix)
docker pull ghcr.io/tercen/ggrs_plot_operator:0.1.0
docker pull ghcr.io/tercen/ggrs_plot_operator:1.2.3
```

## CI/CD Behavior Summary (Final)

| Event | Tests Run? | Docker Build? | Image Tags |
|-------|-----------|---------------|------------|
| Push to main | ✅ Yes | ✅ Yes | `main` (auto via metadata-action) |
| Push to feature branch | ✅ Yes | ❌ No | None (no build, only test) |
| Push tag `0.1.0` | ✅ Yes | ✅ Yes* | `0.1.0` (auto via metadata-action) |
| Pull request | ❌ No** | ❌ No | None |

\* Docker build only happens if tag push is to main or trigger is on main
\*\* Tests run on push, not on PR opening (different from some CI patterns)

## Documentation Updates

All documentation has been updated to reflect these changes:

1. **docs/04_DOCKER_AND_CICD.md**:
   - Updated trigger conditions
   - Updated tagging strategy table
   - Clarified multi-stage build discards Rust toolchain
   - Added custom base image notes

2. **docs/05_DOCKER_CICD_SUMMARY.md**:
   - Updated tagging strategy
   - Added semver format requirements
   - Updated trigger conditions

3. **BUILD.md**:
   - Updated release instructions
   - Emphasized no 'v' prefix
   - Added correct/incorrect examples

4. **Dockerfile**:
   - Added comments about custom base image
   - Clarified builder stage is discarded
   - Added note about future tercen/rust-operator-base

## Verification

To verify the multi-stage build works correctly:

```bash
# Build the image
docker build -t test .

# Check that Rust toolchain is NOT in final image
docker run --rm test which cargo
# Should return: exit code 1 (not found)

docker run --rm test which rustc
# Should return: exit code 1 (not found)

# Verify the binary is present
docker run --rm test ls -lh /usr/local/bin/ggrs_plot_operator
# Should show the binary (~20-30 MB)

# Check image size
docker images test
# Should show ~120-150 MB, not 1+ GB
```

## Migration Notes

If you need to migrate from 'v' prefixed tags:

```bash
# Old tags (if any exist)
git tag v0.1.0

# Create new tag without 'v'
git tag 0.1.0

# Push new tag
git push origin 0.1.0

# Optionally delete old tag
git tag -d v0.1.0
git push origin :refs/tags/v0.1.0
```

## Summary of Key Changes

✅ **CI structure**: Two jobs (test + build-and-push-image) following Tercen pattern
✅ **CI triggers**: Push to any branch runs tests; only main builds Docker
✅ **Automatic tagging**: Uses `docker/metadata-action` like Tercen reference
✅ **Action versions**: Updated to match Tercen reference (docker/build-push-action@v6.7.0, etc.)
✅ **Rust additions**: Added test job with Cargo caching and Rust-specific checks
✅ **Multi-stage build**: Already correct - Rust toolchain not in final image
✅ **Future-ready**: Comments added for custom base image migration
✅ **Documentation**: All docs updated with correct examples

The infrastructure is now fully aligned with Tercen operator conventions (model_estimator pattern) while adding Rust-specific testing.
