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

        Self {
            chunk_size: 10_000, // Internal constant
            plot_width,
            plot_height,
            backend,
            point_size: 4, // Hardcoded for now, TODO: get from crosstab aesthetics
        }
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
}
