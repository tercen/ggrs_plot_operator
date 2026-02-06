# Data Flow Rules

Coordinate systems, transformations, and data flow through the operator.

## Coordinate System

Tercen sends quantized coordinates. GGRS dequantizes to actual values.

| Column | Type | Range | Purpose |
|--------|------|-------|---------|
| `.xs` | u16 | 0-65535 | Quantized X position |
| `.ys` | u16 | 0-65535 | Quantized Y position |
| `.x` | f64 | data range | Actual X value (after dequantization) |
| `.y` | f64 | data range | Actual Y value (after dequantization) |

## Dequantization Formula

```
actual_value = min_value + (quantized / 65535.0) * (max_value - min_value)
```

Per-facet axis ranges come from Y-axis table (`.minY`, `.maxY`) and optionally X-axis table.

## Chart-Type Driven Layout

Chart type (from Tercen UI) determines layout behavior via `ChartKind` enum:

| Aspect | Scatter/Line/Bar | Heatmap |
|--------|------------------|---------|
| Position columns | `.xs`, `.ys` (quantized) | `.ci`, `.ri` (grid indices) |
| Axis type | Continuous | Discrete (categorical labels) |
| Faceting | Yes (`.ci`/`.ri` → panels) | No (grid IS the plot) |
| Coordinate transform | Dequantize u16 → f64 | None (integers) |
| Scale expansion | 5% padding | 0.5 units (centers in tiles) |

### Geom Selection (pipeline.rs)

| ChartKind | Geom | Notes |
|-----------|------|-------|
| `Point` | `Geom::point_sized(config.point_size)` | Shape from `layer_shapes` |
| `Line` | `Geom::line_width(config.point_size)` | Width = dot size × multiplier |
| `Heatmap` | `Geom::tile()` | Full-cell tiles |
| `Bar` | `Geom::bar()` | Vertical bars |

### Line Rendering

Lines are rendered as polylines per color group:
- `add_line_point(x, y, color)` accumulates points in insertion order
- `flush_lines()` draws Cairo polylines (`move_to`/`line_to`/`stroke`)
- Inter-chunk continuity via `prev_line_points` HashMap (persists last point per color)
- Line width = dot size (UI scale 1-10 × `point.size.multiplier`)

## Data Streaming

GGRS calls `query_data_multi_facet(data_range)` to stream data in chunks:

```rust
// Columns fetched for non-heatmap
vec![".ci", ".ri", ".xs", ".ys", /* color columns */]

// For heatmaps: aggregate by (ci, ri) first
let aggregated = self.aggregate_heatmap_data().await?;
```

## Color Flow

1. Tercen provides color factor values or `.colorLevels`
2. Operator interpolates using palette (`add_color_columns()`)
3. Packed RGB stored as i64 in `.color` column
4. GGRS uses pre-computed colors directly

```rust
// Continuous: factor column → palette interpolation
let rgb = interpolate_color(value, &palette);

// Categorical: .colorLevels → default palette
let rgb = categorical_color_from_level(level);
```

## Facet Index Mapping

For pagination, grid indices map to original data indices:

```rust
// Grid position (0-11) → original .ri value (12-23 for page 2)
let original_idx = facet_info.row_facets.groups[grid_idx].original_index;
```

## Transform Support

Axis transforms (log, asinh, logicle) are stored in `NumericAxisData.transform`:

```rust
NumericAxisData {
    min_value: ...,
    max_value: ...,
    transform: Some(Transform {
        transform_type: TransformType::Ln,
        parameters: vec![],
    }),
}
```

GGRS applies inverse transform during dequantization.

## Axis Scale Types

GGRS uses `AxisScale` trait for axis rendering. The operator returns appropriate scale via `axis_scale_for()`:

| AxisData Variant | Scale Type | Breaks | Labels |
|------------------|------------|--------|--------|
| `Numeric` | `LinearAxisScale` | Computed ticks | Formatted numbers |
| `Categorical` | `CategoricalAxisScale` | Integer positions (0, 1, 2...) | Category names |

### Categorical Scale Details

```rust
// ggrs-core/src/stream.rs
impl AxisScale for CategoricalAxisScale {
    fn range(&self) -> (f64, f64) {
        (-0.5, self.categories.len() as f64 - 0.5)  // 0.5 expansion
    }
    fn major_breaks(&self, _min: f64, _max: f64) -> Vec<f64> {
        (0..self.categories.len()).map(|i| i as f64).collect()
    }
    fn format_label(&self, value: f64) -> String {
        self.categories.get(value.round() as usize).cloned().unwrap_or_default()
    }
}
```

For heatmaps, both X and Y axes are categorical with labels from Tercen's row/column factor tables.

## Opacity

Global opacity (0.0–1.0, default 1.0) controls transparency of all data geoms:

```
operator.json → OperatorConfig.opacity → PlotSpec.opacity → BatchRenderer flush methods
```

- Applied via `set_source_rgba` (replacing `set_source_rgb`) in flush_points, flush_rects, flush_shapes, flush_lines
- Non-data elements (axes, labels, grid, strip text, borders) stay fully opaque
- Shape borders (pch 21-25 black strokes) stay opaque
- Zero memory/perf cost: Cairo surfaces are already ARGB32; PNG stays RGB (composited against white)

## Multi-layer Color Priority

In `stream_generator.rs`, color sources are checked in this order:
1. `per_layer_colors` — multi-layer (respects `.axisIndex` for per-layer assignment)
2. `color_infos` — single-layer legacy uniform colors
3. Layer-based coloring — pure layer colors from `.axisIndex`

`per_layer_colors` must take priority to avoid applying the same palette to all layers.