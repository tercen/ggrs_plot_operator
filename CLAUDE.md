# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with Tercen's gRPC API. It receives tabular data from Tercen, generates high-performance plots with faceting and colors, and returns PNG images.

**Current status**: Scatter plots, heatmaps, continuous/categorical colors, faceting, pagination, and axis transforms (log, asinh, logicle) are implemented. Bar and line plots are planned.

## Essential Commands

```bash
# Build (use dev-release for faster iteration)
cargo build --profile dev-release

# Quality Checks (MANDATORY before code is complete)
cargo fmt && cargo clippy -- -D warnings && cargo test

# Local testing with Tercen
./test_local.sh [cpu|gpu]

# Proto submodule setup (required for gRPC definitions)
git submodule update --init --recursive
```

## Architecture

```
Tercen gRPC API
      ↓
[1] src/tercen/              → gRPC client, auth, TSON streaming
      ↓
[2] TSON → Polars            → Columnar data transformation
      ↓
[3] src/ggrs_integration/    → Implements GGRS StreamGenerator trait
      ↓
ggrs-core (../ggrs/crates/)  → Plot rendering
      ↓
PNG Output
```

### Key Modules

| Module | Purpose |
|--------|---------|
| `src/tercen/client.rs` | TercenClient with gRPC auth |
| `src/tercen/context/` | `TercenContext` trait + `ProductionContext`/`DevContext` |
| `src/tercen/color_processor.rs` | `add_color_columns()` for color interpolation |
| `src/ggrs_integration/stream_generator.rs` | `TercenStreamGenerator` implements GGRS `StreamGenerator` |
| `src/pipeline.rs` | Orchestrates plot generation, selects geom, configures theme |

### Related Repository

The `ggrs-core` library at `../ggrs/crates/ggrs-core` is the plotting engine. Changes often span both repositories.

```toml
# Cargo.toml: Switch to path dependency when modifying ggrs-core locally
# ggrs-core = { path = "../ggrs/crates/ggrs-core", features = [...] }
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

// ✅ GOOD: Trust the specification, fail loudly
data.column(".ys")
```

### 3. Single Rendering Path

All rendering goes through `stream_and_render_direct()` in ggrs-core. There is no alternative path.

## Project Rules (Detailed)

See `.claude/rules/` for comprehensive documentation:
- `architecture.md` - Design principles, component responsibilities
- `ggrs-integration.md` - GGRS bindings, StreamGenerator trait
- `tercen-api.md` - Tercen gRPC integration, table IDs, TSON format
- `data-flow.md` - Coordinate systems, chart types, color flow
- `debugging.md` - Common errors, lessons learned

## Session Context

- `CONTINUE.md` - Current work status and next tasks (read first when resuming)
- `docs/` - Architecture docs (see `09_FINAL_DESIGN.md`)

## Local Testing

Edit `test_local.sh` to select the active example:
- **EXAMPLE1**: Heatmap with divergent palette
- **EXAMPLE2**: Simple scatter (no X-axis table)
- **EXAMPLE3**: Scatter with X-axis table
- **EXAMPLE4**: Log transform test

Override operator properties via `operator_config.json`:
```json
{"backend": "gpu", "plot.width": "800", "legend.position": "right"}
```

## Notes for Claude Code

- Never commit/push unless explicitly requested
- Run quality checks before reporting task complete
- Add diagnostic prints to verify data flow before making multiple changes
- Verify ONE change at a time - don't batch multiple file changes without testing
