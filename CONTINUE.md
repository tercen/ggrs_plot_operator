# Continue From Here

**Last Updated**: 2026-02-06
**Status**: SVG cosmetic properties + output metadata complete; all tests pass

---

## Current State

### Recent Changes (2026-02-06, latest session)

1. **SVG Legend rendering** (ggrs-core)
   - Discrete legends: `<circle>` + `<text>` elements
   - Continuous legends: `<linearGradient>` + `<rect>`
   - Legend y-position clamping: prevents off-screen rendering when legend taller than plot

2. **Hybrid SVG (hSVG)** (ggrs-core)
   - Vector SVG skeleton (axes, labels, grids) + per-panel rasterized PNG as `<image>` elements
   - `BackendChoice::HybridSvg` + `OutputFormat::HybridSvg`
   - Supports all geom types (points, lines, tiles, bars) via `stream_data_hybrid()`
   - Inkscape compatibility: `xmlns:xlink` + `xlink:href` attributes

3. **SVG Cosmetic Properties** — 6 new operator properties
   - `grid.major.disable` / `grid.minor.disable` — boolean toggles (default: false)
   - `plot.title.font.size` / `axis.label.font.size` / `axis.tick.font.size` — optional pt overrides
   - `axis.line.width` — optional panel border width override
   - All default to empty = use theme defaults

4. **Theme setters** (ggrs-core `theme/mod.rs`)
   - `set_plot_title_size(pt)`, `set_axis_title_size(pt)`, `set_axis_text_size(pt)`
   - `set_panel_border_linewidth(pt)`, `panel_border_color()`, `panel_border_linewidth()`
   - `disable_grid_major()`, `disable_grid_minor()`, `show_grid_major()`, `show_grid_minor()`

5. **SVG Y-axis tick rotation** (ggrs-core `svg/mod.rs`)
   - Fixed: Y-axis tick labels now respect `theme.y_tick_rotation()` (X already worked)

6. **SVG grid conditionals** (ggrs-core `svg/mod.rs`)
   - Major grid wrapped in `if theme.show_grid_major() { ... }`
   - Minor grid wrapped in `if theme.show_grid_minor() && ... { ... }`

7. **SVG panel border from theme** (ggrs-core `svg/mod.rs`)
   - Replaced hardcoded `#333333`/`0.5` with `theme.panel_border_color()` / `theme.panel_border_linewidth()`

8. **SVG output metadata** (operator `result.rs`)
   - `PlotResult` now has `output_ext: String` field
   - `mimetype_for_ext()` helper: `"svg"` → `"image/svg+xml"`, default → `"image/png"`
   - `save_result()` and `save_results()` use dynamic filename/mimetype from output_ext
   - `dev.rs` uses `plot.output_ext` for local file naming
   - Production `main.rs` passes `&plot.output_ext` to `save_result()`

### Files Changed (this session)

**ggrs-core:**
| File | Change |
|------|--------|
| `svg/mod.rs` | Legend rendering, y-tick rotation fix, grid conditionals, themed panel border |
| `theme/mod.rs` | Font size setters, grid visibility methods, panel border getters/setters |

**operator:**
| File | Change |
|------|--------|
| `operator.json` | 6 new cosmetic properties |
| `src/tercen/operator_properties.rs` | `get_bool()`, `get_optional_f64()` methods + tests |
| `src/config.rs` | 6 new fields (grid_major_disable, grid_minor_disable, title_font_size, axis_label_font_size, tick_label_font_size, axis_line_width) |
| `src/pipeline.rs` | Theme customization block (grid/font/linewidth), `output_ext` in PlotResult |
| `src/tercen/result.rs` | `output_ext` field, `mimetype_for_ext()`, dynamic filename/mimetype |
| `src/tercen/context/base.rs` | `save_result()` wrapper updated with `output_ext` param |
| `src/main.rs` | Passes `&plot.output_ext` to `save_result()` |
| `src/bin/dev.rs` | Uses `plot.output_ext` for local file naming |

---

## Project Architecture

```
src/
├── tercen/                    # Tercen gRPC integration
│   ├── client.rs              # TercenClient with auth
│   ├── context/               # TercenContext trait + impls
│   ├── result.rs              # Result upload (PNG/SVG, dynamic mimetype)
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
- **EXAMPLE3**: Scatter with X-axis table (crabs dataset)
- **EXAMPLE4**: Log transform test
- **EXAMPLE5**: Bar plots
- **EXAMPLE6**: Multiple layers
- **EXAMPLE7**: Line plot
- **EXAMPLE8**: SVG scatter test — currently active

Override cosmetic properties via `operator_config.json`:
```json
{
  "grid.major.disable": "true",
  "plot.title.font.size": "20",
  "axis.label.font.size": "16",
  "axis.tick.font.size": "12",
  "axis.line.width": "2"
}
```

---

## Development Workflow

```bash
# Build
cargo build --profile dev-release

# Quality checks (MANDATORY)
cargo fmt && cargo clippy -- -D warnings && cargo test

# Local test
./test_local.sh [cpu|gpu] [theme] [png|svg|hsvg]
```

### Cargo.toml Dependency

Currently using **path dependency** (local ggrs-core). The git version does NOT have the latest theme setters/grid methods/hSVG support.

```toml
# Local dev (CURRENT):
ggrs-core = { path = "../ggrs/crates/ggrs-core", features = [...] }

# CI/Production (switch after pushing ggrs-core changes):
# ggrs-core = { git = "https://github.com/tercen/ggrs", branch = "main", features = [...] }
```

**Before committing operator**: Push ggrs-core changes to git first, then switch to git dependency.

---

## Pending Tasks

### Before Committing
- [ ] Push ggrs-core changes to git (theme setters, SVG legend, hSVG, grid conditionals, y-tick rotation, panel border from theme)
- [ ] Switch Cargo.toml to git dependency
- [ ] Commit operator changes

### SVG Phase 2
- [ ] Non-circle shapes (pch 0-25) via `<defs>/<use>` + shapes.rs
- [ ] SVGZ compression option
- [ ] Better text measurement (font metrics instead of estimation)

### General
- [ ] Visual verification of line plot (EXAMPLE7) with SVG output
- [ ] Visual verification of hSVG with heatmap (EXAMPLE1)

---

## Session History

- 2026-02-06: SVG output metadata (dynamic ext/mimetype), cosmetic properties (grid toggle, font sizes, axis line width), Y-tick rotation fix, legend y-position clamping, hSVG backend
- 2026-02-06: SVG backend Phase 1, operator wiring, visual testing
- 2026-02-06: Global opacity, line rendering, strict property validation, multi-layer color fix
- 2026-02-05: Theme publish, heatmap categorical labels, tile rendering fix, bar plots
- 2026-02-04: Refactoring complete - rendering path, color processor, transform parsing
- 2026-02-03: Transform support implemented (log, asinh, logicle)
