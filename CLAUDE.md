# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with Tercen's gRPC API. It receives tabular data from Tercen, generates high-performance plots with faceting and colors, and returns images (PNG, SVG, or hybrid SVG).

Supported plot types: scatter, heatmap (categorical axes), bar, line. Features: continuous/categorical colors, multi-layer rendering, faceting, pagination, axis transforms (log, asinh, logicle), global opacity, themes (gray, bw, linedraw, light, dark, minimal, classic, void, publish). Rendering backends: CPU (Cairo) and GPU (WebGPU/Vulkan).

## Essential Commands

```bash
# Build (use dev-release for faster iteration, ~2x faster than release)
cargo build --profile dev-release

# Quality checks (MANDATORY before code is complete)
cargo fmt && cargo clippy -- -D warnings && cargo test

# ggrs-core quality checks (when modifying the plotting engine)
cd ../ggrs/crates/ggrs-core && cargo fmt && cargo clippy --features "webgpu-backend,cairo-backend" -- -D warnings

# Run a single test
cargo test test_name

# Local testing with Tercen (requires running Tercen instance)
./test_local.sh [cpu|gpu] [theme] [png|svg|hsvg]

# Proto submodule setup (required for gRPC definitions)
git submodule update --init --recursive

```

## Binaries

- **`ggrs_plot_operator`** (src/main.rs): Production entry point. Tercen passes `--taskId`, `--serviceUri`, `--token` as CLI args. Creates `ProductionContext`, generates plots, uploads results back to Tercen.
- **`dev`** (src/bin/dev.rs): Local testing. Reads `TERCEN_URI`, `TERCEN_TOKEN`, `WORKFLOW_ID`, `STEP_ID` from env, loads `operator_config.json` for property overrides, saves PNGs to local files.
- **`prepare`** (src/bin/prepare.rs): Creates CubeQueryTask for a data step. Used by `setup_test_data.sh` to prepare steps before rendering.

Both `ggrs_plot_operator` and `dev` share the same pipeline via `pipeline::generate_plots<C: TercenContext>()`.

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
ggrs-core (../ggrs/crates/)  → Plot rendering (single path: stream_and_render_direct())
      ↓
PNG/SVG Output
```

### Key Modules

| Module | Purpose |
|--------|---------|
| `src/tercen/client.rs` | TercenClient with gRPC auth |
| `src/tercen/context/` | `TercenContext` trait + `ProductionContext`/`DevContext` |
| `src/tercen/color_processor.rs` | `add_color_columns()` for color interpolation |
| `src/tercen/operator_properties.rs` | `OperatorPropertyReader` — validates props from operator.json |
| `src/ggrs_integration/stream_generator.rs` | `TercenStreamGenerator` implements GGRS `StreamGenerator` |
| `src/pipeline.rs` | Orchestrates plot generation, selects geom, configures theme |
| `src/config.rs` | `OperatorConfig` from `operator.json` properties |

### Related Repository: ggrs-core

The `ggrs-core` library at `../ggrs/crates/ggrs-core` is the plotting engine. Changes often span both repos.

```toml
# Cargo.toml: Switch to path dependency when modifying ggrs-core locally
ggrs-core = { path = "../ggrs/crates/ggrs-core", features = [...] }

# Switch back to git before committing — must push ggrs-core to git first
# ggrs-core = { git = "https://github.com/tercen/ggrs", tag = "0.3.0", features = [...] }
```

**Important**: The git version may lag behind the local path version. Always verify the git version has all needed methods/types before switching the operator to a git dependency.

### Key ggrs-core Files

| File | Purpose |
|------|---------|
| `ggrs-core/src/render.rs` | `stream_and_render_direct()` — all rendering (~2000 lines) |
| `ggrs-core/src/engine.rs` | `PlotSpec` — all plot configuration (opacity, layers, theme, etc.) |
| `ggrs-core/src/geom.rs` | `GeomType` enum (Point, Line, Tile, Bar) + `Geom` constructors |
| `ggrs-core/src/panel/draw_primitives.rs` | `BatchRenderer` — Cairo drawing (points, rects, shapes, lines) |
| `ggrs-core/src/stream.rs` | `StreamGenerator` trait, `AxisScale` trait, `AxisData` |
| `ggrs-core/src/theme/mod.rs` | `Theme` struct, pre-built themes |

## Core Technical Decisions

1. **Columnar architecture** — Never build row-by-row structures. Always use Polars columnar operations.
2. **No fallback strategies** — Fallbacks mask bugs. Fail loudly with informative errors. All property validation returns `Result<T, String>` and propagates errors.
3. **Single rendering path** — All rendering goes through `stream_and_render_direct()` in ggrs-core `render.rs`.

## Local Testing

Edit `test_local.sh` to select the active example by uncommenting the desired `WORKFLOW_ID`/`STEP_ID`:
- **EXAMPLE1**: Heatmap with divergent palette
- **EXAMPLE2**: Simple scatter (no X-axis table)
- **EXAMPLE3**: Scatter with X-axis table
- **EXAMPLE4**: Log transform test
- **EXAMPLE5**: Bar plots
- **EXAMPLE6**: Multiple layers
- **EXAMPLE7**: Line plot

Override operator properties via `operator_config.json`:
```json
{"backend": "cpu", "theme": "light", "plot.width": "800", "legend.position": "right", "opacity": "0.5"}
```

### Showcase Pipeline

`setup_test_data.sh` automates end-to-end visual testing: creates a Tercen project, uploads test data, builds workflow steps, renders all combinations of backend × theme (× palette for heatmaps), and generates `showcase.html` with interactive dropdowns. Output images go to `showcase_output/`.

## Session Context

- `CONTINUE.md` — Current work status and next tasks (read first when resuming)

## Notes for Claude Code

- Never commit/push unless explicitly requested
- Run quality checks before reporting task complete
- Add diagnostic prints to verify data flow before making multiple changes
- Verify ONE change at a time — don't batch multiple file changes without testing
- Check both operator AND ggrs-core when rendering issues occur
