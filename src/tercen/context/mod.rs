//! TercenContext trait and implementations
//!
//! This module provides a unified interface for accessing Tercen task/query data
//! regardless of whether we're in production mode (with task_id) or dev mode
//! (with workflow_id + step_id).
//!
//! This mirrors Python's OperatorContext / OperatorContextDev pattern.

use crate::tercen::client::proto::{CubeQuery, OperatorSettings};
use crate::tercen::colors::{ChartKind, ColorInfo};
use crate::tercen::TercenClient;
use std::sync::Arc;

mod dev_context;
mod production_context;

pub use dev_context::DevContext;
pub use production_context::ProductionContext;

/// Trait for accessing Tercen context data
///
/// Implementations:
/// - `ProductionContext`: Initialized from task_id (production mode)
/// - `DevContext`: Initialized from workflow_id + step_id (dev/test mode)
pub trait TercenContext: Send + Sync {
    /// Get the CubeQuery containing table hashes
    fn cube_query(&self) -> &CubeQuery;

    /// Get the schema IDs (table IDs for Y-axis, colors, etc.)
    fn schema_ids(&self) -> &[String];

    /// Get the workflow ID
    fn workflow_id(&self) -> &str;

    /// Get the step ID
    fn step_id(&self) -> &str;

    /// Get the project ID
    fn project_id(&self) -> &str;

    /// Get the namespace
    fn namespace(&self) -> &str;

    /// Get the operator settings (if available)
    fn operator_settings(&self) -> Option<&OperatorSettings>;

    /// Get the color information extracted from the workflow
    fn color_infos(&self) -> &[ColorInfo];

    /// Get the page factor names
    fn page_factors(&self) -> &[String];

    /// Get the Y-axis table ID (if available)
    fn y_axis_table_id(&self) -> Option<&str>;

    /// Get the X-axis table ID (if available)
    fn x_axis_table_id(&self) -> Option<&str>;

    /// Get the point size from crosstab model (UI scale 1-10, None = use default)
    fn point_size(&self) -> Option<i32>;

    /// Get the chart kind (Point, Heatmap, Line, Bar)
    fn chart_kind(&self) -> ChartKind;

    /// Get the Tercen client
    fn client(&self) -> &Arc<TercenClient>;

    // Convenience methods with default implementations

    /// Get the main table hash (qt_hash)
    fn qt_hash(&self) -> &str {
        &self.cube_query().qt_hash
    }

    /// Get the column facet table hash
    fn column_hash(&self) -> &str {
        &self.cube_query().column_hash
    }

    /// Get the row facet table hash
    fn row_hash(&self) -> &str {
        &self.cube_query().row_hash
    }
}
