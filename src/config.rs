//! Operator configuration from Tercen properties
//!
//! Configuration is loaded from operator properties (defined in operator.json)
//! rather than config files. When testing locally with no properties set,
//! explicit defaults are used.

use crate::tercen::client::proto::OperatorSettings;
use crate::tercen::properties::{PlotDimension, PropertyReader};

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
    /// Parse from string, returns default (Last) for invalid values
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "last" => Self::Last,
            "first" => Self::First,
            "mean" => Self::Mean,
            "median" => Self::Median,
            other => {
                eprintln!(
                    "⚠ Invalid heatmap.cell.aggregation '{}', using 'last'",
                    other
                );
                Self::Last
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct OperatorConfig {
    /// Number of rows per chunk (configurable via chunk_size property, default: 10000)
    pub chunk_size: usize,

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
}

impl OperatorConfig {
    /// Create config from operator properties with explicit defaults
    ///
    /// When testing locally (no properties set), uses these defaults:
    /// - plot_width: "auto" (derive from column facet count)
    /// - plot_height: "auto" (derive from row facet count)
    /// - backend: "cpu"
    /// - point_size: 4 (UI scale, will be converted to render size)
    ///
    /// Properties come from operator.json definitions and are set via Tercen UI.
    ///
    /// # Arguments
    /// * `operator_settings` - Operator settings from Tercen
    /// * `ui_point_size` - Point size from crosstab model (UI scale 1-10), None = use default (4)
    pub fn from_properties(
        operator_settings: Option<&OperatorSettings>,
        ui_point_size: Option<i32>,
    ) -> Self {
        let props = PropertyReader::from_operator_settings(operator_settings);

        // Parse plot dimensions with "auto" support
        // Empty string or "auto" → Auto (derive from facet count)
        // "1500" → Pixels(1500) if valid range [100-10000]
        let plot_width =
            PlotDimension::from_str(&props.get_string("plot.width", ""), PlotDimension::Auto);

        let plot_height =
            PlotDimension::from_str(&props.get_string("plot.height", ""), PlotDimension::Auto);

        // Parse backend with validation
        let backend = props.get_string("backend", "cpu");
        let backend = match backend.to_lowercase().as_str() {
            "gpu" | "webgpu" => "gpu",
            "cpu" | "cairo" => "cpu",
            other => {
                eprintln!("⚠ Invalid backend '{}', using 'cpu'", other);
                "cpu"
            }
        }
        .to_string();

        // Parse legend position (matches ggplot2 theme(legend.position = ...))
        // Valid values: "right" (default), "left", "top", "bottom", "inside", "none"
        let legend_position = props.get_string("legend.position", "right");
        let legend_position = match legend_position.to_lowercase().as_str() {
            "left" | "right" | "top" | "bottom" | "inside" | "none" => legend_position,
            other => {
                eprintln!("⚠ Invalid legend.position '{}', using 'right'", other);
                "right".to_string()
            }
        };

        // Parse legend.position.inside (only used when legend.position = "inside")
        // Format: "x,y" where x,y ∈ [0,1], e.g., "0.9,0.1"
        let legend_position_inside =
            Self::parse_coords(&props.get_string("legend.position.inside", ""));

        // Parse legend.justification
        // Format: "x,y" where x,y ∈ [0,1]
        let legend_justification =
            Self::parse_coords(&props.get_string("legend.justification", ""));

        // Parse chunk_size from properties (default: 10000)
        let chunk_size = props.get_i32("chunk_size", 10_000) as usize;

        // Parse PNG compression level (default: "default")
        let png_compression = props.get_string("png.compression", "default");
        let png_compression = match png_compression.to_lowercase().as_str() {
            "fast" | "best" | "default" => png_compression,
            other => {
                eprintln!("⚠ Invalid png.compression '{}', using 'default'", other);
                "default".to_string()
            }
        };

        // Parse text labels (all optional)
        let plot_title = {
            let title = props.get_string("plot.title", "");
            if title.is_empty() {
                None
            } else {
                Some(title)
            }
        };

        // Parse plot title position (matches ggplot2 theme(plot.title.position = ...))
        // Valid values: "top" (default), "bottom", "left", "right"
        let plot_title_position = props.get_string("plot.title.position", "top");
        let plot_title_position = match plot_title_position.to_lowercase().as_str() {
            "top" | "bottom" | "left" | "right" => plot_title_position,
            other => {
                eprintln!("⚠ Invalid plot.title.position '{}', using 'top'", other);
                "top".to_string()
            }
        };

        // Parse plot.title.justification
        // Format: "x,y" where x,y ∈ [0,1]
        let plot_title_justification =
            Self::parse_coords(&props.get_string("plot.title.justification", "0.5,0.5"));

        let x_axis_label = {
            let label = props.get_string("axis.x.label", "");
            if label.is_empty() {
                None
            } else {
                Some(label)
            }
        };

        let y_axis_label = {
            let label = props.get_string("axis.y.label", "");
            if label.is_empty() {
                None
            } else {
                Some(label)
            }
        };

        // Parse tick label rotation (in degrees)
        let x_tick_rotation = Self::parse_rotation(&props.get_string("axis.x.tick.rotation", "0"));
        let y_tick_rotation = Self::parse_rotation(&props.get_string("axis.y.tick.rotation", "0"));

        // Parse heatmap cell aggregation method (default: "last" to match Tercen)
        let heatmap_cell_aggregation =
            HeatmapCellAggregation::parse(&props.get_string("heatmap.cell.aggregation", "last"));

        // Parse point size multiplier (default: 1.0, must be > 0)
        let point_size_multiplier = {
            let mult_str = props.get_string("point.size.multiplier", "1");
            match mult_str.parse::<f64>() {
                Ok(m) if m > 0.0 => m,
                Ok(m) => {
                    eprintln!(
                        "⚠ Invalid point.size.multiplier '{}' (must be > 0), using 1.0",
                        m
                    );
                    1.0
                }
                Err(_) => {
                    eprintln!("⚠ Invalid point.size.multiplier '{}', using 1.0", mult_str);
                    1.0
                }
            }
        };

        // Convert UI point size (1-10) to render size with multiplier
        // UI scale: 1 = minimal (1px), 10 = 2.5x default (10px)
        // Default UI value is 4, which maps to 4px
        // Formula: render_size = ui_value * multiplier
        let ui_size = ui_point_size.unwrap_or(4).clamp(1, 10);
        let point_size = (ui_size as f64) * point_size_multiplier;

        Self {
            chunk_size,
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
        }
    }

    /// Parse rotation angle string into degrees (f64)
    ///
    /// Accepts any numeric value, returns 0.0 for invalid input
    /// Common values: 0 (horizontal), 45 (diagonal), 90 (vertical)
    fn parse_rotation(s: &str) -> f64 {
        if s.is_empty() {
            return 0.0;
        }

        match s.trim().parse::<f64>() {
            Ok(deg) => deg,
            Err(_) => {
                eprintln!("⚠ Invalid rotation '{}', using 0", s);
                0.0
            }
        }
    }

    /// Parse coordinate string "x,y" into (f64, f64)
    ///
    /// Format: "x,y" where x,y ∈ [0,1]
    /// Examples: "0.9,0.1", "0.5,0.5", "1,1"
    /// Returns None if empty string or invalid format
    fn parse_coords(s: &str) -> Option<(f64, f64)> {
        if s.is_empty() {
            return None;
        }

        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 2 {
            eprintln!("⚠ Invalid coordinate format '{}', expected 'x,y'", s);
            return None;
        }

        let x = parts[0].trim().parse::<f64>().ok()?;
        let y = parts[1].trim().parse::<f64>().ok()?;

        // Validate range [0, 1]
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            eprintln!("⚠ Coordinates '{}' out of range [0,1]", s);
            return None;
        }

        Some((x, y))
    }

    /// Resolve plot dimensions to actual pixels
    ///
    /// Called after knowing facet counts. For "auto" dimensions,
    /// derives size from facet count using:
    /// - base_size (800px) + (n_facets - 1) * 400px
    /// - Capped at 4000px
    ///
    /// Returns (width, height) in pixels
    pub fn resolve_dimensions(&self, n_col_facets: usize, n_row_facets: usize) -> (i32, i32) {
        let width = self.plot_width.resolve(n_col_facets);
        let height = self.plot_height.resolve(n_row_facets);
        (width, height)
    }

    /// Convert legend config to GGRS LegendPosition enum
    ///
    /// Matches ggplot2 semantics exactly. Note that legend.justification is stored
    /// but not yet used by GGRS for positioning along edges - that requires extending
    /// the GGRS rendering logic.
    pub fn to_legend_position(&self) -> ggrs_core::theme::LegendPosition {
        use ggrs_core::theme::LegendPosition;

        match self.legend_position.to_lowercase().as_str() {
            "left" => LegendPosition::Left,
            "right" => LegendPosition::Right,
            "top" => LegendPosition::Top,
            "bottom" => LegendPosition::Bottom,
            "inside" => {
                // Use legend.position.inside for coordinates
                let (x, y) = self.legend_position_inside.unwrap_or((0.95, 0.95));
                LegendPosition::Inside(x, y)
            }
            "none" => LegendPosition::None,
            _ => LegendPosition::Right, // Fallback (should not happen due to validation)
        }
    }
}
