# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with Tercen's gRPC API. It receives tabular data from Tercen, generates high-performance plots with faceting and colors, and returns PNG images.

## Essential Commands

```bash
# Build (use dev-release for faster iteration)
cargo build --profile dev-release     # Faster builds with incremental compilation
cargo build --release                 # Production only (LTO enabled, slow)

# Quality Checks (MANDATORY before code is complete)
cargo fmt && cargo clippy -- -D warnings && cargo test

# Run specific test
cargo test test_name

# Local development with Tercen
export TERCEN_URI=http://127.0.0.1:50051
export TERCEN_TOKEN=your_token
export WORKFLOW_ID=your_workflow_id
export STEP_ID=your_step_id
cargo run --bin dev --profile dev-release

# Local testing script
./test_local.sh

# Proto submodule setup (required for gRPC definitions)
git submodule update --init --recursive
```

## Architecture

### Three-Layer Design

```
Tercen gRPC API
      ↓
[1] src/tercen/         → gRPC client, auth, streaming (tonic/prost)
      ↓
[2] TSON → Polars       → Columnar data transformation
      ↓
[3] src/ggrs_integration/ → Implements GGRS StreamGenerator trait
      ↓
ggrs-core library       → Plot rendering (../ggrs/crates/ggrs-core)
      ↓
PNG Output
```

### Data Flow Details

**Coordinate System**: Tercen sends quantized coordinates (`.xs`, `.ys` as uint16 0-65535). GGRS dequantizes to actual values (`.x`, `.y`) using per-facet axis ranges.

**Color Mapping**:
- Continuous: Factor column (f64) → palette interpolation → `.color` hex strings
- Categorical: `.colorLevels` (int32) → default palette → `.color` hex strings

**Pagination**: Page factors filter facets (not data). GGRS matches data to facets via `original_index` mapping.

### Key Modules

**Tercen Client** (`src/tercen/`)
- `client.rs` - TercenClient with gRPC auth
- `context/` - `TercenContext` trait + `ProductionContext`/`DevContext` implementations
- `table.rs` - TableStreamer for chunked data streaming
- `tson_convert.rs` - TSON → Polars DataFrame conversion
- `colors.rs` - Color palette extraction, `ChartKind` enum, `ColorInfo`
- `pages.rs` - Multi-page plot support
- `facets.rs` - Facet metadata handling

**GGRS Integration** (`src/ggrs_integration/`)
- `stream_generator.rs` - `TercenStreamGenerator` implements GGRS `StreamGenerator` trait
- `cached_stream_generator.rs` - Disk-cached version for multi-page plots

**Configuration**
- `src/config.rs` - `OperatorConfig` from `operator.json` properties
- `operator.json` - Operator property definitions (plot.width, plot.height, backend, legend.position, axis labels, tick rotation, etc.)

### Related Repository

The `ggrs-core` library at `../ggrs/crates/ggrs-core` is the plotting engine. Changes often span both repositories. Use local path for dev, switch to git dependency for CI.

### Chart-Type Driven Layout

Chart type (from Tercen UI) determines layout behavior via `ChartKind` enum:

| Aspect | Scatter/Line/Bar | Heatmap |
|--------|------------------|---------|
| Position columns | `.xs`, `.ys` (quantized u16) | `.ci`, `.ri` (grid indices) |
| Axis type | Continuous | Discrete (categorical labels) |
| Faceting | Yes (`.ci`/`.ri` → panels) | No (grid IS the plot) |
| Coordinate transform | Dequantize u16 → f64 | None (integers) |
| Scale expansion | 5% padding | 0.5 units (centers labels in tiles) |

**Reference**: R plot_operator (`main/plot_operator/utils.R`) for expected behavior.

### Component Responsibilities

**TercenStreamGenerator** (`src/ggrs_integration/stream_generator.rs`)
- Streams raw data from Tercen tables
- Provides facet metadata (cschema, rschema dimensions)
- Returns data with original columns
- **Does NOT know about chart types or layout**

**Pipeline** (`src/pipeline.rs`)
- Orchestrates plot generation across all pages
- Selects geom (tile vs point) based on ChartKind
- Configures theme, scales, and layout strategy

**TercenContext** (`src/tercen/context/`)
- Trait abstracting production vs development environments
- Key methods: `cube_query()`, `color_infos()`, `page_factors()`, `chart_kind()`, `point_size()`
- Extracts all workflow metadata needed for plot generation

## Core Technical Decisions

### 1. Columnar Architecture (CRITICAL)

**Never build row-by-row structures. Always stay columnar.**

```rust
// ✅ GOOD: Columnar operations
let filtered = df.lazy().filter(col(".ci").eq(lit(0))).collect()?;

// ❌ BAD: Row-by-row iteration
for row in 0..df.height() { build_record(df, row); }
```

### 2. No Fallback Strategies

**Never implement fallback logic unless explicitly requested.** Fallbacks mask bugs.

```rust
// ❌ BAD: Fallback pattern
if data.has_column(".ys") { use_ys() } else { use_y() }

// ✅ GOOD: Trust the specification
data.column(".ys")
```

### 3. Context Trait Pattern

The `TercenContext` trait abstracts production vs development environments:

```rust
// Both implement TercenContext
ProductionContext::from_task_id(client, task_id)  // Production
DevContext::new(client, workflow_id, step_id)      // Local development
```

## Key Dependencies

- `ggrs-core` - Local path `../ggrs/crates/ggrs-core` (switch to git for CI). Features: `webgpu-backend`, `cairo-backend`
- `polars` - Columnar DataFrame (critical for performance)
- `tonic`/`prost` - gRPC client (v0.14)
- `tokio` - Async runtime
- `rustson` - Tercen TSON binary format parsing

## Dev Config Override

For local testing, create `operator_config.json` with properties to override defaults:

```json
{
  "backend": "gpu",
  "plot.width": "800",
  "legend.position": "right"
}
```

## Notes for Claude Code

### Git Policy
- Never commit/push unless explicitly requested
- Run quality checks before reporting task complete

### Code Completion Checklist
1. `cargo fmt`
2. `cargo clippy -- -D warnings`
3. `cargo build --profile dev-release`
4. `cargo test`

### Session Context
- `CONTINUE.md` - Current ongoing work status (read this first for context)
- `SESSION_*.md` - Recent session notes
- `docs/` - Architecture and implementation docs (numbered in reading order)
