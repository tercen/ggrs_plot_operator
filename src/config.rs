//! Operator configuration from Tercen properties
//!
//! Configuration is loaded from operator properties (defined in operator.json)
//! rather than config files. When testing locally with no properties set,
//! explicit defaults are used.

use crate::tercen::client::proto::OperatorSettings;
use crate::tercen::properties::{PlotDimension, PropertyReader};

#[derive(Debug, Clone)]
pub struct OperatorConfig {
    /// Number of rows per chunk (internal constant, not configurable)
    pub chunk_size: usize,

    /// Plot width (pixels or Auto)
    pub plot_width: PlotDimension,

    /// Plot height (pixels or Auto)
    pub plot_height: PlotDimension,

    /// Render backend: "cpu" or "gpu"
    pub backend: String,

    /// Point size (hardcoded, TODO: get from crosstab aesthetics)
    pub point_size: i32,

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
}

impl OperatorConfig {
    /// Create config from operator properties with explicit defaults
    ///
    /// When testing locally (no properties set), uses these defaults:
    /// - plot_width: "auto" (derive from column facet count)
    /// - plot_height: "auto" (derive from row facet count)
    /// - backend: "cpu"
    ///
    /// Properties come from operator.json definitions and are set via Tercen UI.
    /// Note: point_size is hardcoded (4) - should come from crosstab aesthetics in future.
    pub fn from_properties(operator_settings: Option<&OperatorSettings>) -> Self {
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

        Self {
            chunk_size: 10_000, // Internal constant
            plot_width,
            plot_height,
            backend,
            point_size: 4, // Hardcoded for now, TODO: get from crosstab aesthetics
            legend_position,
            legend_position_inside,
            legend_justification,
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
