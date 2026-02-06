# Continue From Here

**Last Updated**: 2026-02-06
**Status**: SVG backend Phase 1 complete + operator wiring; visually tested

---

## Current State

### Recent Changes (2026-02-06)

1. **SVG backend (ggrs-core)** — Phase 1 complete
   - New `svg/` module: `mod.rs` (SvgRenderer), `writer.rs` (SvgWriter), `batch.rs` (SvgBatchRenderer)
   - Zero-length-line trick for scatter points: `<path d="M{x} {y}h0" stroke-linecap="round"/>`
   - CSS color classes (`.c-FF0000 { stroke: #FF0000; fill: #FF0000; }`)
   - Streaming XML writer — no DOM tree, constant memory, 64KB buffer
   - Per-panel `<g>` groups with `translate()` transforms
   - Supports all geom types: points (ZLL), rects (paths), lines (polylines)
   - Grid lines (major/minor), panel backgrounds, panel borders, strips
   - Axis labels, tick marks, title
   - Opacity support via `<g class="data" opacity="...">` wrapper
   - Dispatched via `BackendChoice::Svg` in `render.rs`
   - Visually tested: EXAMPLE2 (13K pts, 2 pages) + EXAMPLE3 (1K pts, 500 panels)

2. **SVG wiring (operator)**
   - `operator.json`: "svg" added to backend enum values
   - `pipeline.rs`: "svg" → `BackendChoice::Svg` + `OutputFormat::Svg`, correct .svg extension
   - `dev.rs`: Saves as .svg when backend="svg"
   - `test_local.sh`: "svg" added to valid backends, backend passed to config

3. **Global opacity support**
   - New `opacity` property in operator.json (0.0–1.0, default 1.0)
   - Flows: `OperatorConfig.opacity` → `PlotSpec.opacity` → `BatchRenderer` flush methods
   - `set_source_rgb` → `set_source_rgba` in flush_points, flush_rects, flush_shapes, flush_lines

4. **Line rendering** (ggrs-core)
   - `BatchRenderer.add_line_point(x, y, color)` accumulates points per color group
   - `BatchRenderer.flush_lines(ctx, width, opacity, prev_points)` draws polylines
   - Inter-chunk continuity via `prev_line_points` HashMap

5. **Strict property validation**
   - All `OperatorPropertyReader` methods return `Result<T, String>` — no silent fallbacks

6. **Multi-layer color fix**
   - `per_layer_colors` now takes priority over `color_infos` in stream_generator.rs

### Files Changed (this session)

**ggrs-core:**
| File | Change |
|------|--------|
| `svg/mod.rs` | NEW: SvgRenderer — full SVG rendering orchestration (~960 lines) |
| `svg/writer.rs` | NEW: SvgWriter — streaming XML writer (~320 lines) |
| `svg/batch.rs` | NEW: SvgBatchRenderer — per-panel path data accumulation (~290 lines) |
| `lib.rs` | Added `pub mod svg;` + `pub use svg::SvgRenderer;` |
| `render.rs` | Added `render_to_file_svg()` method, `BackendChoice::Svg` dispatch |
| `engine.rs` | Added `opacity: f64` field + builder method to PlotSpec |
| `render.rs` | Added `is_line_geom` detection, line rendering branches, opacity pass-through |
| `panel/draw_primitives.rs` | Added `add_line_point()`, `flush_lines()`, `set_source_rgba` |

**operator:**
| File | Change |
|------|--------|
| `operator.json` | Added `opacity` property, "svg" backend value |
| `src/pipeline.rs` | SVG backend dispatch, format-aware temp file extension |
| `src/bin/dev.rs` | SVG-aware file extension for output |
| `test_local.sh` | "svg" in valid backends, backend var in config template |
| `src/config.rs` | Added `opacity: f64` field |
| `src/tercen/operator_properties.rs` | All validation methods now return `Result<T, String>` |
| `src/ggrs_integration/stream_generator.rs` | Fixed multi-layer color priority |

---

## Project Architecture

```
src/
├── tercen/                    # Tercen gRPC integration
│   ├── client.rs              # TercenClient with auth
│   ├── context/               # TercenContext trait + impls
│   ├── table.rs               # TableStreamer
│   ├── colors.rs              # Color types, palette extraction, ChartKind
│   ├── color_processor.rs     # add_color_columns() + add_mixed_layer_colors()
│   ├── palettes.rs            # PALETTE_REGISTRY
│   ├── operator_properties.rs # OperatorPropertyReader (strict validation)
│   └── ...
├── ggrs_integration/
│   ├── stream_generator.rs    # TercenStreamGenerator (implements StreamGenerator)
│   └── cached_stream_generator.rs
├── pipeline.rs                # Plot generation orchestration
├── config.rs                  # OperatorConfig from operator.json
└── main.rs                    # Production entry point
```

---

## Test Configuration

Edit `test_local.sh` to select test example:
- **EXAMPLE1**: Heatmap with divergent palette
- **EXAMPLE2**: Simple scatter (no X-axis table)
- **EXAMPLE3**: Scatter with X-axis table (crabs dataset) — currently active
- **EXAMPLE4**: Log transform test
- **EXAMPLE5**: Bar plots
- **EXAMPLE6**: Multiple layers
- **EXAMPLE7**: Line plot
- **EXAMPLE8**: SVG scatter test

---

## Development Workflow

```bash
# Build
cargo build --profile dev-release

# Quality checks (MANDATORY)
cargo fmt && cargo clippy -- -D warnings && cargo test

# Local test (cpu, gpu, or svg)
./test_local.sh [cpu|gpu|svg]
```

### Cargo.toml Dependency

Check dependency mode before committing:
```toml
# Local dev (current - uncomment for local ggrs changes):
ggrs-core = { path = "../ggrs/crates/ggrs-core", features = [...] }

# CI/Production (switch to this before committing):
# ggrs-core = { git = "https://github.com/tercen/ggrs", branch = "main", features = [...] }
```

**Note**: Currently using path dependency. Switch to git before committing.

---

## Pending Tasks

### SVG Phase 2
- [ ] Non-circle shapes (pch 0-25) via `<defs>/<use>` + shapes.rs
- [ ] Legend rendering in SVG
- [ ] SVGZ compression option
- [ ] Better text measurement (font metrics instead of estimation)

### General
- [ ] Visual verification of line plot (EXAMPLE7) — needs `./test_local.sh cpu`
- [ ] Visual verification of opacity (set `"opacity": "0.5"` in operator_config.json)
- [ ] Commit ggrs-core changes (opacity, line rendering, SVG backend)
- [ ] Commit operator changes (opacity, line geom, strict validation, color fix, SVG wiring)
- [ ] Switch Cargo.toml to git dependency before final commit

---

## Session History

- 2026-02-06: SVG backend Phase 1, operator wiring, visual testing
- 2026-02-06: Global opacity, line rendering, strict property validation, multi-layer color fix
- 2026-02-05: Theme publish, heatmap categorical labels, tile rendering fix, bar plots
- 2026-02-04: Refactoring complete - rendering path, color processor, transform parsing
- 2026-02-03: Transform support implemented (log, asinh, logicle)
