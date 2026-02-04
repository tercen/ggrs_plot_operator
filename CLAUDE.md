# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with Tercen's gRPC API. It receives tabular data from Tercen, generates high-performance plots with faceting and colors, and returns PNG images.

**Current status**: Scatter plots, heatmaps, continuous/categorical colors, faceting, pagination, and axis transforms (log, asinh, logicle) are implemented. Bar and line plots are planned.

## Project Rules

Detailed rules are in `.claude/rules/`:
- `architecture.md` - Design principles, component responsibilities
- `ggrs-integration.md` - GGRS bindings, StreamGenerator trait implementation
- `tercen-api.md` - Tercen gRPC integration, table IDs, TSON format
- `data-flow.md` - Coordinate systems, chart types, color flow
- `commands.md` - Build, test, and development commands
- `debugging.md` - Debugging practices, common errors, lessons learned

## Session Context

- `CONTINUE.md` - Current work status and next tasks (read first if resuming work)
- `SESSION_*.md` - Recent session notes with implementation details
- `docs/` - Architecture docs (start with `09_FINAL_DESIGN.md`)

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
./test_local.sh [backend]  # cpu (default) or gpu

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

### Key Modules

| Module | Purpose |
|--------|---------|
| `src/tercen/client.rs` | TercenClient with gRPC auth |
| `src/tercen/context/` | `TercenContext` trait + `ProductionContext`/`DevContext` |
| `src/tercen/table.rs` | TableStreamer for chunked data streaming |
| `src/tercen/colors.rs` | Color palette extraction, `ChartKind` enum |
| `src/ggrs_integration/stream_generator.rs` | `TercenStreamGenerator` implements GGRS `StreamGenerator` |
| `src/pipeline.rs` | Orchestrates plot generation, selects geom, configures theme |
| `src/config.rs` | `OperatorConfig` from `operator.json` properties |

### Related Repository

The `ggrs-core` library at `../ggrs/crates/ggrs-core` is the plotting engine. Changes often span both repositories.

**Cargo.toml dependency switching**:
```toml
# Local dev (uncomment for local changes):
# ggrs-core = { path = "../ggrs/crates/ggrs-core", features = [...] }

# CI/Production (current default):
ggrs-core = { git = "https://github.com/tercen/ggrs", branch = "main", features = [...] }
```

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

### 3. Direct Rendering Path

The renderer uses `stream_and_render_direct()` for all rendering. This is the only rendering path - there is no alternative standard path. Any rendering modifications must update this function.

## Implementation Notes

### Tick Label Rotation

Due to plotters library limitations, rotation is mapped to the nearest 90° increment:
- -45° to 44° → 0° (horizontal)
- 45° to 134° → 90° (vertical)
- 135° to 224° → 180° (upside down)
- 225° to 314° → 270° (vertical, counter-clockwise)

### Label Overlap Culling

GGRS automatically hides overlapping axis labels. Labels are processed first-come-first-served; if a label's bounding box overlaps a previously rendered label (with 2px padding), it's skipped.

## Local Testing

Edit `test_local.sh` to change the active example (uncomment the desired WORKFLOW_ID/STEP_ID):
- **EXAMPLE1**: Heatmap with divergent palette
- **EXAMPLE2**: Simple scatter (no X-axis table)
- **EXAMPLE3**: Scatter with X-axis table (crabs dataset)
- **EXAMPLE4**: Log transform test

Create `operator_config.json` to override operator properties:
```json
{
  "backend": "gpu",
  "plot.width": "800",
  "legend.position": "right"
}
```

## Notes for Claude Code

- Never commit/push unless explicitly requested
- Run quality checks (`cargo fmt && cargo clippy -- -D warnings && cargo test`) before reporting task complete
- Add diagnostic prints to verify data flow before making multiple changes
- When modifying rendering: check if you're updating the lightweight path (`stream_and_render_direct`) or standard path
- Verify ONE change at a time - don't batch multiple file changes without testing
