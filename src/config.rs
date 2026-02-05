//! Operator configuration from Tercen properties
//!
//! Configuration is loaded from operator properties (defined in operator.json).
//! All default values come from operator.json - no hardcoded fallbacks in this code.
//!
//! Property definitions and defaults are parsed from operator.json at compile time
//! via the `OperatorPropertyReader` which ensures single-source-of-truth for defaults.

use crate::tercen::client::proto::OperatorSettings;
use crate::tercen::operator_properties::OperatorPropertyReader;
use crate::tercen::properties::PlotDimension;

/// How to aggregate multiple data points in the same heatmap cell
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeatmapCellAggregation {
    /// Use the last data point (matches Tercen's default overdraw behavior)
    #[default]
    Last,
    /// Use the first data point
    First,
    /// Compute the mean of all data points
    Mean,
    /// Compute the median of all data points
    Median,
}

impl HeatmapCellAggregation {
    /// Parse from string value
    ///
    /// This is an internal enum - validation happens in OperatorPropertyReader.get_enum()
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "first" => Self::First,
            "mean" => Self::Mean,
            "median" => Self::Median,
            _ => Self::Last, // "last" or any other value
        }
    }
}

#[derive(Debug, Clone)]
pub struct OperatorConfig {
    /// Number of rows per chunk (default: 10000, not in operator.json)
    pub chunk_size: usize,

    /// Theme name: "gray", "bw", "minimal"
    /// Matches ggplot2's theme_gray(), theme_bw(), theme_minimal()
    pub theme: String,

    /// Plot width (pixels or Auto)
    pub plot_width: PlotDimension,

    /// Plot height (pixels or Auto)
    pub plot_height: PlotDimension,

    /// Render backend: "cpu" or "gpu"
    pub backend: String,

    /// Point size in pixels (derived from UI scale 1-10)
    pub point_size: f64,

    /// Legend position: "left", "right", "top", "bottom", "inside", "none"
    /// Matches ggplot2's legend.position theme setting
    pub legend_position: String,

    /// Legend position inside plot (only used when legend.position = "inside")
    /// Format: "x,y" where x,y ∈ [0,1], e.g., "0.95,0.05" for bottom-right
    /// Matches ggplot2's legend.position.inside theme setting (ggplot2 3.5.0+)
    pub legend_position_inside: Option<(f64, f64)>,

    /// Legend justification (anchor point of legend box): "x,y" where x,y ∈ [0,1]
    /// Controls which point of the legend box is anchored at the position
    /// - c(0,0) = bottom-left corner
    /// - c(1,1) = top-right corner
    /// - c(0.5,0.5) = center (default)
    ///
    /// Matches ggplot2's legend.justification theme setting
    ///
    /// For edge positions (left/right/top/bottom):
    ///   - Controls alignment along that edge
    ///   - For left/right: y-component controls vertical placement (0=bottom, 1=top)
    ///   - For top/bottom: x-component controls horizontal placement (0=left, 1=right)
    ///
    /// For inside position:
    ///   - Determines which corner of legend aligns with legend.position.inside coords
    ///
    /// Examples:
    ///   - legend.position="left", legend.justification="0,0.5" → left edge, center vertically
    ///   - legend.position="top", legend.justification="0.5,1" → top edge, center horizontally
    ///   - legend.position="inside", legend.position.inside="0.95,0.05",
    ///     legend.justification="1,0" → bottom-right corner of legend at (0.95,0.05)
    pub legend_justification: Option<(f64, f64)>,

    /// PNG compression level: "fast", "default", "best"
    /// - "fast": Fastest encoding (~30% speedup), larger files (+15%)
    /// - "default": Balanced (current behavior)
    /// - "best": Slowest encoding (~40% slower), smallest files (-10%)
    pub png_compression: String,

    /// Plot title (optional)
    pub plot_title: Option<String>,

    /// Plot title position: "top", "bottom", "left", "right"
    pub plot_title_position: String,

    /// Plot title justification (anchor point): (x, y) where x,y ∈ [0,1]
    pub plot_title_justification: Option<(f64, f64)>,

    /// X-axis label (optional)
    pub x_axis_label: Option<String>,

    /// Y-axis label (optional)
    pub y_axis_label: Option<String>,

    /// X-axis tick label rotation in degrees (0 = horizontal, 90 = vertical)
    pub x_tick_rotation: f64,

    /// Y-axis tick label rotation in degrees (0 = horizontal, 90 = vertical)
    pub y_tick_rotation: f64,

    /// How to aggregate multiple data points in the same heatmap cell
    pub heatmap_cell_aggregation: HeatmapCellAggregation,

    /// Point shapes per layer (ggplot2 pch values 0-25)
    /// Cycles through layers based on .axisIndex.
    /// Common shapes: 19=filled circle, 15=filled square, 17=filled triangle
    pub layer_shapes: Vec<i32>,
}

impl OperatorConfig {
    /// Create config from operator properties
    ///
    /// All default values come from operator.json via OperatorPropertyReader.
    /// No hardcoded fallbacks in this function.
    ///
    /// # Arguments
    /// * `operator_settings` - Operator settings from Tercen
    /// * `ui_point_size` - Point size from crosstab model (UI scale 1-10), None = use default (4)
    pub fn from_properties(
        operator_settings: Option<&OperatorSettings>,
        ui_point_size: Option<i32>,
    ) -> Self {
        let props = OperatorPropertyReader::new(operator_settings);

        // Theme: validated enum (gray, bw, minimal)
        let theme = props.get_enum("theme");

        // Parse plot dimensions with "auto" support
        // Empty string or "auto" → Auto (derive from facet count)
        // "1500" → Pixels(1500) if valid range [100-10000]
        let plot_width =
            PlotDimension::from_str(&props.get_string("plot.width"), PlotDimension::Auto);

        let plot_height =
            PlotDimension::from_str(&props.get_string("plot.height"), PlotDimension::Auto);

        // Backend: uses get_enum for validation against operator.json values
        let backend = props.get_enum("backend");

        // Legend position: validated enum
        let legend_position = props.get_enum("legend.position");

        // Legend position inside (coordinate pair)
        let legend_position_inside = props.get_coords("legend.position.inside");

        // Legend justification (coordinate pair)
        let legend_justification = props.get_coords("legend.justification");

        // Chunk size (not in operator.json, internal setting)
        let chunk_size = 10_000usize;

        // PNG compression: validated enum
        let png_compression = props.get_enum("png.compression");

        // Text labels (all optional)
        let plot_title = props.get_optional_string("plot.title");

        // Plot title position: validated enum
        let plot_title_position = props.get_enum("plot.title.position");

        // Plot title justification
        let plot_title_justification = props.get_coords("plot.title.justification");

        // Axis labels
        let x_axis_label = props.get_optional_string("axis.x.label");
        let y_axis_label = props.get_optional_string("axis.y.label");

        // Tick rotation (degrees)
        let x_tick_rotation = props.get_f64("axis.x.tick.rotation");
        let y_tick_rotation = props.get_f64("axis.y.tick.rotation");

        // Heatmap cell aggregation: validated enum
        let heatmap_cell_aggregation =
            HeatmapCellAggregation::parse(&props.get_enum("heatmap.cell.aggregation"));

        // Point shapes per layer
        let layer_shapes = props.get_shape_list("point.shapes");

        // Point size: UI value (1-10) * multiplier
        // Default UI value is 4 (from crosstab model, not operator.json)
        let point_size_multiplier = props.get_f64_in_range("point.size.multiplier", 0.01, 100.0);
        let ui_size = ui_point_size.unwrap_or(4).clamp(1, 10);
        let point_size = (ui_size as f64) * point_size_multiplier;

        Self {
            chunk_size,
            theme,
            plot_width,
            plot_height,
            backend,
            point_size,
            legend_position,
            legend_position_inside,
            legend_justification,
            png_compression,
            plot_title,
            plot_title_position,
            plot_title_justification,
            x_axis_label,
            y_axis_label,
            x_tick_rotation,
            y_tick_rotation,
            heatmap_cell_aggregation,
            layer_shapes,
        }
    }

    /// Resolve plot dimensions to actual pixels
    ///
    /// Priority for auto-sizing:
    /// 1. If crosstab_dimensions provided, use those (from Tercen UI)
    /// 2. Otherwise, derive from grid dimensions (facet count or heatmap size)
    ///
    /// Legend space is added based on legend position:
    /// - left/right: adds width
    /// - top/bottom: adds height
    /// - inside/none: no extra space
    ///
    /// Returns (width, height) in pixels
    pub fn resolve_dimensions_with_crosstab(
        &self,
        crosstab_dims: Option<(i32, i32)>,
        grid_cols: usize,
        grid_rows: usize,
    ) -> (i32, i32) {
        // Calculate legend space based on position
        let (legend_width, legend_height) = match self.legend_position.to_lowercase().as_str() {
            "left" | "right" => (150, 0), // Space for vertical legend
            "top" | "bottom" => (0, 100), // Space for horizontal legend
            _ => (0, 0),                  // Inside or none
        };

        // Resolve base dimensions
        let (base_width, base_height) = if let Some((w, h)) = crosstab_dims {
            // Use crosstab dimensions from Tercen UI (cellSize × nRows)
            match (&self.plot_width, &self.plot_height) {
                (PlotDimension::Auto, PlotDimension::Auto) => (w, h),
                (PlotDimension::Pixels(pw), PlotDimension::Auto) => (*pw, h),
                (PlotDimension::Auto, PlotDimension::Pixels(ph)) => (w, *ph),
                (PlotDimension::Pixels(pw), PlotDimension::Pixels(ph)) => (*pw, *ph),
            }
        } else {
            // Fallback to grid-based calculation
            let width = self.plot_width.resolve(grid_cols);
            let height = self.plot_height.resolve(grid_rows);
            (width, height)
        };

        // Add legend space
        (base_width + legend_width, base_height + legend_height)
    }

    /// Legacy method for backwards compatibility
    /// Prefer resolve_dimensions_with_crosstab when crosstab info is available
    pub fn resolve_dimensions(&self, n_col_facets: usize, n_row_facets: usize) -> (i32, i32) {
        self.resolve_dimensions_with_crosstab(None, n_col_facets, n_row_facets)
    }

    /// Convert legend config to GGRS LegendPosition enum
    ///
    /// Matches ggplot2 semantics exactly. Note that legend.justification is stored
    /// but not yet used by GGRS for positioning along edges - that requires extending
    /// the GGRS rendering logic.
    pub fn to_legend_position(&self) -> ggrs_core::theme::LegendPosition {
        use crate::tercen::operator_properties::registry;
        use ggrs_core::theme::LegendPosition;

        match self.legend_position.to_lowercase().as_str() {
            "left" => LegendPosition::Left,
            "right" => LegendPosition::Right,
            "top" => LegendPosition::Top,
            "bottom" => LegendPosition::Bottom,
            "inside" => {
                // Use legend.position.inside for coordinates
                // Default comes from operator.json (empty = use fallback position)
                let (x, y) = self.legend_position_inside.unwrap_or_else(|| {
                    // Parse default from operator.json if available
                    registry()
                        .get_default("legend.position.inside")
                        .and_then(|s| {
                            if s.is_empty() {
                                None
                            } else {
                                let parts: Vec<&str> = s.split(',').collect();
                                if parts.len() == 2 {
                                    let x = parts[0].trim().parse().ok()?;
                                    let y = parts[1].trim().parse().ok()?;
                                    Some((x, y))
                                } else {
                                    None
                                }
                            }
                        })
                        .unwrap_or((0.95, 0.95))
                });
                LegendPosition::Inside(x, y)
            }
            "none" => LegendPosition::None,
            _ => LegendPosition::Right, // Should not happen due to enum validation
        }
    }

    /// Convert theme config to GGRS Theme
    ///
    /// Matches ggplot2 theme functions exactly:
    /// - gray: Default ggplot2 theme with gray panel background and white grid
    /// - bw: Black and white theme with white background and gray grid
    /// - linedraw: Black lines on white backgrounds
    /// - light: Light grey lines and axes on white
    /// - dark: Dark background (inverse of light)
    /// - minimal: No background, border, or ticks
    /// - classic: Axis lines, no grid (traditional look)
    /// - void: Completely empty (just data)
    pub fn to_theme(&self) -> ggrs_core::theme::Theme {
        use ggrs_core::theme::Theme;

        match self.theme.to_lowercase().as_str() {
            "bw" => Theme::bw(),
            "linedraw" => Theme::linedraw(),
            "light" => Theme::light(),
            "dark" => Theme::dark(),
            "minimal" => Theme::minimal(),
            "classic" => Theme::classic(),
            "void" => Theme::void(),
            _ => Theme::gray(), // "gray" or any other value
        }
    }
}
