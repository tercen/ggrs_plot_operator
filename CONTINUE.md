# Continue From Here

**Last Updated**: 2026-02-10
**Status**: Showcase ready to run — pending visual verification

---

## Current Task: Interactive HTML Showcase

### What We Built

A fully automated showcase pipeline (`setup_test_data.sh`) that:
1. Creates a Tercen project, uploads test data, builds a workflow
2. Creates 5 data steps (scatter×2, line×2, heatmap)
3. Renders all combinations of backend × theme (and palette for heatmap)
4. Generates `showcase.html` with interactive dropdowns

### Showcase Scenarios

| Scenario | Chart | Y | X | Color | Notes |
|----------|-------|---|---|-------|-------|
| `scatter_nocolor` | point | y | x | — | Basic scatter |
| `scatter_cat` | point | y | x | CAT1 | Categorical color |
| `line_nocolor` | line | y | x | — | Basic line |
| `line_cat` | line | y | x | CAT1 | Categorical color |
| `heatmap` | heatmap | heatval | — | heatval (RampPalette) | Continuous color, palette iteration |

### Test Data Design

CSV with 5000 rows, columns: `x, y, heatval, CAT1, CAT2, CAT3`

**`y` — category-dependent for scatter/line:**
- Per-CAT1 offsets: alpha=-6, beta=-2, gamma=+2, delta=+6
- Per-CAT1 slopes: alpha=0.3, beta=0.6, gamma=-0.2, delta=0.8
- ±2 noise
- Result: four clearly separated bands with different trends

**`heatval` — category-dependent for heatmap:**
- 16 distinct base means (4 CAT1 × 4 CAT2), spread across 0-100
- ±8 noise, clamped [0, 100]
- Result: clear checkerboard pattern that makes palettes pop

### Image Counts

| Type | Formula | Count |
|------|---------|-------|
| Non-heatmap | 4 scenarios × 2 backends × 9 themes | 72 |
| Heatmap | 1 scenario × 2 backends × 9 themes × 7 palettes | 126 |
| **Total** | | **198** |

### Heatmap Palette Iteration

The heatmap cycles over 7 palettes: `Spectral, Jet, Viridis, Hot, Cool, RdBu, YlGnBu`

Key functions:
- `patch_ramp_palette(wf_id, step_id, palette_name)` — patches `colors/palette` with a named `RampPalette` (`isUserDefined: false`)
- `render_heatmap_scenario()` — outer loop re-patches palette before each backend×theme batch
- Filename: `heatmap_{backend}_{theme}_{palette}.png`

### Showcase HTML

`showcase.html` with sections for Scatter, Line, Heatmap:
- Scatter/Line: dropdowns for Color variant, Theme, Backend
- Heatmap: dropdowns for **Palette**, Theme, Backend
- JS updates `<img src>` on dropdown change

### Recent Rendering Fixes (this session)

**Axis lines, tick marks, panel borders** — previously missing from all themes:
- Added `axis_line_x_color()`, `axis_line_y_color()`, `axis_line_linewidth()`, `axis_ticks_linewidth()` to `theme/mod.rs`
- Added panel border drawing, axis tick marks, and axis lines to `setup_cairo_chrome()` in `render.rs`
- Verified across classic, bw, publish, light, dark themes

### What's Done

- [x] `setup_test_data.sh` — full pipeline script
- [x] `prepare.rs` binary — creates CubeQueryTask for a step
- [x] CSV data with category-dependent means (y + heatval)
- [x] Heatmap color factor + palette patching (`add_color_factor` + `patch_ramp_palette`)
- [x] Heatmap palette iteration (7 palettes)
- [x] HTML showcase with interactive dropdowns
- [x] Axis lines, tick marks, panel borders in renderer

### Next Steps (Tomorrow)

1. **Run `./setup_test_data.sh`** — generate all 198 images + showcase.html
2. **Visual verification** — open showcase.html, spot-check:
   - scatter_cat: four separated color bands
   - line_cat: four diverging trend lines
   - heatmap: colorful checkerboard across palettes
   - Themes: axis lines (classic/publish), panel borders (bw/linedraw), ticks
3. **Fix any rendering issues** discovered during verification
4. **Quality checks**: `cargo fmt && cargo clippy -- -D warnings && cargo test` on both repos

### Not Yet Supported

- `scatter_cont` (continuous color): using same column for axis+color causes `.colorLevels` conflict
- `bar`: operator X-axis loading expects `.minX/.maxX`, bar provides `.xLevels`

---

## Key Files

| File | Role |
|------|------|
| `setup_test_data.sh` | Showcase generator script |
| `showcase.html` | Generated interactive HTML (output) |
| `showcase_output/` | Generated images directory (output) |
| `src/bin/prepare.rs` | CubeQueryTask creation binary |
| `src/bin/dev.rs` | Local rendering binary |
| `ggrs-core/src/render.rs` | Renderer (axis lines/ticks/borders added) |
| `ggrs-core/src/theme/mod.rs` | Theme accessors (axis line/tick methods added) |

---

## Previous Work (Completed)

### GPU Rendering (Phase 1) — Complete

- Hybrid GPU+Cairo architecture working for points
- Vulkan backend (GL has non-square texture bug)
- `Rgba8Unorm` texture format (avoids sRGB double-encoding)
- Visual verification passed: GPU scatter matches CPU scatter

### Pending (Not Current Priority)

- Push ggrs-core to git, switch to git dependency
- GPU Phase 2: heatmap/bar visual verification, performance comparison

---

## Development Workflow

```bash
# Build
cargo build --profile dev-release

# Quality checks (MANDATORY)
cargo fmt && cargo clippy -- -D warnings && cargo test

# ggrs-core checks
cd ../ggrs/crates/ggrs-core && cargo fmt && cargo clippy --features "webgpu-backend,cairo-backend" -- -D warnings

# Run showcase
./setup_test_data.sh

# Local test (single step)
./test_local.sh [cpu|gpu] [theme] [png|svg|hsvg]
```

### Cargo.toml Dependency

Currently using **path dependency** (local ggrs-core).

---

## Session History

- 2026-02-10: Showcase heatmap fix (color factor + palette patching), palette iteration, category-dependent data, axis lines/ticks/borders in renderer
- 2026-02-09: Showcase pipeline — setup_test_data.sh, prepare.rs, 5 scenarios, HTML generation
- 2026-02-09: GPU rendering Phase 1 — layout extraction, WebGPU pipelines, compositing
- 2026-02-06: SVG output, cosmetic properties, hSVG backend
- 2026-02-06: Global opacity, line rendering, strict property validation, multi-layer color fix
- 2026-02-05: Theme publish, heatmap categorical labels, tile rendering fix, bar plots
- 2026-02-04: Refactoring complete - rendering path, color processor, transform parsing
- 2026-02-03: Transform support implemented (log, asinh, logicle)
