# Release Workflow

**Status**: ✅ IMPLEMENTED (2025-01-09)

## Overview

The release workflow automates the process of creating releases, building Docker images with semantic version tags, and updating the operator.json file. It triggers on git tags following semantic versioning (e.g., `1.0.0`, `0.2.1`).

## How to Create a Release

### 1. Ensure All Changes Are Committed

```bash
git status  # Should show clean working directory
```

### 2. Create and Push a Semantic Version Tag

```bash
# Example: Create version 0.1.0
git tag 0.1.0
git push origin 0.1.0
```

**Important**: Use semantic versioning **without** a 'v' prefix:
- ✅ Correct: `0.1.0`, `1.0.0`, `2.3.1`
- ❌ Incorrect: `v0.1.0`, `v1.0.0`

### 3. Workflow Automatically Runs

The GitHub Actions workflow will:
1. Check out the repository with submodules
2. Update `operator.json` with the tagged version
3. Commit and push the operator.json changes
4. Build Docker image with the version tag
5. Push image to `ghcr.io/tercen/ggrs_plot_operator:<version>`
6. Generate changelog
7. Create GitHub release

## Workflow Details

### Trigger Pattern

```yaml
on:
  push:
    tags:
      - '[0-9]+.[0-9]+.[0-9]+'  # Matches X.Y.Z
```

### operator.json Update

The workflow updates the `container` field in `operator.json`:

```json
{
  "container": "ghcr.io/tercen/ggrs_plot_operator:0.1.0"
}
```

This ensures the operator.json always references the correct Docker image version.

### Docker Image Tagging

Images are tagged with the semantic version:
- `ghcr.io/tercen/ggrs_plot_operator:0.1.0`
- `ghcr.io/tercen/ggrs_plot_operator:1.0.0`

**Note**: The `main` tag continues to be updated on every push to the main branch via the CI workflow.

### Build Process

The workflow uses:
- **Docker Buildx**: Advanced build features
- **GitHub Actions cache**: Speeds up repeated builds
- **Build arguments**: Includes GH_PAT secret for private dependencies

## Comparison with CI Workflow

### CI Workflow (`.github/workflows/ci.yml`)

**Triggers**: Every push to any branch
**Purpose**: Continuous integration testing and development builds
**Tags**: Branch names (e.g., `main`, `feature-branch`)
**Steps**:
1. Run Rust tests (fmt, clippy, unit tests)
2. Build and push Docker image
3. Generate attestation

### Release Workflow (`.github/workflows/release.yml`)

**Triggers**: Push of semantic version tags
**Purpose**: Production releases
**Tags**: Semantic versions (e.g., `0.1.0`, `1.0.0`)
**Steps**:
1. Update operator.json
2. Commit and push changes
3. Build and push Docker image with version tag
4. Generate changelog
5. Create GitHub release

## What Gets Published

### Docker Image

Location: `ghcr.io/tercen/ggrs_plot_operator:<version>`

Includes:
- Rust binary built with `--profile dev-release`
- System dependencies (Cairo, Pango, jemalloc)
- Runtime environment (Debian bookworm-slim)

### GitHub Release

Includes:
- Release notes (auto-generated changelog)
- Tag reference
- Source code archive

### operator.json

Updated with:
- Container image reference with version tag
- Committed back to the tagged release

## Testing the Workflow

### Dry Run (Test Locally)

You can test parts of the workflow locally:

```bash
# Test jq command for operator.json update
TAG_VERSION="0.1.0"
jq --arg variable "ghcr.io/tercen/ggrs_plot_operator:${TAG_VERSION}" \
   '.container = $variable' operator.json > operator.json.tmp
cat operator.json.tmp
rm operator.json.tmp
```

### Create a Test Release

```bash
# Create a test tag
git tag 0.0.1-test
git push origin 0.0.1-test

# Watch the workflow
# Go to: https://github.com/tercen/ggrs_plot_operator/actions

# Clean up if needed
git tag -d 0.0.1-test
git push origin :refs/tags/0.0.1-test
gh release delete 0.0.1-test  # If release was created
```

## Troubleshooting

### operator.json Not Updated

**Issue**: operator.json changes not committed

**Solution**: Ensure the workflow has write permissions:
```yaml
permissions:
  contents: write
```

### Docker Build Fails

**Issue**: Missing dependencies or build errors

**Solution**:
1. Check Dockerfile is correct
2. Verify GH_PAT secret is set
3. Test Docker build locally:
   ```bash
   docker build --secret id=gh_pat,env=GH_PAT -t test:local .
   ```

### Tag Already Exists

**Issue**: Cannot create tag that already exists

**Solution**:
```bash
# Delete local and remote tag
git tag -d 0.1.0
git push origin :refs/tags/0.1.0

# Create new tag
git tag 0.1.0
git push origin 0.1.0
```

## Best Practices

1. **Test Before Tagging**: Ensure CI passes on main branch before creating release tag
2. **Semantic Versioning**: Follow semver rules:
   - MAJOR: Breaking changes
   - MINOR: New features (backward compatible)
   - PATCH: Bug fixes
3. **Changelog**: Write meaningful commit messages - they become the changelog
4. **Review**: Check the GitHub Actions run to ensure all steps succeed

## Future Enhancements

Potential improvements:
1. **Pre-release support**: Support `0.1.0-rc1` tags
2. **Automated versioning**: Auto-bump version based on commit messages
3. **Multi-platform builds**: Build for arm64 and amd64
4. **Release notes template**: Structured release note format
5. **Deployment notifications**: Slack/email notifications on release

## References

- GitHub Actions: https://docs.github.com/en/actions
- Docker Buildx: https://docs.docker.com/buildx/working-with-buildx/
- Semantic Versioning: https://semver.org/
