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