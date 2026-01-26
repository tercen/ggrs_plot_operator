# Heatmap Implementation Plan

## Overview

Add `geom_tile` support to GGRS for rendering heatmaps. The implementation follows ggplot2 semantics exactly.

**Guiding Principles**:
- ggplot2 is the reference implementation
- The plot operator remains minimal ("dumb") - only passes geom type
- All rendering logic AND tile sizing lives in GGRS
- Fail fast with clear errors (no fallback strategies)

## Architecture Summary

```
Tercen Data (.xs, .ys, .ci, .ri, color)
    ↓
ggrs_plot_operator (passes geom_type only)
    ↓
GGRS StreamGenerator (same data flow as scatter)
    ↓
GGRS Renderer (GeomRenderer trait dispatch)
    ↓
TileRenderer calculates tile dimensions from:
  - Panel dimensions (known at render time)
  - Unique x/y positions in data (derived from quantized coords)
    ↓
PNG Output
```

**Key insight**: Heatmap uses identical data flow to scatter plots. GGRS calculates tile dimensions internally - the operator does NOT pass tile size metadata.

## Tile Sizing Strategy

Based on analysis of sci_web_component2 and ggplot2:

**ggplot2 approach**: `resolution()` finds smallest gap between adjacent x/y values
**sci_crosstab approach**: Fixed `cellSize` (250px default), or `panel_size / n_unique_values`

**GGRS approach** (combining both):
1. At render time, GGRS knows the panel dimensions in pixels
2. GGRS can derive unique x/y positions from quantized coordinates
3. Tile dimensions = `panel_dimension / n_unique_values`

```
tile_width_px = panel_width_px / n_unique_x
tile_height_px = panel_height_px / n_unique_y
```

This keeps the operator completely dumb - it just says "use tiles" and GGRS figures out the sizing.

---

## Phase 1: GGRS Core - GeomRenderer Trait Architecture

### 1.1 Design Pattern: Trait-Based Renderers

Following ggplot2's pattern where each Geom has its own `draw_panel()` method:

```r
# ggplot2
GeomPoint$draw_panel(data, panel_params, coord)
GeomTile$draw_panel(data, panel_params, coord)
GeomBar$draw_panel(data, panel_params, coord)
```

We implement this in Rust with a `GeomRenderer` trait:

```rust
// GGRS
point_renderer.render(panel, data, theme)
tile_renderer.render(panel, data, theme)
bar_renderer.render(panel, data, theme)
```

### 1.2 New Module Structure

```
ggrs-core/src/
├── renderer/
│   ├── mod.rs          // GeomRenderer trait + common utilities
│   ├── point.rs        // PointRenderer
│   ├── tile.rs         // TileRenderer
│   ├── bar.rs          // BarRenderer (future)
│   └── line.rs         // LineRenderer (future)
├── geom.rs             // Geom struct (uses renderers)
└── render.rs           // Main render loop (simplified)
```

### 1.3 GeomRenderer Trait

**File**: `ggrs/crates/ggrs-core/src/renderer/mod.rs`

```rust
//! Geometry renderers - one per geom type
//!
//! Each renderer implements the GeomRenderer trait, following ggplot2's
//! pattern where each Geom has its own draw_panel() method.

use crate::colormap::Colormap;
use crate::data::DataPoint;
use crate::error::Result;
use crate::theme::Theme;
use crate::render::PanelContext;
use plotters::prelude::DrawingBackend;

mod point;
mod tile;

pub use point::PointRenderer;
pub use tile::TileRenderer;

/// Trait for rendering geometric objects to a panel
///
/// Mirrors ggplot2's Geom$draw_panel() method.
/// Each geom type implements this trait with its specific rendering logic.
pub trait GeomRenderer: Send + Sync + std::fmt::Debug {
    /// Render data points to the panel
    ///
    /// # Arguments
    /// * `panel` - Panel context with chart, ranges, scales, AND dimensions
    /// * `data` - Data points to render
    /// * `theme` - Theme for styling
    ///
    /// # Errors
    /// Returns error if rendering fails. Does NOT fall back silently.
    ///
    /// # Note on Tile Sizing
    /// For TileRenderer, tile dimensions are calculated internally from:
    /// - panel.width_px / n_unique_x_values
    /// - panel.height_px / n_unique_y_values
    /// The operator does NOT pass tile dimensions.
    fn render<DB: DrawingBackend>(
        &self,
        panel: &mut PanelContext<DB>,
        data: &[DataPoint],
        theme: &Theme,
    ) -> Result<()>;

    /// Get the geom name for error messages
    fn name(&self) -> &'static str;
}

/// Parse hex color string to RGB
///
/// # Panics
/// Panics if hex string is invalid. No fallback to default color.
pub fn parse_hex_color(hex: &str) -> plotters::style::RGBColor {
    use plotters::style::RGBColor;

    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        panic!("Invalid hex color '{}': must be 6 characters", hex);
    }

    let r = u8::from_str_radix(&hex[0..2], 16)
        .unwrap_or_else(|_| panic!("Invalid hex color '{}': bad red component", hex));
    let g = u8::from_str_radix(&hex[2..4], 16)
        .unwrap_or_else(|_| panic!("Invalid hex color '{}': bad green component", hex));
    let b = u8::from_str_radix(&hex[4..6], 16)
        .unwrap_or_else(|_| panic!("Invalid hex color '{}': bad blue component", hex));

    RGBColor(r, g, b)
}
```

### 1.4 PointRenderer

**File**: `ggrs/crates/ggrs-core/src/renderer/point.rs`

```rust
//! Point geometry renderer (scatter plots)
//!
//! Renders data as circles, matching ggplot2's geom_point().

use super::{GeomRenderer, parse_hex_color};
use crate::data::DataPoint;
use crate::error::{GgrsError, Result};
use crate::render::{PanelContext, dequantize_point};
use crate::theme::Theme;
use plotters::prelude::*;

/// Renderer for point geometries (scatter plots)
#[derive(Debug, Clone)]
pub struct PointRenderer {
    /// Point size in mm (ggplot2 default: 1.5mm)
    pub size: f64,
    /// Point shape (0-25, matching ggplot2)
    pub shape: i32,
    /// Stroke width for shapes with borders
    pub stroke: f64,
    /// Alpha transparency (0-1)
    pub alpha: f64,
}

impl Default for PointRenderer {
    fn default() -> Self {
        Self {
            size: 1.5,
            shape: 19,
            stroke: 0.5,
            alpha: 1.0,
        }
    }
}

impl PointRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }

    pub fn with_shape(mut self, shape: i32) -> Self {
        self.shape = shape;
        self
    }
}

impl GeomRenderer for PointRenderer {
    fn render<DB: DrawingBackend>(
        &self,
        panel: &mut PanelContext<DB>,
        data: &[DataPoint],
        _theme: &Theme,
    ) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        // Convert mm to pixels (approximate: 1.5mm * 96 DPI / 25.4 ≈ 5.7px)
        let point_size_px = (self.size * 96.0 / 25.4).round() as i32;

        for point in data {
            // Dequantize coordinates
            let (x, y) = dequantize_point(point.xs, point.ys, panel.x_range, panel.y_range);

            // Apply scale transformation
            let x_transformed = panel.x_scale.transform(x);
            let y_transformed = panel.y_scale.transform(y);

            // Get color (required for points with color aesthetic)
            let color = match &point.color {
                Some(hex) => parse_hex_color(hex),
                None => RGBColor(0, 0, 0), // Default black for points without color mapping
            };

            // Draw circle
            panel
                .chart
                .draw_series(std::iter::once(Circle::new(
                    (x_transformed, y_transformed),
                    point_size_px,
                    ShapeStyle::from(&color).filled(),
                )))
                .map_err(|e| GgrsError::RenderError(format!("Failed to draw point: {}", e)))?;
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "point"
    }
}
```

### 1.5 TileRenderer

**File**: `ggrs/crates/ggrs-core/src/renderer/tile.rs`

```rust
//! Tile geometry renderer (heatmaps)
//!
//! Renders data as filled rectangles, matching ggplot2's geom_tile().
//!
//! ## Tile Sizing (calculated internally by GGRS)
//!
//! Unlike sci_crosstab which uses a fixed cellSize, GGRS calculates tile
//! dimensions at render time from the data:
//!
//! 1. Count unique x positions (from quantized .xs values)
//! 2. Count unique y positions (from quantized .ys values)
//! 3. tile_width_px = panel_width_px / n_unique_x
//! 4. tile_height_px = panel_height_px / n_unique_y
//!
//! The operator does NOT pass tile dimensions - GGRS figures it out.

use super::{GeomRenderer, parse_hex_color};
use crate::colormap::Colormap;
use crate::data::DataPoint;
use crate::error::{GgrsError, Result};
use crate::render::{PanelContext, dequantize_point};
use crate::theme::Theme;
use plotters::prelude::*;
use std::collections::HashSet;

/// Renderer for tile geometries (heatmaps)
#[derive(Debug, Clone)]
pub struct TileRenderer {
    /// Colormap for fill values
    pub colormap: Colormap,
}

impl Default for TileRenderer {
    fn default() -> Self {
        Self {
            colormap: Colormap::Default,
        }
    }
}

impl TileRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_colormap(mut self, colormap: Colormap) -> Self {
        self.colormap = colormap;
        self
    }

    /// Calculate tile dimensions from unique x/y positions in data
    ///
    /// Returns (tile_width_px, tile_height_px)
    fn calculate_tile_dimensions(
        data: &[DataPoint],
        panel_width_px: u32,
        panel_height_px: u32,
    ) -> (f64, f64) {
        // Collect unique quantized x and y values
        let unique_xs: HashSet<u16> = data.iter().map(|p| p.xs).collect();
        let unique_ys: HashSet<u16> = data.iter().map(|p| p.ys).collect();

        let n_x = unique_xs.len().max(1) as f64;
        let n_y = unique_ys.len().max(1) as f64;

        let tile_width = panel_width_px as f64 / n_x;
        let tile_height = panel_height_px as f64 / n_y;

        (tile_width, tile_height)
    }
}

impl GeomRenderer for TileRenderer {
    fn render<DB: DrawingBackend>(
        &self,
        panel: &mut PanelContext<DB>,
        data: &[DataPoint],
        _theme: &Theme,
    ) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }

        // Calculate tile dimensions from data (GGRS does this, not operator)
        let (tile_width_px, tile_height_px) = Self::calculate_tile_dimensions(
            data,
            panel.width_px,
            panel.height_px,
        );

        // Convert pixel dimensions to data units for positioning
        let x_range_size = panel.x_range.1 - panel.x_range.0;
        let y_range_size = panel.y_range.1 - panel.y_range.0;

        let tile_width_data = (tile_width_px / panel.width_px as f64) * x_range_size;
        let tile_height_data = (tile_height_px / panel.height_px as f64) * y_range_size;

        let half_width = tile_width_data / 2.0;
        let half_height = tile_height_data / 2.0;

        for point in data {
            // Dequantize center coordinates
            let (cx, cy) = dequantize_point(point.xs, point.ys, panel.x_range, panel.y_range);

            // Calculate tile corners (ggplot2: xmin = x - width/2, etc.)
            let x_min = panel.x_scale.transform(cx - half_width);
            let x_max = panel.x_scale.transform(cx + half_width);
            let y_min = panel.y_scale.transform(cy - half_height);
            let y_max = panel.y_scale.transform(cy + half_height);

            // Get fill color (required for heatmaps)
            let color = match &point.color {
                Some(hex) => parse_hex_color(hex),
                None => {
                    return Err(GgrsError::RenderError(
                        "TileRenderer: tile missing fill color. Heatmaps require color mapping.".to_string()
                    ));
                }
            };

            // Draw filled rectangle
            panel
                .chart
                .draw_series(std::iter::once(Rectangle::new(
                    [(x_min, y_min), (x_max, y_max)],
                    color.filled(),
                )))
                .map_err(|e| GgrsError::RenderError(format!("Failed to draw tile: {}", e)))?;
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "tile"
    }
}
```

**Key insight**: `PanelContext` needs `width_px` and `height_px` fields so renderers can calculate pixel-based dimensions. This is already available at render time.

### 1.5.1 PanelContext Update

**File**: `ggrs/crates/ggrs-core/src/render.rs`

The `PanelContext` struct needs panel dimensions for TileRenderer to calculate tile sizes:

```rust
/// Context for rendering to a single panel
pub struct PanelContext<DB: DrawingBackend> {
    pub chart: ChartContext<DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    pub x_range: (f64, f64),
    pub y_range: (f64, f64),
    pub x_scale: Scale,
    pub y_scale: Scale,
    // NEW: Panel dimensions in pixels (needed for tile sizing)
    pub width_px: u32,
    pub height_px: u32,
}
```

These values are known when creating the panel and should be passed through.

### 1.6 Update Geom Struct

**File**: `ggrs/crates/ggrs-core/src/geom.rs`

```rust
use crate::renderer::{GeomRenderer, PointRenderer, TileRenderer};
use crate::colormap::Colormap;

/// A geometry layer with its renderer
///
/// Each geom type is self-contained - it calculates what it needs
/// from the panel context and data at render time.
#[derive(Debug, Clone)]
pub struct Geom {
    /// The renderer for this geometry
    pub renderer: Box<dyn GeomRenderer>,
    /// Aesthetic mappings for this layer (if overriding plot defaults)
    pub aes: Option<Aes>,
}

impl Geom {
    /// Create a point geometry layer (scatter plot)
    pub fn point() -> Self {
        Self {
            renderer: Box::new(PointRenderer::default()),
            aes: None,
        }
    }

    /// Create a point geometry with custom size
    pub fn point_sized(size: f64) -> Self {
        Self {
            renderer: Box::new(PointRenderer::new().with_size(size)),
            aes: None,
        }
    }

    /// Create a tile geometry (heatmap)
    ///
    /// Tile dimensions are calculated automatically at render time
    /// from the unique x/y positions in the data.
    pub fn tile() -> Self {
        Self {
            renderer: Box::new(TileRenderer::default()),
            aes: None,
        }
    }

    /// Create a tile geometry with colormap
    ///
    /// Tile dimensions are calculated automatically at render time.
    pub fn tile_with_colormap(colormap: Colormap) -> Self {
        Self {
            renderer: Box::new(TileRenderer::new().with_colormap(colormap)),
            aes: None,
        }
    }

    /// Set aesthetic mappings for this layer
    pub fn with_aes(mut self, aes: Aes) -> Self {
        self.aes = Some(aes);
        self
    }
}
```

### 1.7 Simplified Render Loop

**File**: `ggrs/crates/ggrs-core/src/render.rs`

The render loop becomes clean and extensible:

```rust
// In the chunk rendering loop
for (panel_idx, points) in chunk_buckets.iter() {
    let layer = &self.generator.spec().layers[0];

    // Render using the layer's renderer
    // Note: TileRenderer calculates tile dimensions internally from data
    layer.renderer.render(&mut panels[*panel_idx], points, theme)?;
}
```

No match statements, no if/else chains, no tile dimension passing. Adding a new geom type just requires:
1. Create `NewGeomRenderer` struct
2. Implement `GeomRenderer` trait
3. Add `Geom::new_geom()` constructor

Each renderer is self-contained and calculates what it needs from the panel context and data.

---

## Phase 2: Operator Refactoring - Separation of Concerns

Before adding heatmap support, refactor to cleanly separate responsibilities.

### 2.0 Current Problems

**TercenStreamGenerator holds too much:**
- `aes`, `facet_spec`, `cached_legend_scale` are plot configuration, not data streaming

**OperatorConfig mixes concerns:**
- `legend_position`, `plot_title`, `point_size` are visual config, not operator settings

**main.rs does ad-hoc extraction:**
- Color info, theme building, PlotSpec building all inline

### 2.1 New Architecture

```
┌───────────────────────────────────────────────────────────┐
│                     PlotMetadata                          │
│  Source: CubeQuery + Workflow + Step                      │
├───────────────────────────────────────────────────────────┤
│  chart_kind: ChartKind        // Point, Tile, Bar, Line   │
│  aes: Aes                     // aesthetic mappings       │
│  facet_spec: FacetSpec        // grid/row/col layout      │
│  legend_scale: LegendScale    // legend data              │
│  colormap: Colormap           // visual palette           │
│  labels: PlotLabels           // title, x_label, y_label  │
│  theme_config: ThemeConfig    // legend pos, justification│
│  point_size: f64              // from crosstab aesthetics │
└───────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────┐
│                TercenStreamGenerator                      │
│  Source: Table connections + schemas                      │
├───────────────────────────────────────────────────────────┤
│  client: Arc<TercenClient>    // gRPC connection          │
│  main_table_id: String        // data table               │
│  facet_info: FacetInfo        // for .ci/.ri filtering    │
│  axis_ranges: HashMap<...>    // for dequantization       │
│  color_infos: Vec<ColorInfo>  // for add_color_columns()  │
│  total_rows: usize            // schema row count         │
│  chunk_size: usize            // streaming config         │
│  (NO aes, facet_spec, legend_scale!)                      │
└───────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────┐
│                   OperatorConfig                          │
│  Source: operator.json properties                         │
├───────────────────────────────────────────────────────────┤
│  backend: String              // "cpu" or "gpu"           │
│  chunk_size: usize            // streaming chunk size     │
│  png_compression: String      // "fast", "default", "best"│
│  plot_width: PlotDimension    // Auto or Pixels           │
│  plot_height: PlotDimension   // Auto or Pixels           │
│  (NO legend_position, plot_title, point_size!)            │
└───────────────────────────────────────────────────────────┘
```

### 2.2 PlotMetadata Module

**File**: `ggrs_plot_operator/src/tercen/metadata.rs`

```rust
//! Plot metadata extracted from Tercen CubeQuery/Workflow
//!
//! Separate from StreamGenerator - this is chart configuration,
//! not data streaming.

use ggrs_core::{aes::Aes, legend::LegendScale, stream::FacetSpec, theme::LegendPosition};

/// Chart types as defined by Tercen Crosstab
#[derive(Debug, Clone, PartialEq)]
pub enum ChartKind {
    Point,
    Heatmap,
    Bar,
    Line,
}

/// Plot labels (title, axis labels)
#[derive(Debug, Clone, Default)]
pub struct PlotLabels {
    pub title: Option<String>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
}

/// Theme configuration overrides
#[derive(Debug, Clone)]
pub struct ThemeConfig {
    pub legend_position: LegendPosition,
    pub legend_justification: Option<(f64, f64)>,
    pub plot_title_position: String,
    pub plot_title_justification: Option<(f64, f64)>,
}

/// Plot metadata - everything GGRS needs to know about the visualization
///
/// This is extracted from CubeQuery/Workflow, NOT from operator.json.
/// The operator just passes this to GGRS.
#[derive(Debug, Clone)]
pub struct PlotMetadata {
    pub chart_kind: ChartKind,
    pub aes: Aes,
    pub facet_spec: FacetSpec,
    pub legend_scale: LegendScale,
    pub labels: PlotLabels,
    pub theme_config: ThemeConfig,
    pub point_size: f64,  // From crosstab aesthetics
}

impl PlotMetadata {
    /// Extract plot metadata from CubeQuery and related sources
    pub async fn from_tercen(
        client: &TercenClient,
        cube_query: &CubeQuery,
        workflow: &Workflow,
        step_id: &str,
        facet_info: &FacetInfo,      // For building facet_spec
        color_infos: &[ColorInfo],    // For building legend_scale
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Extract chart_kind from step configuration
        let chart_kind = Self::extract_chart_kind(workflow, step_id)?;

        // Build Aes based on chart type and color presence
        let aes = Self::build_aes(&chart_kind, !color_infos.is_empty());

        // Build FacetSpec from facet_info
        let facet_spec = Self::build_facet_spec(facet_info);

        // Build LegendScale from color_infos
        let legend_scale = Self::build_legend_scale(client, color_infos).await?;

        // Extract labels from step/workflow config
        let labels = Self::extract_labels(workflow, step_id)?;

        // Extract theme config
        let theme_config = Self::extract_theme_config(workflow, step_id)?;

        // Extract point size from crosstab aesthetics
        let point_size = Self::extract_point_size(workflow, step_id).unwrap_or(4.0);

        Ok(Self {
            chart_kind,
            aes,
            facet_spec,
            legend_scale,
            labels,
            theme_config,
            point_size,
        })
    }

    fn extract_chart_kind(workflow: &Workflow, step_id: &str) -> Result<ChartKind, ...> {
        // Look at step's chartKind field
        // "ChartHeatmap" -> ChartKind::Heatmap
        // "ChartBar" -> ChartKind::Bar
        // default -> ChartKind::Point
    }

    fn build_aes(chart_kind: &ChartKind, has_colors: bool) -> Aes {
        let mut aes = Aes::new().x(".x").y(".y");
        if has_colors {
            aes = aes.color(".color");
        }
        // For heatmaps, might use .fill instead of .color
        aes
    }

    fn build_facet_spec(facet_info: &FacetInfo) -> FacetSpec {
        // Same logic currently in TercenStreamGenerator::new()
        // Move it here
    }
}
```

### 2.3 Update TercenStreamGenerator - Remove Plot Config

**File**: `ggrs_plot_operator/src/ggrs_integration/stream_generator.rs`

Remove from struct:
- `aes` - moved to PlotMetadata
- `facet_spec` - moved to PlotMetadata
- `cached_legend_scale` - moved to PlotMetadata
- `page_factors` - only for debugging, remove

Keep in struct:
- `client`, `main_table_id`, `chunk_size`, `total_rows`
- `facet_info` - needed for filtering
- `axis_ranges` - needed for query_x/y_axis()
- `color_infos` - needed for add_color_columns()

The StreamGenerator trait methods that return plot config (`aes()`, `facet_spec()`, `query_legend_scale()`) will need to be reconsidered - either:
- A) Return references to PlotMetadata (requires lifetime changes)
- B) Keep minimal versions in StreamGenerator, sync with PlotMetadata
- C) Change GGRS trait to not require these (breaking change)

### 2.4 Update OperatorConfig - Remove Visual Config

Move OUT of OperatorConfig:
- `legend_position` → PlotMetadata.theme_config
- `legend_position_inside` → PlotMetadata.theme_config
- `legend_justification` → PlotMetadata.theme_config
- `plot_title`, `plot_title_position`, `plot_title_justification` → PlotMetadata.labels
- `x_axis_label`, `y_axis_label` → PlotMetadata.labels
- `point_size` → PlotMetadata

Keep in OperatorConfig:
- `backend`, `chunk_size`, `png_compression`
- `plot_width`, `plot_height` (auto-dimension logic)

### 2.5 Update main.rs - Orchestrate Clean Separation

```rust
async fn process_task(client: Arc<TercenClient>, task_id: &str) -> Result<()> {
    // 1. Load operator config (from operator.json properties)
    let config = OperatorConfig::from_properties(operator_settings.as_ref());

    // 2. Create stream generator (data streaming only)
    let stream_gen = TercenStreamGenerator::new(
        client.clone(),
        table_ids,
        config.chunk_size,
        color_infos.clone(),  // For data transformation
    ).await?;

    // 3. Extract plot metadata (from CubeQuery/Workflow)
    let metadata = PlotMetadata::from_tercen(
        &client,
        &cube_query,
        &workflow,
        &step_id,
        stream_gen.facet_info(),  // Reference, not moved
        &color_infos,              // For legend
    ).await?;

    // 4. Build GGRS components from metadata
    let geom = match metadata.chart_kind {
        ChartKind::Heatmap => Geom::tile(),
        ChartKind::Point => Geom::point_sized(metadata.point_size),
        _ => panic!("Unsupported chart type"),
    };

    let theme = Theme {
        legend_position: metadata.theme_config.legend_position,
        legend_justification: metadata.theme_config.legend_justification,
        ..Default::default()
    };

    let plot_spec = EnginePlotSpec::new()
        .add_layer(geom)
        .theme(theme)
        .title(metadata.labels.title)
        .x_label(metadata.labels.x_label)
        .y_label(metadata.labels.y_label);

    // 5. Render
    let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;
    // ...
}
```

This is a bigger refactoring than just adding heatmaps. It should be done BEFORE or AS PART OF the heatmap work.

### 2.6 GGRS Architecture Change: Clean Separation

The current GGRS `StreamGenerator` trait mixes data streaming with plot configuration.
This should be fixed as part of the heatmap work.

#### Current (Mixed Concerns)
```rust
trait StreamGenerator {
    // Data concerns ✓
    fn query_data_chunk(...) -> DataFrame;
    fn n_total_data_rows(&self) -> usize;
    fn query_x_axis(...) -> AxisData;

    // Plot configuration ✗ (doesn't belong here)
    fn aes(&self) -> &Aes;
    fn facet_spec(&self) -> &FacetSpec;
    fn query_legend_scale(&self) -> LegendScale;
}
```

#### Target Architecture

**StreamGenerator = Pure Data Source**
```rust
trait StreamGenerator {
    // Data streaming
    fn n_total_data_rows(&self) -> usize;
    fn query_data_chunk(&self, range: Range) -> DataFrame;
    fn query_data_multi_facet(&self, range: Range) -> DataFrame;

    // Coordinate system (for dequantization)
    fn query_x_axis(&self, col: usize, row: usize) -> AxisData;
    fn query_y_axis(&self, col: usize, row: usize) -> AxisData;

    // Facet structure (inherent in data)
    fn n_col_facets(&self) -> usize;
    fn n_row_facets(&self) -> usize;
    fn query_col_facet_labels(&self) -> DataFrame;
    fn query_row_facet_labels(&self) -> DataFrame;
    fn get_original_col_idx(&self, col: usize) -> usize;
    fn get_original_row_idx(&self, row: usize) -> usize;

    fn preferred_chunk_size(&self) -> Option<usize>;
}
```

**PlotSpec = All Plot Configuration**
```rust
// Expand EnginePlotSpec to include aes, facet_spec, legend_scale
let plot_spec = PlotSpec::new()
    .aes(Aes::new().x(".x").y(".y").color(".color"))
    .facet_grid(".ri", ".ci", FacetScales::FreeY)
    .legend_scale(legend_scale)
    .add_layer(Geom::tile())
    .theme(theme)
    .title("My Plot")
    .x_label("X Axis");

// PlotGenerator takes data source + plot config
let plot_gen = PlotGenerator::new(stream_gen, plot_spec)?;
```

#### Benefits

| Aspect | Benefit |
|--------|---------|
| **Matches ggplot2** | Everything added to plot object, data source is separate |
| **Single responsibility** | StreamGenerator = data, PlotSpec = configuration |
| **Testability** | Test rendering with mock data, test streaming with mock plots |
| **Reusability** | Same PlotSpec with different data sources |
| **Operator simplicity** | Clear separation of what comes from where |

#### Data Flow After Refactoring

```
Tercen CubeQuery/Workflow
         │
         ├──────────────────────┐
         ▼                      ▼
   PlotMetadata           TercenStreamGenerator
   (extracted from        (pure data streaming,
    Tercen config)         no plot config)
         │                      │
         ▼                      │
     PlotSpec                   │
   (aes, facet_spec,            │
    geom, legend,               │
    theme, labels)              │
         │                      │
         └──────────┬───────────┘
                    ▼
              PlotGenerator
                    │
                    ▼
                  PNG
```

### 2.1 No Operator Properties Needed

The geom type is NOT a user-configurable property. Tercen's Crosstab already knows
what chart type to render (ChartPoint, ChartHeatmap, ChartBar, etc.) and passes
this through the CubeQuery.

### 2.2 Separation of Concerns: PlotMetadata vs StreamGenerator

**Design principle**: The StreamGenerator should only stream data. Chart configuration
is a separate concern that should live in its own struct.

```
┌───────────────────────────┐     ┌────────────────────┐
│   TercenStreamGenerator   │     │    PlotMetadata    │
│   - streams data chunks   │     │    - chart_kind    │
│   - TSON → DataFrame      │     │    - colormap      │
│   - facet filtering       │     │    - point_size    │
│   - color column addition │     │    - theme config  │
└───────────────────────────┘     └────────────────────┘
              │                            │
              └────────────┬───────────────┘
                           ▼
                    ┌─────────────┐
                    │   main()    │
                    │ orchestrates│
                    └─────────────┘
```

**File**: `ggrs_plot_operator/src/tercen/metadata.rs` (new module)

```rust
/// Chart types as defined by Tercen Crosstab
#[derive(Debug, Clone, PartialEq)]
pub enum ChartKind {
    Point,    // ChartPoint -> Geom::point()
    Heatmap,  // ChartHeatmap -> Geom::tile()
    Bar,      // ChartBar -> Geom::bar() (future)
    Line,     // ChartLine -> Geom::line() (future)
}

/// Plot metadata extracted from CubeQuery/Crosstab
///
/// Separate from StreamGenerator - this is chart configuration,
/// not data streaming.
#[derive(Debug, Clone)]
pub struct PlotMetadata {
    pub chart_kind: ChartKind,
    pub colormap: Colormap,
    // ... other chart configuration
}

impl PlotMetadata {
    /// Extract plot metadata from CubeQuery
    pub fn from_cube_query(cube_query: &CubeQuery) -> Result<Self> {
        let chart_kind = match cube_query.chart_kind.as_str() {
            "ChartHeatmap" => ChartKind::Heatmap,
            "ChartBar" => ChartKind::Bar,
            "ChartLine" => ChartKind::Line,
            _ => ChartKind::Point,
        };

        let colormap = Self::extract_colormap(cube_query)?;

        Ok(Self {
            chart_kind,
            colormap,
        })
    }
}
```

### 2.3 TercenStreamGenerator - NO Chart Metadata

**File**: `ggrs_plot_operator/src/ggrs_integration/stream_generator.rs`

The StreamGenerator stays focused on its single responsibility - streaming data:

```rust
pub struct TercenStreamGenerator {
    // Data streaming concerns ONLY
    client: TercenClient,
    table_id: String,
    chunk_size: usize,
    facet_metadata: FacetMetadata,
    color_info: ColorInfo,
    // ...

    // NO chart_kind here - that's PlotMetadata's job
}
```

### 2.4 Update Main - Orchestrate Both

**File**: `ggrs_plot_operator/src/main.rs`

```rust
// Extract metadata from CubeQuery (chart config)
let metadata = PlotMetadata::from_cube_query(&cube_query)?;

// Create stream generator (data streaming)
let stream_generator = TercenStreamGenerator::new(client, table_id, ...)?;

// Create geom based on metadata - clean separation
let geom = match metadata.chart_kind {
    ChartKind::Heatmap => Geom::tile_with_colormap(metadata.colormap),
    ChartKind::Point => Geom::point_sized(config.point_size),
    ChartKind::Bar => panic!("Bar charts not yet implemented"),
    ChartKind::Line => panic!("Line charts not yet implemented"),
};

let plot_spec = EnginePlotSpec::new()
    .add_layer(geom)
    .theme(theme);

// NO tile dimension passing needed - GGRS figures it out from data
```

**Key insight**:
- `PlotMetadata` = what to render (chart type, colors, styling)
- `TercenStreamGenerator` = how to get data (streaming, conversion, filtering)
- `main()` = orchestrates both, doesn't mix concerns

---

## Phase 3: Testing

### 3.1 Unit Tests - Renderers

**File**: `ggrs/crates/ggrs-core/src/renderer/tile.rs` (tests module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_tile_dimensions_10x10() {
        // 10x10 grid in 800x600 panel
        let data: Vec<DataPoint> = (0..10)
            .flat_map(|x| (0..10).map(move |y| DataPoint {
                xs: (x * 6553) as u16,  // Evenly spaced in quantized space
                ys: (y * 6553) as u16,
                color: Some("#FF0000".to_string()),
            }))
            .collect();

        let (tile_w, tile_h) = TileRenderer::calculate_tile_dimensions(&data, 800, 600);

        assert!((tile_w - 80.0).abs() < 0.1);  // 800 / 10 = 80
        assert!((tile_h - 60.0).abs() < 0.1);  // 600 / 10 = 60
    }

    #[test]
    fn test_calculate_tile_dimensions_5x20() {
        // 5 columns, 20 rows in 800x600 panel
        let data: Vec<DataPoint> = (0..5)
            .flat_map(|x| (0..20).map(move |y| DataPoint {
                xs: (x * 13107) as u16,
                ys: (y * 3276) as u16,
                color: Some("#FF0000".to_string()),
            }))
            .collect();

        let (tile_w, tile_h) = TileRenderer::calculate_tile_dimensions(&data, 800, 600);

        assert!((tile_w - 160.0).abs() < 0.1);  // 800 / 5 = 160 (wide tiles)
        assert!((tile_h - 30.0).abs() < 0.1);   // 600 / 20 = 30 (short tiles)
    }

    #[test]
    fn test_tile_corners_from_center() {
        // Tile centered at (100, 200) with width=20, height=30
        let cx = 100.0;
        let cy = 200.0;
        let half_w = 10.0;
        let half_h = 15.0;

        let x_min = cx - half_w;
        let x_max = cx + half_w;
        let y_min = cy - half_h;
        let y_max = cy + half_h;

        assert_eq!(x_min, 90.0);
        assert_eq!(x_max, 110.0);
        assert_eq!(y_min, 185.0);
        assert_eq!(y_max, 215.0);
    }

    #[test]
    fn test_empty_data_returns_panel_size() {
        // Edge case: empty data should return full panel as single tile
        let data: Vec<DataPoint> = vec![];
        let (tile_w, tile_h) = TileRenderer::calculate_tile_dimensions(&data, 800, 600);

        // With max(1), we get panel_size / 1 = panel_size
        assert!((tile_w - 800.0).abs() < 0.1);
        assert!((tile_h - 600.0).abs() < 0.1);
    }
}
```

**File**: `ggrs/crates/ggrs-core/src/renderer/mod.rs` (tests)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color_valid() {
        assert_eq!(parse_hex_color("#FF0000"), RGBColor(255, 0, 0));
        assert_eq!(parse_hex_color("00FF00"), RGBColor(0, 255, 0));
        assert_eq!(parse_hex_color("#0000ff"), RGBColor(0, 0, 255));
    }

    #[test]
    #[should_panic(expected = "Invalid hex color")]
    fn test_parse_hex_color_invalid_length() {
        parse_hex_color("#FFF");
    }

    #[test]
    #[should_panic(expected = "Invalid hex color")]
    fn test_parse_hex_color_invalid_chars() {
        parse_hex_color("#GGGGGG");
    }
}
```

### 3.2 Integration Test - GGRS Example

**File**: `ggrs/crates/ggrs-core/examples/basic_heatmap.rs`

```rust
//! Basic heatmap example - tests tile rendering
//!
//! Note: Tile dimensions are calculated internally by TileRenderer.
//! The user just specifies Geom::tile() and GGRS handles sizing.

use ggrs_core::{
    aes, DataFrame, EnginePlotSpec, FacetSpec, Geom,
    InMemoryStreamGenerator, PlotGenerator, PlotRenderer, Theme,
};
use ggrs_core::renderer::{BackendChoice, OutputFormat};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create 10x10 heatmap data
    let mut records = Vec::new();
    for y in 0..10 {
        for x in 0..10 {
            records.push(vec![
                ("x".to_string(), (x as f64).into()),
                ("y".to_string(), (y as f64).into()),
                ("value".to_string(), ((x + y) as f64 / 18.0).into()),
            ]);
        }
    }
    let df = DataFrame::from_records(records)?;

    let aes = aes().x("x").y("y").fill("value");
    let facet_spec = FacetSpec::none();
    let stream_gen = InMemoryStreamGenerator::new(df, aes, facet_spec)?;

    let plot_spec = EnginePlotSpec::new()
        .title("10x10 Heatmap")
        .add_layer(Geom::tile())  // Just specify tile - GGRS calculates dimensions
        .theme(Theme::gray());

    let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;

    // NO tile dimension setting needed!
    // TileRenderer counts unique x/y values and calculates:
    // - tile_width = 800 / 10 = 80px
    // - tile_height = 600 / 10 = 60px

    let renderer = PlotRenderer::new(&plot_gen, 800, 600);

    renderer.render_to_file(
        "tests/outputs/basic_heatmap.png",
        BackendChoice::Cairo,
        OutputFormat::Png,
    )?;

    println!("Saved heatmap to tests/outputs/basic_heatmap.png");
    Ok(())
}
```

### 3.3 Manual Testing Checklist

| Test Case | Expected Result |
|-----------|-----------------|
| 10×10 heatmap, default colormap | Blue gradient, tiles fill space (80×60px each) |
| 10×10 heatmap, jet colormap | Rainbow gradient |
| 5×20 heatmap (non-square) | Wide tiles (160×30px in 800×600) |
| 20×5 heatmap (non-square) | Tall tiles (40×120px in 800×600) |
| 3×3 faceted heatmap | 9 panels, each with correct tiles |
| Large 100×100 heatmap | Renders without error |
| Missing color data | Panics with clear error |
| Unknown geom type | Panics with clear error |
| Single cell heatmap | One tile fills entire panel |

---

## Phase 4: Verification Against ggplot2

### 4.1 Create R Reference Script

**File**: `ggrs/tests/equivalence/r_scripts/heatmap.R`

```r
library(ggplot2)

# Generate same 10x10 data
df <- expand.grid(x = 0:9, y = 0:9)
df$value <- (df$x + df$y) / 18

# Basic heatmap
p1 <- ggplot(df, aes(x, y, fill = value)) +
  geom_tile() +
  scale_fill_gradient(low = "#132B43", high = "#56B1F7") +
  theme_gray() +
  ggtitle("10x10 Heatmap")

ggsave("reference/basic_heatmap_r.png", p1, width = 8, height = 6)

# Jet colormap
p2 <- ggplot(df, aes(x, y, fill = value)) +
  geom_tile() +
  scale_fill_gradientn(colours = c("#00008F", "#0000FF", "#00FFFF",
                                    "#00FF00", "#FFFF00", "#FF0000")) +
  theme_gray() +
  ggtitle("Jet Heatmap")

ggsave("reference/jet_heatmap_r.png", p2, width = 8, height = 6)
```

### 4.2 Visual Comparison Criteria

- Tile positions match (centered at data points)
- Tile sizes match (fill space without gaps)
- Colors match colormap
- No visual artifacts at tile edges

---

## Implementation Order

### Week 1: GGRS Architecture Refactoring
**Goal**: Clean separation between data streaming and plot configuration

1. **GGRS: Simplify StreamGenerator trait**
   - Remove `aes()`, `facet_spec()`, `query_legend_scale()` methods
   - Keep only data streaming and coordinate system methods

2. **GGRS: Expand PlotSpec (EnginePlotSpec)**
   - Add `.aes(Aes)` method
   - Add `.facet_grid()`, `.facet_row()`, `.facet_col()` methods
   - Add `.legend_scale(LegendScale)` method

3. **GGRS: Update PlotGenerator**
   - Adjust to get aes/facet_spec/legend from PlotSpec instead of StreamGenerator

4. **Operator: Update TercenStreamGenerator**
   - Remove aes, facet_spec, cached_legend_scale fields
   - Remove trait method implementations for removed methods

5. **Operator: Create PlotMetadata**
   - Extract ChartKind from CubeQuery
   - Build Aes, FacetSpec, LegendScale
   - Extract labels and theme config

6. **Operator: Update main.rs**
   - Use PlotMetadata to build PlotSpec
   - Clean orchestration of components

### Week 2: GGRS Core - GeomRenderer Architecture
**Goal**: Trait-based geom rendering for extensibility

7. Add `width_px`, `height_px` fields to `PanelContext`
8. Create `renderer/` module structure
9. Define `GeomRenderer` trait (just `render()` and `name()`)
10. Implement `PointRenderer` (move existing code)
11. Implement `TileRenderer` with internal `calculate_tile_dimensions()`
12. Update `Geom` struct to use renderers
13. Simplify render loop (no if/else dispatch)
14. Unit tests for renderers

### Week 3: Integration & Validation
**Goal**: Working heatmaps with clean architecture

15. Update GGRS examples (basic_heatmap.rs)
16. Update operator to select Geom based on `metadata.chart_kind`
17. Integration tests with Tercen heatmap workflows
18. Visual comparison with ggplot2 reference
19. Edge case testing (single cell, non-square, large grids)
20. Documentation updates

---

## Open Questions for Tercen Team

1. **Exact field path for chartKind in CubeQuery?**
   - What's the protobuf field name/path to extract the chart type?
   - Example values: "ChartHeatmap", "ChartPoint", "ChartBar"?

2. **Color palette for heatmaps**
   - Same JetPalette structure as scatter?
   - Need to extract min/max for normalization?

## Resolved Questions

1. ~~**How are `nXLevels` and `nYLevels` provided?**~~
   - **Answer**: They're NOT used. GGRS calculates tile dimensions from unique x/y positions in the data itself.
   - This matches how sci_crosstab works: `panel_size / n_unique_values`

2. ~~**How does operator know when to use `Geom::tile()` vs `Geom::point()`?**~~
   - **Answer**: From the Crosstab/CubeQuery `chartKind` field. Tercen decides the chart type, not the operator.

---

## Future Extensions

With the `GeomRenderer` trait in place, adding new geoms is straightforward:

```rust
// Future: BarRenderer
pub struct BarRenderer {
    pub position: Position,
    pub width: f64,
}

impl GeomRenderer for BarRenderer {
    fn render<DB: DrawingBackend>(...) -> Result<()> {
        // Draw bars
    }
    fn name(&self) -> &'static str { "bar" }
}

// Future: LineRenderer
pub struct LineRenderer {
    pub width: f64,
    pub linetype: LineType,
}

impl GeomRenderer for LineRenderer {
    fn render<DB: DrawingBackend>(...) -> Result<()> {
        // Draw lines
    }
    fn name(&self) -> &'static str { "line" }
}
```

Each new geom:
1. Create struct with geom-specific parameters
2. Implement `GeomRenderer` trait
3. Add constructor to `Geom`
4. Done - no changes to render loop

---

## Success Criteria

### Architecture Goals
1. **StreamGenerator is pure data source** - no aes(), facet_spec(), query_legend_scale()
2. **PlotSpec contains all plot configuration** - aes, facet_spec, legend_scale, geom, theme
3. **Operator has clean separation** - PlotMetadata (from Tercen) vs OperatorConfig (from operator.json)
4. **TercenStreamGenerator is minimal** - only data streaming, no plot config

### Functional Goals
5. Basic 10×10 heatmap renders correctly (tiles auto-sized to 80×60px in 800×600)
6. Non-square grids render correctly (wide/tall tiles as expected)
7. Faceted heatmaps work
8. Both colormaps (Default, Jet) work
9. Visual output matches ggplot2 reference
10. Missing color data causes clear panics (no silent fallbacks)
11. All existing scatter plot tests still pass

### Extensibility Goals
12. Adding new geom types requires no changes to render loop
13. **Operator does NOT pass tile dimensions** - GGRS TileRenderer calculates internally
14. **Chart type comes from Tercen** - operator reads chartKind from CubeQuery
