# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with Tercen's gRPC API. It receives tabular data from Tercen, generates high-performance plots with faceting and colors, and returns PNG images.

Supported plot types: scatter, heatmap (categorical axes), bar, line. Features: continuous/categorical colors, multi-layer rendering, faceting, pagination, axis transforms (log, asinh, logicle), global opacity, themes (gray, bw, linedraw, light, dark, minimal, classic, void, publish).

## Essential Commands

```bash
# Build (use dev-release for faster iteration, ~2x faster than release)
cargo build --profile dev-release

# Quality checks (MANDATORY before code is complete)
cargo fmt && cargo clippy -- -D warnings && cargo test

# Run a single test
cargo test test_name

# Local testing with Tercen (requires running Tercen instance)
./test_local.sh [cpu|gpu] [theme]

# Proto submodule setup (required for gRPC definitions)
git submodule update --init --recursive
```

## Two Binaries

- **`ggrs_plot_operator`** (src/main.rs): Production entry point. Tercen passes `--taskId`, `--serviceUri`, `--token` as CLI args. Creates `ProductionContext`, generates plots, uploads results back to Tercen.
- **`dev`** (src/bin/dev.rs): Local testing. Reads `TERCEN_URI`, `TERCEN_TOKEN`, `WORKFLOW_ID`, `STEP_ID` from env, loads `operator_config.json` for property overrides, saves PNGs to local files.

Both share the same pipeline via `pipeline::generate_plots<C: TercenContext>()`.

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
PNG Output
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

### Related Repository

The `ggrs-core` library at `../ggrs/crates/ggrs-core` is the plotting engine. Changes often span both repos.

```toml
# Cargo.toml: Switch to path dependency when modifying ggrs-core locally
ggrs-core = { path = "../ggrs/crates/ggrs-core", features = [...] }

# Switch back to git before committing
# ggrs-core = { git = "https://github.com/tercen/ggrs", branch = "main", features = [...] }
```

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

## Geom Selection (pipeline.rs)

`ChartKind` maps to `Geom` in `pipeline.rs`:

| ChartKind | Geom | Notes |
|-----------|------|-------|
| `Point` | `Geom::point_sized(config.point_size)` | Shape from `layer_shapes` |
| `Line` | `Geom::line_width(config.point_size)` | Width = dot size × multiplier |
| `Heatmap` | `Geom::tile()` | Full-cell tiles |
| `Bar` | `Geom::bar()` | Vertical bars |

## Opacity

Global opacity (0.0–1.0) flows: `operator.json` → `OperatorConfig.opacity` → `PlotSpec.opacity` → `BatchRenderer` flush methods.

- Applies to all data geoms (points, tiles, bars, lines) via `set_source_rgba`
- Non-data elements (axes, labels, grid, borders) stay fully opaque
- Zero performance cost: Cairo surfaces are already ARGB32; PNG stays RGB (composited against white)
- Shape borders (pch 21-25) remain opaque

## Line Rendering

Lines are rendered as polylines per color group in `BatchRenderer`:

- `add_line_point(x, y, color)` accumulates points per color
- `flush_lines()` draws polylines with Cairo `move_to`/`line_to`/`stroke`
- **Inter-chunk continuity**: `prev_line_points` HashMap persists last point per color across batch flushes, so lines connect seamlessly across data chunks
- Line width derived from dot size (UI scale × `point.size.multiplier`)
- Cairo `LineJoin::Round` + `LineCap::Round` for smooth rendering

## BatchRenderer (draw_primitives.rs)

Groups draw calls by color (HashMap<PackedRgba, Vec>) to minimize Cairo state changes:

| Method | Geom | Key detail |
|--------|------|------------|
| `flush_points(ctx, radius, opacity)` | Point (circle) | Basic filled circles |
| `flush_rects(ctx, opacity)` | Tile/Bar | Axis-aligned rectangles |
| `flush_shapes(ctx, radius, opacity)` | Point (pch 0-25) | All ggplot2 point shapes |
| `flush_lines(ctx, width, opacity, prev_points)` | Line | Polylines with inter-chunk continuity |

## Property Validation

All operator property methods in `OperatorPropertyReader` return `Result<T, String>`:
- `get_enum()`, `get_f64()`, `get_f64_in_range()`, `get_i32()`, `get_coords()`, `get_shape_list()`
- Invalid values produce errors, never silent fallbacks
- `OperatorConfig::from_properties()` returns `Result<Self, String>`

## Multi-layer Color Priority

In `stream_generator.rs`, color sources are checked in this order:
1. `per_layer_colors` — multi-layer: per-layer color config (mixed, explicit, constant)
2. `color_infos` — single-layer: legacy uniform colors
3. Layer-based coloring — pure layer colors from `.axisIndex`

`per_layer_colors` takes priority when present to correctly handle per-layer palettes.

## Detailed Rules

See `.claude/rules/` for comprehensive documentation:
- `architecture.md` — Design principles, component responsibilities, error handling patterns
- `ggrs-integration.md` — GGRS bindings, StreamGenerator trait, data contract columns
- `tercen-api.md` — Tercen gRPC integration, table IDs, TSON format
- `data-flow.md` — Coordinate systems, chart types, color flow, axis scale types
- `debugging.md` — Common errors, lessons learned, heatmap coordinate gotchas

## Session Context

- `CONTINUE.md` — Current work status and next tasks (read first when resuming)

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

## Notes for Claude Code

- Never commit/push unless explicitly requested
- Run quality checks before reporting task complete
- Add diagnostic prints to verify data flow before making multiple changes
- Verify ONE change at a time — don't batch multiple file changes without testing
- Check both operator AND ggrs-core when rendering issues occur
