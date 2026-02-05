# ggrs_plot_operator
Plot operator using ggrs

# Roadmap

### 0.0.1 ✅ COMPLETE

- [x] Data load
- [x] Streaming and chunking architecture
- [x] Scatter plot with a single facet
- [x] GITHUB Actions (CI)
- [x] GITHUB Actions (Release)
- [x] Plot saving


### 0.0.2 ✅ COMPLETE

- [x] Scatter plot with multiple facets (row/column/grid faceting with FreeY scales)
- [x] Optimize bulk streaming for multi-facet (currently uses per-facet chunking)
- [x] Add operator properties - Plot width/height with "auto", backend (cpu/gpu)
- [x] **Add support for continuous colors** (numeric color factors with palette interpolation)
- [x] Review and optimize dependencies

Note: Point size is hardcoded (4) - should come from crosstab model aesthetics.

### 0.0.3

- [x] Use operator input specs to get projection information
- [x] Dynamic point size
- [x] Specify gRPC as communication protocol
- [x] Add pages
- [x] Add x axis support
- [x] Add support for continuous color scale 
- [x] Add support for categorical colors (ColorLevels column)
- [x] Add color scale legend
- [x] Configurable textual elements in plot (axis labels, legend, title)

Note: Legend positioning still requires fine-tuning

### 0.0.4

- [x] Add heatmap (basic tile rendering)
- [x] Heatmap cell labels (category names from schema)
- [x] Axis label overlap culling (automatic label hiding when dense)
- [x] Legend colorbar uses actual Tercen palette (not hardcoded gradient)
- [x] Log, asinh and logicle scales
- [x] Add support for manual axis ranges

### 0.0.5

- [ ] Add themes
- [ ] Add bar plot
- [ ] Add line plot
- [ ] Tick label rotation (axis.x.tick.rotation, axis.y.tick.rotation)


### Unspecified Version
- [ ] Switching between GPU / CPU
- [ ] Further optimize bulk streaming for multi-facet
- [ ] Use factor name for y and x axis labels when parameter is not specified


## Label Overlap Culling

GGRS automatically hides overlapping axis labels to maintain visual clarity. This is implemented in `ggrs-core/src/overlap.rs`.

### How it works

1. Labels are processed in order (first-come-first-served)
2. Each label's bounding box is checked against previously rendered labels
3. If overlap detected (with 2px padding), the label is skipped
4. Result: Only non-overlapping labels are displayed

### Implementation

| Component | Description |
|-----------|-------------|
| `BoundingBox` | Represents element bounds with rotation support |
| `OverlapCuller` trait | Common interface for overlap detection |
| `LinearCuller` | O(n) implementation for labels (< 1000 elements) |
| `GridCuller` | O(1) implementation for dense data points (future use) |

### Rotation support

Axis labels can be rotated (e.g., 45° or 90°). The culling uses the axis-aligned bounding box (AABB) of the rotated rectangle:

```
Original (50×20):     Rotated 45°:        AABB of rotated:
┌──────────────┐      ╱╲                  ┌─────────────────┐
│   Label      │  →  ╱  ╲             →   │                 │
└──────────────┘    ╲    ╱                │    (49×49)      │
                     ╲  ╱                 │                 │
                      ╲╱                  └─────────────────┘
```

### Usage in render.rs

```rust
let mut x_label_culler = LinearCuller::new(2.0); // 2px padding

for (&x_break, label) in x_breaks.iter().zip(x_labels.iter()) {
    let (text_width, text_height) = estimate_text_size(label, font_size_px);
    let bbox = BoundingBox::new(x_pixel, y_pixel, text_width, text_height);

    if x_label_culler.should_render(bbox) {
        root.draw(&Text::new(label, ...));
    }
}
```


## Legend Colorbar

The legend colorbar for continuous color scales now uses the actual Tercen palette instead of a hardcoded gradient.

### How it works

1. `TercenStreamGenerator::load_legend_scale()` extracts color stops from the Tercen `ColorPalette`
2. Color stops are converted to `ggrs_core::legend::ColorStop` and stored in `LegendScale::Continuous`
3. `draw_continuous_legend()` uses `LegendScale::interpolate_color()` to sample the gradient
4. The legend gradient now matches the colors used in the actual plot

### LegendScale structure

```rust
pub enum LegendScale {
    Continuous {
        min: f64,
        max: f64,
        aesthetic_name: String,
        color_stops: Vec<ColorStop>,  // From Tercen palette
    },
    Discrete { ... },
    None,
}
```

### Color interpolation

The `interpolate_color(normalized)` method:
- Takes a normalized value [0, 1]
- Converts to data space using min/max
- Finds surrounding color stops
- Performs linear RGB interpolation between stops
- Falls back to blue-red gradient if no stops provided


## Tick Label Rotation

Axis tick labels can be rotated using operator properties.

### Properties

| Property | Default | Description |
|----------|---------|-------------|
| `axis.x.tick.rotation` | `0` | X-axis tick label rotation in degrees |
| `axis.y.tick.rotation` | `0` | Y-axis tick label rotation in degrees |

### Supported angles

Due to plotters library limitations, rotation is mapped to the nearest 90° increment:

| Input Range | Effective Rotation |
|-------------|-------------------|
| -45° to 44° | 0° (horizontal) |
| 45° to 134° | 90° (vertical, clockwise) |
| 135° to 224° | 180° (upside down) |
| 225° to 314° | 270° (vertical, counter-clockwise) |

### Common use cases

- **Horizontal labels (default)**: `axis.x.tick.rotation = 0`
- **Vertical X labels**: `axis.x.tick.rotation = 90` - useful for long category names
- **Vertical Y labels**: `axis.y.tick.rotation = 90` - rarely needed

### Text alignment

Alignment is automatically adjusted based on rotation:
- 0°: Center-Top (X), Right-Center (Y)
- 90°: Right-Center (X), Center-Bottom (Y)
- 180°: Center-Bottom (X), Left-Center (Y)
- 270°: Left-Center (X), Center-Top (Y)