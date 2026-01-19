# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with Tercen's gRPC API. It receives tabular data from Tercen, generates high-performance plots with faceting and colors, and returns PNG images for visualization.

**Current Version**: 0.0.3-dev
**Status**: Production-ready (EventService logging disabled due to infrastructure limitation)

---

## Essential Commands

```bash
# Build (use dev-release for faster iteration)
cargo build --profile dev-release     # 4-5 min builds
cargo build --release                 # 12+ min builds (production only)

# Quality Checks (MANDATORY before considering code complete!)
cargo fmt && cargo fmt --check
cargo clippy -- -D warnings
cargo test

# Local Testing
./test_local.sh

# Test with specific workflow (requires env vars from test_local.sh)
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAi..."
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="b9659735-27db-4480-b398-4e391431480f"
timeout 45 cargo run --profile dev-release --bin test_stream_generator

# Docker
docker build -t ggrs_plot_operator:local .

# Releases (NO 'v' prefix!)
git tag 0.1.0 && git push origin 0.1.0
```

---

## Architecture

### Three-Layer Design

1. **Tercen gRPC Client** (`src/tercen/`) - Connection, auth, streaming
   - Uses tonic (gRPC), prost (protobuf), tokio (async runtime)
   - Proto files from `tercen_grpc_api` git submodule

2. **Data Transform Layer** - TSON → Polars DataFrame
   - **CRITICAL**: Stay columnar, never iterate row-by-row
   - 10x+ performance vs row-by-row approaches

3. **GGRS Integration** (`src/ggrs_integration/`) - TercenStreamGenerator
   - Implements GGRS StreamGenerator trait
   - Lazy facet loading, GPU/CPU rendering
   - Global data cache for pagination optimization (using `once_cell`)

### Module Structure

```
src/
├── main.rs                      # Entry point (logging disabled)
├── config.rs                    # Property-based configuration
├── tercen/                      # Pure Tercen gRPC client
│   ├── client.rs               # TercenClient with auth
│   ├── table.rs                # TableStreamer (chunked streaming)
│   ├── tson_convert.rs         # TSON → Polars DataFrame
│   ├── facets.rs               # Facet metadata loading
│   ├── pages.rs                # Page metadata for pagination
│   ├── properties.rs           # PropertyReader, PlotDimension
│   ├── colors.rs               # Color palette & interpolation
│   ├── result.rs               # Result upload to Tercen
│   └── error.rs                # TercenError types
├── ggrs_integration/
│   └── stream_generator.rs     # TercenStreamGenerator + global cache
└── bin/
    └── test_stream_generator.rs # Test binary

tests/
└── test_pagination_synthetic.rs # Pagination tests

tercen_grpc_api/                # Git submodule (proto files)
```

### Data Flow

```
1. TercenStreamGenerator::new()
   → Connect via gRPC
   → Load facet metadata
   → Compute Y-axis ranges
   → Load color info
   → Initialize global cache (if pagination)

2. GGRS calls query_data_chunk(col_idx, row_idx)
   → Check global DATA_CACHE for chunk
   → If MISS: Stream TSON chunks from Tercen, store in cache
   → If HIT: Read from cache (no network call)
   → Parse to Polars DataFrame (columnar!)
   → Filter by .ci/.ri (facet indices)
   → Add .color column
   → Return quantized coords (.xs/.ys as i64)

3. GGRS render pipeline
   → Dequantize (.xs/.ys) → (.x/.y)
   → Render with GPU (OpenGL) or CPU (Cairo)
   → Draw legend (if colors present)
   → Generate PNG

4. Result upload
   → Encode PNG to base64
   → Upload via FileService
   → Update task with fileResultId
```

---

## Core Technical Decisions

### 1. Columnar Architecture (CRITICAL!)

**Never build row-by-row structures. Always stay columnar.**

```rust
// ✅ GOOD: Columnar operations
let filtered = df.lazy()
    .filter(col(".ci").eq(lit(0)))
    .collect()?;

// ❌ BAD: Row-by-row iteration
for row in 0..df.height() {
    let record = build_record(df, row); // NO!
}
```

**Why**: 10x+ performance, lower memory, aligns with Polars/GGRS architecture.

### 2. NO FALLBACK STRATEGIES

**Never implement fallback logic unless explicitly requested.**

```rust
// ❌ BAD: Fallback pattern
if data.has_column(".ys") {
    use_ys()
} else {
    use_y()  // Masks bugs!
}

// ✅ GOOD: Trust the specification
data.column(".ys")  // User said .ys exists
```

**Rationale**: Fallbacks mask bugs, add complexity, hurt performance. Only use for backward compatibility or explicit user input validation.

### 3. Memory Efficiency Strategy

- **Streaming**: Process in chunks (default: 10K rows)
- **Lazy Faceting**: Only load data for cells being rendered
- **Schema-Based Limiting**: Use table schema row count to prevent infinite loops
- **Quantized Coordinates**: Transmit 2 bytes/coordinate (uint16), dequantize on demand
- **Progressive Processing**: Discard chunks immediately after processing
- **Global Cache**: Transparent caching for pagination (37% performance improvement)

**Results**: 475K rows in 0.5s (GPU) or 3.1s (CPU), memory stable at 162MB (GPU) / 49MB (CPU).

### 4. GPU vs CPU Backend

- **Configuration**: Property `backend` = "cpu" or "gpu" in `operator.json`
- **OpenGL selected**: 162MB vs Vulkan's 314MB (49% reduction)
- **Performance**: 10x speedup vs CPU for same quality
- **Trade-off**: 3.3x memory overhead acceptable for 10x speed

### 5. Pagination Optimization

**Global Data Cache** (`src/ggrs_integration/stream_generator.rs:15-27`):
- Static cache using `once_cell::Lazy<Mutex<HashMap>>`
- Caches raw TSON bytes keyed by `(table_id, offset)`
- Transparent to callers (zero API changes)
- **Performance**: Reduces pagination overhead from 71% to ~37% speedup
- **How it works**: First page streams from Tercen → cache. Subsequent pages read from cache (no network call)

---

## Features

### Current Capabilities (0.0.3)

**Plotting**:
- Full pipeline: gRPC → TSON → Polars → GGRS → PNG
- GPU acceleration (OpenGL) or CPU (Cairo)
- Row/column/grid faceting with independent Y-axes
- Quantized coordinates with dequantization
- Pagination support with global caching

**Colors**:
- **Continuous**: Numeric factors with palette interpolation
- **Categorical**: String factors with explicit color mappings
- **Level-based**: `.colorLevels` integer indices (0-7) with default palette

**Legend**:
- GGRS infrastructure: draw_continuous_legend(), draw_discrete_legend()
- Theme-aware styling
- Configurable positioning: right, left, top, bottom, inside, none
- Adjustable justification (anchor point) for fine-tuning
- ⚠️ **Known issue**: Category names not loaded for `.colorLevels` colors

**Configuration**:
- Property-based config from `operator.json`
- Auto plot dimensions: `800px + (n_facets-1) × 400px`, capped at 4000px
- Backend selection (cpu/gpu)
- Legend positioning and justification

**Known Limitations**:
- EventService logging disabled (returns UnimplementedError in production)
- Point size hardcoded to 4 (should come from crosstab aesthetics)
- Category names missing for `.colorLevels` in legends

### Roadmap

**0.0.4**: Themes (minimal, white), bulk streaming optimization
**0.0.5**: Bar/line plots, manual axis ranges
**0.0.6**: Heatmaps, configurable text elements (axis labels, legend, title)

---

## Development Workflow

### Pre-Commit Checklist (MANDATORY!)

```bash
# 1. Format
cargo fmt

# 2. Lint (zero warnings required)
cargo clippy -- -D warnings

# 3. Build
cargo build --profile dev-release

# 4. Test
cargo test
```

**NEVER consider code complete until all checks pass.** CI will fail otherwise.

### Testing Workflow

**Local testing with test script**:
```bash
./test_local.sh
```

**Manual testing with environment variables**:
```bash
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAi..."
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="b9659735-27db-4480-b398-4e391431480f"
cargo run --profile dev-release --bin test_stream_generator
```

**Pagination testing**:
```bash
# Use test_pagination_synthetic.rs or create workflow with Page factors
cargo test test_pagination_synthetic
```

### Proto Files (Submodule)

```bash
# Setup
git submodule update --init --recursive

# Update
cd tercen_grpc_api && git pull origin main
```

Proto files compiled at build time via `build.rs`.

---

## Operator Properties

Properties defined in `operator.json`, read via `PropertyReader`:

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `plot.width` | String | `""` (auto) | Plot width in pixels or "auto" |
| `plot.height` | String | `""` (auto) | Plot height in pixels or "auto" |
| `backend` | Enum | `"cpu"` | Render backend: "cpu" or "gpu" |
| `legend.position` | Enum | `"right"` | Legend position: right, left, top, bottom, inside, none |
| `legend.position.inside` | String | `""` | Coordinates for inside position: "x,y" where x,y ∈ [0,1] |
| `legend.justification` | String | `""` | Legend anchor point: "x,y" where x,y ∈ [0,1] |

**Auto Dimensions**: `800px + (n_facets - 1) × 400px`, capped at 4000px

---

## Color System

### Three Color Types

1. **Continuous Colors** (numeric factors):
   - Palette with numeric stops: `[0.0, 0.5, 1.0]`
   - Interpolation via `interpolate_color()`
   - Returns gradient based on normalized value

2. **Categorical Colors** (string factors):
   - Explicit mappings: `{category → "#RRGGBB"}`
   - Direct lookup in CategoryColorMap

3. **Level-Based Colors** (`.colorLevels` integers):
   - Default 8-color palette
   - Indices (0-7) map to palette positions
   - ⚠️ **Issue**: Category names not loaded from color tables

### Implementation

- `src/tercen/colors.rs`: Palette parsing, interpolation, category mapping
- `src/ggrs_integration/stream_generator.rs`: Color column addition
- Color data: Stored in `color_table_ids` during init

---

## Key Dependencies

```toml
tokio = "1.49"              # Async runtime
tonic = "0.14"              # gRPC client
prost = "0.14"              # Protobuf
polars = "0.51"             # Columnar DataFrame (CRITICAL!)
rustson = { git = "..." }   # TSON parsing
ggrs-core = { path = "../ggrs/crates/ggrs-core",
              features = ["webgpu-backend", "cairo-backend"] }
once_cell = "1.20"          # Global cache for pagination
thiserror = "1.0"           # Error macros
anyhow = "1.0"              # Error context
base64 = "0.22"             # PNG encoding
```

**Note**: GGRS uses local path for development. Switch to git dependency for CI/production.

---

## Documentation

### Primary Docs
- **CLAUDE.md** (this file) - Project overview and guidelines
- **BUILD.md** - Comprehensive build instructions
- **TEST_LOCAL.md** - Local testing procedures
- **DEPLOYMENT_DEBUG.md** - Known issues and debugging

### Session Documentation
- `SESSION_2025-01-18_PAGINATION_OPTIMIZATION.md` - Global cache implementation
- `SESSION_2025-01-16_PAGES_DEBUG.md` - Pagination debugging notes
- Check for other `SESSION_*.md` files for recent development context

### Supporting Docs
- `docs/09_FINAL_DESIGN.md` - Complete architecture
- `docs/10_IMPLEMENTATION_PHASES.md` - Implementation roadmap
- `docs/GPU_BACKEND_MEMORY.md` - GPU optimization analysis

### External Resources
- [Tercen gRPC API](https://github.com/tercen/tercen_grpc_api)
- [GGRS Library](https://github.com/tercen/ggrs)
- [Tercen C# Client](https://github.com/tercen/TercenCSharpClient) - Reference implementation

---

## Common Issues

### Build Issues

**Slow builds?** Use dev-release profile:
```bash
cargo build --profile dev-release  # 4-5 min vs 12+ min for release
```

**Proto files missing?**
```bash
git submodule update --init --recursive
```

### Runtime Issues

**Not connecting?**
- Check `TERCEN_URI`, `TERCEN_TOKEN` env vars
- Verify token from `test_local.sh`

**Faceting issues?**
- Verify `.ci`/`.ri` columns exist
- Check facet metadata tables

**Legend not showing?**
- Check `query_legend_scale()` returns Continuous/Discrete (not None)
- See known issue: Category names not loaded for `.colorLevels` colors

**Cache debugging** (pagination):
- Look for `DEBUG: Cache HIT/MISS` messages in output
- Cache is in `stream_generator.rs:15-27`
- First page always misses, subsequent pages should hit

---

## Notes for Claude Code

### Git Policy
- ❌ Never commit/push unless explicitly requested
- ✅ Run quality checks: fmt, clippy, build, test
- ✅ Use `git status` and `git diff` to show changes
- ✅ Create commits only when user explicitly asks

**Default behavior: User handles commits/pushes manually.**

### Release Tags
- Use semver format **WITHOUT 'v' prefix**: `0.1.0`, `1.2.3`
- Example: `git tag 0.1.0 && git push origin 0.1.0`

### Code Completion Checklist
Before reporting a task as complete:
1. ✅ Code formatted: `cargo fmt`
2. ✅ No clippy warnings: `cargo clippy -- -D warnings`
3. ✅ Builds successfully: `cargo build --profile dev-release`
4. ✅ Tests pass: `cargo test`

### Session Documentation
When working on complex features:
- Check for `SESSION_*.md` files for recent development context
- Create new session docs for significant multi-day work
- Update CLAUDE.md roadmap section when features are complete
