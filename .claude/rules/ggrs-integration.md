# GGRS Integration Rules

Bindings between ggrs_plot_operator and the ggrs-core plotting library.

## StreamGenerator Trait

`TercenStreamGenerator` implements the GGRS `StreamGenerator` trait at `src/ggrs_integration/stream_generator.rs`.

### Required Trait Methods

```rust
impl StreamGenerator for TercenStreamGenerator {
    fn n_col_facets(&self) -> usize;           // Number of column facets
    fn n_row_facets(&self) -> usize;           // Number of row facets
    fn n_total_data_rows(&self) -> usize;      // Total rows across all facets

    fn query_col_facet_labels(&self) -> DataFrame;  // Column facet labels
    fn query_row_facet_labels(&self) -> DataFrame;  // Row facet labels

    fn query_x_axis(&self, col_idx, row_idx) -> AxisData;  // X-axis metadata
    fn query_y_axis(&self, col_idx, row_idx) -> AxisData;  // Y-axis metadata

    fn query_legend_scale(&self) -> LegendScale;    // Legend configuration
    fn query_color_metadata(&self) -> ColorMetadata; // Color handling mode

    fn facet_spec(&self) -> &FacetSpec;  // Faceting configuration
    fn aes(&self) -> &Aes;               // Aesthetic mappings

    fn query_data_multi_facet(&self, data_range: Range) -> DataFrame;  // Main data streaming
}
```

## Data Contract with GGRS

### Columns Streamed

| Column | Type | Purpose |
|--------|------|---------|
| `.ci` | usize | Column facet index for panel routing |
| `.ri` | usize | Row facet index for panel routing |
| `.xs` | u16 | Quantized X coordinates (0-65535) |
| `.ys` | u16 | Quantized Y coordinates (0-65535) |
| `.color` | i64 | Pre-computed packed RGB (u32 stored as i64) |

### Dequantization

Tercen sends quantized coordinates. GGRS dequantizes using axis ranges:
- `.xs`, `.ys` (uint16 0-65535) → `.x`, `.y` (actual f64 values)
- Transformation happens in GGRS `render.rs`, not in the operator

## Heatmap Mode

When `ChartKind::Heatmap`:
- Facet counts override to 1×1 (single panel)
- `.ci` becomes X position, `.ri` becomes Y position
- Data is aggregated by `(ci, ri)` with configurable aggregation (last/first/mean/median)
- Axis ranges: X = (-0.5, n_cols-0.5), Y = (-0.5, n_rows-0.5)

```rust
stream_gen.set_heatmap_mode(n_cols, n_rows);
```

## Color Handling

Colors are pre-computed in `add_color_columns()` and passed to GGRS as packed RGB:

```rust
// Packed as u32 stored in i64 for Polars compatibility
let packed = ggrs_core::PackedRgba::rgb(r, g, b).to_u32() as i64;
```

GGRS receives `ColorMetadata::Precomputed` - no scale training needed.

## ggrs-core Dependency

```toml
# Local dev (uncomment for local changes):
# ggrs-core = { path = "../ggrs/crates/ggrs-core", features = [...] }

# CI/Production (current default):
ggrs-core = { git = "https://github.com/tercen/ggrs", branch = "main", features = ["webgpu-backend", "cairo-backend"] }
```

Switch to path dependency when modifying ggrs-core. Switch back to git before committing.

## Key GGRS Types

```rust
use ggrs_core::{
    aes::Aes,
    data::DataFrame,
    legend::{ColorStop, LegendScale},
    stream::{AxisData, FacetSpec, NumericAxisData, Range, StreamGenerator, Transform},
    EnginePlotSpec, Geom, PlotGenerator, PlotRenderer, Theme,
};
```