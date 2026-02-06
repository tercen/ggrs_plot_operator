//! Tercen Stream Generator - Bridges Tercen data with GGRS plotting
//!
//! This module implements the GGRS `StreamGenerator` trait for Tercen,
//! enabling lazy loading of data directly from Tercen's gRPC API.

use crate::config::HeatmapCellAggregation;
use crate::tercen::{
    tson_to_dataframe, ChartKind, FacetInfo, SchemaCache, TableStreamer, TercenClient,
};
use ggrs_core::{
    aes::Aes,
    data::DataFrame,
    legend::{ColorStop as LegendColorStop, LegendScale, LegendSection},
    stream::{
        AxisData, CategoricalAxisData, FacetSpec, NumericAxisData, Range, StreamGenerator,
        Transform,
    },
};
use polars::prelude::IntoColumn;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Default number of categorical color levels in Tercen's built-in palette.
/// When no actual category names are available, generic labels "Level 0" through "Level 7" are used.
const DEFAULT_PALETTE_LEVELS: usize = 8;

/// Configuration for creating a TercenStreamGenerator
///
/// Groups all the parameters needed to initialize a stream generator,
/// making the API cleaner and more maintainable.
#[derive(Clone)]
pub struct TercenStreamConfig {
    /// Main data table ID (qt_hash)
    pub main_table_id: String,
    /// Column facet table ID (cschema)
    pub col_facet_table_id: String,
    /// Row facet table ID (rschema)
    pub row_facet_table_id: String,
    /// Optional Y-axis range table ID
    pub y_axis_table_id: Option<String>,
    /// Optional X-axis range table ID
    pub x_axis_table_id: Option<String>,
    /// Chunk size for streaming data
    pub chunk_size: usize,
    /// Color factor configurations (legacy - used when all layers share same colors)
    pub color_infos: Vec<crate::tercen::ColorInfo>,
    /// Per-layer color configuration (for mixed-layer scenarios)
    pub per_layer_colors: Option<crate::tercen::PerLayerColorConfig>,
    /// Page factor names for pagination
    pub page_factors: Vec<String>,
    /// Optional schema cache for multi-page plots
    pub schema_cache: Option<SchemaCache>,
    /// How to aggregate multiple data points in the same heatmap cell
    pub heatmap_cell_aggregation: HeatmapCellAggregation,
    /// Y-axis transform type (e.g., "log", "ln", "log10")
    /// When set, indicates data is pre-transformed and GGRS should invert it
    pub y_transform: Option<String>,
    /// X-axis transform type
    pub x_transform: Option<String>,
    /// Number of layers (axis_queries) - used for layer-based coloring
    pub n_layers: usize,
    /// Palette name for layer-based coloring (from crosstab)
    pub layer_palette_name: Option<String>,
    /// Y-axis factor names per layer (from axis_queries[i].yAxis.name)
    /// Used for legend entries when layers don't have explicit color factors
    pub layer_y_factor_names: Vec<String>,
    /// Chart kind - determines data columns needed (e.g., bar charts need .y0s baseline)
    pub chart_kind: ChartKind,
}

impl TercenStreamConfig {
    /// Create a new configuration with required fields
    pub fn new(
        main_table_id: String,
        col_facet_table_id: String,
        row_facet_table_id: String,
        chunk_size: usize,
    ) -> Self {
        Self {
            main_table_id,
            col_facet_table_id,
            row_facet_table_id,
            y_axis_table_id: None,
            x_axis_table_id: None,
            chunk_size,
            color_infos: Vec::new(),
            per_layer_colors: None,
            page_factors: Vec::new(),
            schema_cache: None,
            heatmap_cell_aggregation: HeatmapCellAggregation::Last,
            y_transform: None,
            x_transform: None,
            n_layers: 1,
            layer_palette_name: None,
            layer_y_factor_names: Vec::new(),
            chart_kind: ChartKind::Point,
        }
    }

    /// Set chart kind (determines which data columns to fetch)
    pub fn chart_kind(mut self, chart_kind: ChartKind) -> Self {
        self.chart_kind = chart_kind;
        self
    }

    /// Set Y-axis table ID
    pub fn y_axis_table(mut self, table_id: Option<String>) -> Self {
        self.y_axis_table_id = table_id;
        self
    }

    /// Set X-axis table ID
    pub fn x_axis_table(mut self, table_id: Option<String>) -> Self {
        self.x_axis_table_id = table_id;
        self
    }

    /// Set color information (legacy - use per_layer_colors for mixed scenarios)
    pub fn colors(mut self, color_infos: Vec<crate::tercen::ColorInfo>) -> Self {
        self.color_infos = color_infos;
        self
    }

    /// Set per-layer color configuration (for mixed-layer scenarios)
    pub fn per_layer_colors(mut self, config: Option<crate::tercen::PerLayerColorConfig>) -> Self {
        self.per_layer_colors = config;
        self
    }

    /// Set page factors
    pub fn page_factors(mut self, factors: Vec<String>) -> Self {
        self.page_factors = factors;
        self
    }

    /// Set schema cache
    pub fn schema_cache(mut self, cache: Option<SchemaCache>) -> Self {
        self.schema_cache = cache;
        self
    }

    /// Set heatmap cell aggregation method
    pub fn heatmap_cell_aggregation(mut self, method: HeatmapCellAggregation) -> Self {
        self.heatmap_cell_aggregation = method;
        self
    }

    /// Set Y-axis transform type
    ///
    /// When set, indicates that Y-axis data is pre-transformed (e.g., already in log space).
    /// GGRS will invert the transform to get original values before applying the scale.
    pub fn y_transform(mut self, transform: Option<String>) -> Self {
        self.y_transform = transform;
        self
    }

    /// Set X-axis transform type
    pub fn x_transform(mut self, transform: Option<String>) -> Self {
        self.x_transform = transform;
        self
    }

    /// Set number of layers (for layer-based coloring)
    pub fn n_layers(mut self, n: usize) -> Self {
        self.n_layers = n;
        self
    }

    /// Set palette name for layer-based coloring
    pub fn layer_palette_name(mut self, name: Option<String>) -> Self {
        self.layer_palette_name = name;
        self
    }

    /// Set Y-axis factor names per layer (for legend entries)
    pub fn layer_y_factor_names(mut self, names: Vec<String>) -> Self {
        self.layer_y_factor_names = names;
        self
    }
}

/// Extract row count from schema
fn extract_row_count_from_schema(
    schema: &crate::tercen::client::proto::ESchema,
) -> Result<i64, Box<dyn std::error::Error>> {
    use crate::tercen::client::proto::e_schema;

    // All schema types (TableSchema, ComputedTableSchema, CubeQueryTableSchema) have nRows field
    match &schema.object {
        Some(e_schema::Object::Tableschema(ts)) => Ok(ts.n_rows as i64),
        Some(e_schema::Object::Computedtableschema(cts)) => Ok(cts.n_rows as i64),
        Some(e_schema::Object::Cubequerytableschema(cqts)) => Ok(cqts.n_rows as i64),
        Some(e_schema::Object::Schema(_)) => Err("Schema variant not supported".into()),
        None => Err("Schema object is None".into()),
    }
}

/// Helper function to extract column names from a schema
pub fn extract_column_names_from_schema(
    schema: &crate::tercen::client::proto::ESchema,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    use crate::tercen::client::proto::e_schema;

    if let Some(e_schema::Object::Cubequerytableschema(cqts)) = &schema.object {
        let mut column_names = Vec::new();
        for col in &cqts.columns {
            if let Some(crate::tercen::client::proto::e_column_schema::Object::Columnschema(cs)) =
                &col.object
            {
                column_names.push(cs.name.clone());
            }
        }
        Ok(column_names)
    } else {
        Err("Schema is not a CubeQueryTableSchema".into())
    }
}

/// Tercen implementation of GGRS StreamGenerator
///
/// Streams raw data from Tercen tables. Does NOT transform coordinates.
/// GGRS handles dequantization using axis ranges.
///
/// Columns streamed:
/// - `.ci`, `.ri` - facet indices for panel routing
/// - `.xs`, `.ys` - quantized coordinates for positioning
/// - `.xLevels`, `.nXLevels` - heatmap grid indices (used by tile renderer)
/// - `.color` - pre-computed color hex strings
pub struct TercenStreamGenerator {
    /// Tercen client for gRPC communication
    client: Arc<TercenClient>,

    /// Main data table ID
    main_table_id: String,

    /// Facet information (column and row facets)
    facet_info: FacetInfo,

    /// Pre-computed axis ranges for each facet cell
    axis_ranges: HashMap<(usize, usize), (AxisData, AxisData)>,

    /// Total row count across ALL facets
    total_rows: usize,

    /// GGRS aesthetic mappings - uses quantized coordinates
    aes: Aes,

    /// GGRS facet specification
    facet_spec: FacetSpec,

    /// Chunk size for streaming
    chunk_size: usize,

    /// Color information (factors and palettes) - legacy field
    color_infos: Vec<crate::tercen::ColorInfo>,

    /// Per-layer color configuration (for mixed-layer scenarios)
    per_layer_colors: Option<crate::tercen::PerLayerColorConfig>,

    /// Cached legend scale (loaded during initialization)
    cached_legend_scale: LegendScale,

    /// Page factor names (kept for metadata/debugging)
    /// Not used for filtering - GGRS handles everything via original_index
    #[allow(dead_code)]
    page_factors: Vec<String>,

    /// Heatmap mode: when set, overrides facet counts to 1x1 and uses grid dimensions for axes
    /// Tuple is (n_columns, n_rows) representing the heatmap grid dimensions
    heatmap_mode: Option<(usize, usize)>,

    /// Optional schema cache for multi-page plots
    /// When provided, schemas are cached and reused across pages
    schema_cache: Option<SchemaCache>,

    /// Cached aggregated data for heatmaps
    /// When in heatmap mode, we aggregate all data by (ci, ri) and cache it here.
    /// This is necessary because GGRS streams in chunks, but aggregation requires all data.
    heatmap_cached_data: RwLock<Option<DataFrame>>,

    /// How to aggregate multiple data points in the same heatmap cell
    heatmap_cell_aggregation: HeatmapCellAggregation,

    /// Y-axis transform type (e.g., "log", "ln", "log10")
    /// When set, indicates Y data is pre-transformed and GGRS should invert it
    /// Note: Transform is applied to axis_ranges, this field kept for debugging
    #[allow(dead_code)]
    y_transform: Option<Transform>,

    /// X-axis transform type
    #[allow(dead_code)]
    x_transform: Option<Transform>,

    /// Number of layers (axis_queries) - used for layer-based coloring
    /// When > 1 and color_infos is empty, we color points by their .axisIndex
    n_layers: usize,

    /// Palette name for layer-based coloring (from crosstab)
    layer_palette_name: Option<String>,

    /// Y-axis factor names per layer (from axis_queries[i].yAxis.name)
    /// Used for legend entries when layers don't have explicit color factors
    /// Note: Used at initialization in load_legend_scale(), not read later
    #[allow(dead_code)]
    layer_y_factor_names: Vec<String>,

    /// Chart kind - determines data columns needed (e.g., bar charts need .y0s baseline)
    chart_kind: ChartKind,
}

impl TercenStreamGenerator {
    /// Create a new stream generator with configuration struct
    ///
    /// This loads facet metadata and axis ranges from pre-computed tables.
    /// Note: page_filter is used to load only the facets for this page (e.g., female or male),
    /// but data is NOT filtered - GGRS handles data matching via original_index
    ///
    /// # Arguments
    /// * `client` - Tercen gRPC client
    /// * `config` - Configuration containing table IDs and options
    /// * `page_filter` - Optional filter for pagination (e.g., {"sex": "female"})
    #[allow(dead_code)]
    pub async fn new(
        client: Arc<TercenClient>,
        config: TercenStreamConfig,
        page_filter: Option<&HashMap<String, String>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Extract config fields for use throughout the function
        let TercenStreamConfig {
            main_table_id,
            col_facet_table_id,
            row_facet_table_id,
            y_axis_table_id,
            x_axis_table_id,
            chunk_size,
            color_infos,
            per_layer_colors,
            page_factors,
            schema_cache,
            heatmap_cell_aggregation,
            y_transform,
            x_transform,
            n_layers,
            layer_palette_name,
            layer_y_factor_names,
            chart_kind,
        } = config;

        // Convert transform strings to Transform structs
        let y_transform = y_transform.and_then(|t| Transform::parse(&t));
        let x_transform = x_transform.and_then(|t| Transform::parse(&t));

        if y_transform.is_some() {
            println!("  Y-axis transform: {:?}", y_transform);
        }
        if x_transform.is_some() {
            println!("  X-axis transform: {:?}", x_transform);
        }

        // Load facets with optional filtering for pagination
        // Each page should only show its own facet panels
        let facet_info = if let Some(filter) = page_filter {
            eprintln!("DEBUG: Loading facets with page filter: {:?}", filter);
            FacetInfo::load_with_filter(&client, &col_facet_table_id, &row_facet_table_id, filter)
                .await?
        } else {
            eprintln!("DEBUG: Loading all facets (no pagination)");
            FacetInfo::load(&client, &col_facet_table_id, &row_facet_table_id).await?
        };

        println!(
            "Loaded facets: {} columns × {} rows = {} cells",
            facet_info.n_col_facets(),
            facet_info.n_row_facets(),
            facet_info.total_facets()
        );

        // NO FILTERING! Operator is dumb - GGRS handles everything via original_index.
        // We just keep the facet_info which has both index and original_index for each facet.

        // Load axis ranges from pre-computed Y-axis table (required)
        let y_table_id = y_axis_table_id.ok_or(
            "Y-axis table is required but was not found. \
             This usually means schema_ids is empty in the task. \
             Ensure the crosstab has a Y-axis factor defined.",
        )?;

        println!("Loading axis ranges from Y-axis table: {}", y_table_id);
        let (mut axis_ranges, total_rows) = Self::load_axis_ranges_from_table(
            &client,
            &y_table_id,
            &main_table_id,
            &facet_info,
            &schema_cache,
        )
        .await?;

        eprintln!(
            "DEBUG: axis_ranges has {} entries (before X range computation), total_rows: {}",
            axis_ranges.len(),
            total_rows
        );

        // Check if X ranges need to be loaded (Y-axis table may not have .minX/.maxX columns)
        let needs_x_range = axis_ranges.values().any(|(x_axis, _)| {
            if let AxisData::Numeric(ref num) = x_axis {
                num.min_value.is_nan() || num.max_value.is_nan()
            } else {
                false
            }
        });

        if needs_x_range {
            // First, try to load X ranges from X-axis table (if available)
            if let Some(ref x_table_id) = x_axis_table_id {
                println!("Loading X-axis ranges from X-axis table: {}", x_table_id);
                Self::load_x_ranges_from_table(
                    &client,
                    x_table_id,
                    &facet_info,
                    &mut axis_ranges,
                    &schema_cache,
                )
                .await?;
            } else {
                // No X-axis table means X is sequential (1..n_rows)
                // No need to scan data - just use the row count
                println!(
                    "No X-axis table - using sequential X range: 1 to {}",
                    total_rows
                );
                Self::set_sequential_x_ranges(total_rows as f64, &mut axis_ranges);
            }
        }

        // NOTE: axis_ranges now keyed by original_index (not filtered index)
        // load_axis_ranges_from_table() already maps table's .ri (0-11) → original_index (12-23)
        // This ensures data[.ri=12] can look up y_ranges[12] correctly
        eprintln!("DEBUG: axis_ranges keyed by original_index for data matching");

        // Apply transform info to axis ranges
        // This tells GGRS that the data is pre-transformed and needs inversion
        if y_transform.is_some() || x_transform.is_some() {
            for (x_axis, y_axis) in axis_ranges.values_mut() {
                if let AxisData::Numeric(ref mut num) = y_axis {
                    num.transform = y_transform.clone();
                }
                if let AxisData::Numeric(ref mut num) = x_axis {
                    num.transform = x_transform.clone();
                }
            }
            eprintln!(
                "DEBUG: Applied transforms to axis_ranges - Y: {:?}, X: {:?}",
                y_transform, x_transform
            );
        }

        eprintln!(
            "DEBUG: TercenStreamGenerator initialized with total_rows = {}",
            total_rows
        );

        // Load legend scale data
        // Load legend scale from color info (n_levels from schema)
        // For mixed scenarios, combine sections from layers with colors and layers without
        println!("Loading legend scale data...");
        let cached_legend_scale = Self::load_legend_scale(
            &color_infos,
            per_layer_colors.as_ref(),
            &layer_y_factor_names,
        )?;
        eprintln!("DEBUG: Cached legend scale: {:?}", cached_legend_scale);

        // Create default aesthetics
        // Dequantization happens in GGRS render.rs using axis ranges
        // After dequantization, columns are .x and .y (actual data values)
        // Add color aesthetic if colors are defined (either legacy or per-layer)
        let mut aes = Aes::new().x(".x").y(".y");

        // Determine if we have any colors to display
        // With the new LayerColorConfig, every layer has a config (explicit or constant)
        let has_colors = if let Some(ref plc) = per_layer_colors {
            plc.has_explicit_colors() || plc.has_constant_colors()
        } else {
            !color_infos.is_empty()
        };

        eprintln!("DEBUG: color_infos.len() = {}", color_infos.len());
        eprintln!(
            "DEBUG: per_layer_colors = {:?}",
            per_layer_colors.as_ref().map(|p| format!(
                "n_layers={}, has_explicit={}, is_mixed={}",
                p.n_layers,
                p.has_explicit_colors(),
                p.is_mixed()
            ))
        );

        if has_colors {
            eprintln!("DEBUG: Adding .color aesthetic to Aes");
            if !color_infos.is_empty() {
                eprintln!("DEBUG: Color factor: '{}'", color_infos[0].factor_name);
                match &color_infos[0].mapping {
                    crate::tercen::ColorMapping::Continuous(palette) => {
                        eprintln!(
                            "DEBUG: Continuous palette with {} color stops",
                            palette.stops.len()
                        );
                        for (i, stop) in palette.stops.iter().enumerate() {
                            eprintln!(
                                "  Stop {}: value={:.2}, color=RGB({}, {}, {})",
                                i, stop.value, stop.color[0], stop.color[1], stop.color[2]
                            );
                        }
                    }
                    crate::tercen::ColorMapping::Categorical(color_map) => {
                        eprintln!(
                            "DEBUG: Categorical palette with {} categories",
                            color_map.mappings.len()
                        );
                    }
                }
            }
            aes = aes.color(".color");
        } else {
            eprintln!("DEBUG: No colors configured, NOT adding .color aesthetic");
        }

        // Create facet spec based on facet metadata
        // Use actual column names from facet tables for labels
        // Data filtering still uses .ri/.ci indices (handled in query_data_chunk)
        let facet_spec = if !facet_info.row_facets.is_empty() && !facet_info.col_facets.is_empty() {
            // Grid faceting: rows × columns
            use ggrs_core::stream::FacetScales;
            let row_vars = facet_info
                .row_facets
                .column_names
                .iter()
                .filter(|n| !n.is_empty())
                .cloned()
                .collect::<Vec<_>>();
            let col_vars = facet_info
                .col_facets
                .column_names
                .iter()
                .filter(|n| !n.is_empty())
                .cloned()
                .collect::<Vec<_>>();
            let row_var = row_vars.first().unwrap_or(&".ri".to_string()).clone();
            let col_var = col_vars.first().unwrap_or(&".ci".to_string()).clone();
            FacetSpec::grid(row_var, col_var).scales(FacetScales::FreeY)
        } else if !facet_info.row_facets.is_empty() {
            // Row faceting only (each row has its own Y range)
            use ggrs_core::stream::FacetScales;
            let row_vars = facet_info
                .row_facets
                .column_names
                .iter()
                .filter(|n| !n.is_empty())
                .cloned()
                .collect::<Vec<_>>();
            let row_var = row_vars.first().unwrap_or(&".ri".to_string()).clone();
            FacetSpec::row(row_var).scales(FacetScales::FreeY)
        } else if !facet_info.col_facets.is_empty() {
            // Column faceting only
            let col_vars = facet_info
                .col_facets
                .column_names
                .iter()
                .filter(|n| !n.is_empty())
                .cloned()
                .collect::<Vec<_>>();
            let col_var = col_vars.first().unwrap_or(&".ci".to_string()).clone();
            FacetSpec::col(col_var)
        } else {
            // No faceting
            FacetSpec::none()
        };

        Ok(Self {
            client,
            main_table_id,
            facet_info,
            axis_ranges,
            total_rows,
            aes,
            facet_spec,
            chunk_size,
            color_infos,
            per_layer_colors,
            cached_legend_scale,
            page_factors,
            heatmap_mode: None,
            schema_cache,
            heatmap_cached_data: RwLock::new(None),
            heatmap_cell_aggregation,
            y_transform,
            x_transform,
            n_layers,
            layer_palette_name,
            layer_y_factor_names,
            chart_kind,
        })
    }

    /// Create a TableStreamer, using the schema cache if available
    fn create_streamer<'a>(
        client: &'a TercenClient,
        cache: &Option<SchemaCache>,
    ) -> TableStreamer<'a> {
        match cache {
            Some(c) => TableStreamer::with_cache(client, c.clone()),
            None => TableStreamer::new(client),
        }
    }

    /// Create a stream generator with pre-computed axis ranges
    ///
    /// This is used when axis ranges are provided externally.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_ranges(
        client: Arc<TercenClient>,
        main_table_id: String,
        facet_info: FacetInfo,
        axis_ranges: HashMap<(usize, usize), (AxisData, AxisData)>,
        total_rows: usize,
        chunk_size: usize,
        color_infos: Vec<crate::tercen::ColorInfo>,
        page_factors: Vec<String>,
    ) -> Self {
        // Aesthetics use dequantized coordinates: .x and .y (actual data values)
        // Dequantization happens in stream_facet_data() before data reaches renderers
        // Add color aesthetic if colors are defined
        let mut aes = Aes::new().x(".x").y(".y");
        if !color_infos.is_empty() {
            aes = aes.color(".color");
        }

        // Create facet spec based on facet metadata
        // Use .ri/.ci as faceting variables since our data uses indices
        // GGRS will use these to determine panel layout
        let facet_spec = if !facet_info.row_facets.is_empty() && !facet_info.col_facets.is_empty() {
            // Grid faceting: rows × columns
            use ggrs_core::stream::FacetScales;
            FacetSpec::grid(".ri", ".ci").scales(FacetScales::FreeY)
        } else if !facet_info.row_facets.is_empty() {
            // Row faceting only (each row has its own Y range)
            use ggrs_core::stream::FacetScales;
            FacetSpec::row(".ri").scales(FacetScales::FreeY)
        } else if !facet_info.col_facets.is_empty() {
            // Column faceting only
            FacetSpec::col(".ci")
        } else {
            // No faceting
            FacetSpec::none()
        };

        // NO FILTERING! Operator is dumb - GGRS handles everything via original_index.

        Self {
            client,
            main_table_id,
            facet_info,
            axis_ranges,
            total_rows,
            aes,
            facet_spec,
            chunk_size,
            color_infos,
            per_layer_colors: None, // Sync constructor doesn't support per-layer colors
            cached_legend_scale: LegendScale::None, // TODO: Load async if needed
            page_factors,
            heatmap_mode: None,
            schema_cache: None, // sync method - no caching
            heatmap_cached_data: RwLock::new(None),
            heatmap_cell_aggregation: HeatmapCellAggregation::Last, // Default for sync constructor
            y_transform: None, // Sync constructor doesn't support transforms
            x_transform: None,
            n_layers: 1, // Sync constructor defaults to single layer
            layer_palette_name: None,
            layer_y_factor_names: Vec::new(), // Sync constructor defaults to empty
            chart_kind: ChartKind::Point,     // Sync constructor defaults to Point
        }
    }

    /// Enable heatmap mode with the given grid dimensions
    ///
    /// In heatmap mode:
    /// - Facet counts are overridden to 1x1 (single panel)
    /// - Axis ranges use grid dimensions: X = (-0.5, n_cols-0.5), Y = (-0.5, n_rows-0.5)
    /// - Data coordinates use .ci for X and .ri for Y (no quantization)
    ///
    /// # Arguments
    /// * `n_cols` - Number of columns in the heatmap grid (max .ci + 1)
    /// * `n_rows` - Number of rows in the heatmap grid (max .ri + 1)
    pub fn set_heatmap_mode(&mut self, n_cols: usize, n_rows: usize) {
        eprintln!(
            "DEBUG: Enabling heatmap mode with grid {}×{}",
            n_cols, n_rows
        );
        self.heatmap_mode = Some((n_cols, n_rows));
    }

    /// Get the heatmap grid dimensions if in heatmap mode
    pub fn heatmap_dims(&self) -> Option<(usize, usize)> {
        self.heatmap_mode
    }

    /// Get the original facet grid dimensions (before heatmap mode override)
    /// Returns (n_col_facets, n_row_facets) from the underlying facet_info
    pub fn original_grid_dims(&self) -> (usize, usize) {
        (
            self.facet_info.n_col_facets(),
            self.facet_info.n_row_facets(),
        )
    }

    /// Get dimensions for auto-sizing the plot
    ///
    /// Returns (n_cols, n_rows) to use for calculating plot width/height.
    /// - For heatmaps: uses the grid dimensions (tiles)
    /// - For regular plots: uses the facet counts
    pub fn sizing_dims(&self) -> (usize, usize) {
        if let Some((n_cols, n_rows)) = self.heatmap_mode {
            (n_cols, n_rows)
        } else {
            (
                self.facet_info.n_col_facets(),
                self.facet_info.n_row_facets(),
            )
        }
    }

    /// Get X-axis labels for heatmaps (from column facet schema)
    /// Returns None if not in heatmap mode or no labels available
    pub fn heatmap_x_labels(&self) -> Option<Vec<String>> {
        self.heatmap_mode?;
        let labels: Vec<String> = self
            .facet_info
            .col_facets
            .groups
            .iter()
            .map(|g| g.label.clone())
            .collect();
        if labels.is_empty() {
            None
        } else {
            Some(labels)
        }
    }

    /// Get Y-axis labels for heatmaps (from row facet schema)
    /// Returns None if not in heatmap mode or no labels available
    pub fn heatmap_y_labels(&self) -> Option<Vec<String>> {
        self.heatmap_mode?;
        let labels: Vec<String> = self
            .facet_info
            .row_facets
            .groups
            .iter()
            .map(|g| g.label.clone())
            .collect();
        if labels.is_empty() {
            None
        } else {
            Some(labels)
        }
    }

    /// Aggregate data for heatmaps by grouping on (ci, ri)
    ///
    /// This is necessary because Tercen streams raw data points, but heatmaps should display
    /// one value per cell. The aggregation method is configurable:
    /// - `Last`: Use the last data point (matches Tercen's default overdraw behavior)
    /// - `First`: Use the first data point
    /// - `Mean`: Compute the mean of all data points
    /// - `Median`: Compute the median of all data points
    ///
    /// # Returns
    /// DataFrame with one row per unique (ci, ri) cell, with aggregated values
    async fn aggregate_heatmap_data(&self) -> Result<DataFrame, Box<dyn std::error::Error>> {
        use polars::prelude::*;

        eprintln!("DEBUG: Aggregating heatmap data by (.ci, .ri)");

        let streamer = Self::create_streamer(&self.client, &self.schema_cache);

        // Build list of columns to fetch: .ci, .ri, and color factors
        // .colorLevels is shared by all categorical factors - only add once
        // Categorical colors on heatmaps are unusual but we handle them with "last"
        let mut columns = vec![".ci".to_string(), ".ri".to_string()];
        let mut has_color_levels = false;
        for color_info in &self.color_infos {
            match &color_info.mapping {
                crate::tercen::ColorMapping::Categorical(_) => {
                    if !has_color_levels {
                        columns.push(".colorLevels".to_string());
                        has_color_levels = true;
                    }
                }
                crate::tercen::ColorMapping::Continuous(_) => {
                    columns.push(color_info.factor_name.clone());
                }
            }
        }
        eprintln!(
            "DEBUG: Fetching columns for heatmap aggregation: {:?}",
            columns
        );

        // Get the actual row count from schema
        let schema = streamer.get_schema(&self.main_table_id).await?;
        let actual_total_rows = extract_row_count_from_schema(&schema)? as usize;
        eprintln!(
            "DEBUG: Schema says {} actual rows to aggregate",
            actual_total_rows
        );

        // Stream data in chunks and accumulate (TSON decoding only handles one chunk at a time)
        let chunk_size = 50000; // Larger chunks for aggregation efficiency
        let mut accumulated_dfs: Vec<polars::frame::DataFrame> = Vec::new();
        let mut offset = 0usize;

        while offset < actual_total_rows {
            let remaining = actual_total_rows - offset;
            let limit = remaining.min(chunk_size);

            let tson_data = streamer
                .stream_tson(
                    &self.main_table_id,
                    Some(columns.clone()),
                    offset as i64,
                    limit as i64,
                )
                .await?;

            if tson_data.is_empty() {
                break;
            }

            let chunk_df = tson_to_dataframe(&tson_data)?;
            let chunk_rows = chunk_df.nrow();

            if chunk_rows == 0 {
                break;
            }

            eprintln!(
                "DEBUG: Aggregation chunk: offset={}, got {} rows",
                offset, chunk_rows
            );

            accumulated_dfs.push(chunk_df.inner().clone());
            offset += chunk_rows;
        }

        eprintln!(
            "DEBUG: Accumulated {} chunks with {} total rows",
            accumulated_dfs.len(),
            offset
        );

        // Concatenate all chunks
        let all_data = if accumulated_dfs.len() == 1 {
            accumulated_dfs.into_iter().next().unwrap()
        } else {
            concat(
                accumulated_dfs
                    .iter()
                    .map(|df| df.clone().lazy())
                    .collect::<Vec<_>>(),
                UnionArgs::default(),
            )?
            .collect()?
        };

        eprintln!("DEBUG: Combined DataFrame has {} rows", all_data.height());

        // Group by .ci and .ri, aggregate based on configured method
        let ci_col = col(".ci");
        let ri_col = col(".ri");

        eprintln!(
            "DEBUG: Using heatmap cell aggregation: {:?}",
            self.heatmap_cell_aggregation
        );

        // Build aggregation expressions for color factors based on configured method
        // .colorLevels is shared by all categorical factors - only aggregate once
        let mut agg_exprs: Vec<Expr> = Vec::new();
        let mut has_color_levels_agg = false;
        for color_info in &self.color_infos {
            match &color_info.mapping {
                crate::tercen::ColorMapping::Categorical(_) => {
                    if !has_color_levels_agg {
                        // Categorical always uses last (mean/median don't make sense)
                        let expr = col(".colorLevels").last();
                        agg_exprs.push(expr.alias(".colorLevels"));
                        has_color_levels_agg = true;
                    }
                }
                crate::tercen::ColorMapping::Continuous(_) => {
                    // For continuous colors, use the configured aggregation method
                    let col_name = &color_info.factor_name;
                    let expr = match self.heatmap_cell_aggregation {
                        HeatmapCellAggregation::Last => col(col_name).last(),
                        HeatmapCellAggregation::First => col(col_name).first(),
                        HeatmapCellAggregation::Mean => col(col_name).mean(),
                        HeatmapCellAggregation::Median => col(col_name).median(),
                    };
                    agg_exprs.push(expr.alias(col_name));
                }
            }
        }

        // Perform the aggregation
        let aggregated = all_data
            .lazy()
            .group_by([ci_col, ri_col])
            .agg(agg_exprs)
            .collect()?;

        eprintln!(
            "DEBUG: Aggregated heatmap data: {} rows (from {} raw rows)",
            aggregated.height(),
            offset
        );

        // Wrap in ggrs DataFrame
        let mut df = ggrs_core::data::DataFrame::from_polars(aggregated);

        // Add color columns to the aggregated data
        if !self.color_infos.is_empty() {
            eprintln!("DEBUG: Adding color columns to aggregated data");
            df = crate::tercen::color_processor::add_color_columns(df, &self.color_infos)?;
            eprintln!("DEBUG: Color columns added to aggregated data");
        }

        Ok(df)
    }

    /// Load axis ranges from pre-computed Y-axis table
    ///
    /// The Y-axis table contains columns: .ri, .minY, .maxY (and optionally .ci)
    /// There should be one row per facet cell (indexed by .ci and .ri)
    async fn load_axis_ranges_from_table(
        client: &TercenClient,
        y_axis_table_id: &str,
        main_table_id: &str,
        facet_info: &FacetInfo,
        schema_cache: &Option<SchemaCache>,
    ) -> Result<
        (
            HashMap<(usize, usize), (AxisData, AxisData)>,
            usize, // total rows across all facets
        ),
        Box<dyn std::error::Error>,
    > {
        let streamer = Self::create_streamer(client, schema_cache);

        // First, get the schema to see which columns exist
        println!("  Fetching Y-axis table schema...");
        let schema = streamer.get_schema(y_axis_table_id).await?;
        let column_names = extract_column_names_from_schema(&schema)?;
        println!("  Y-axis table columns: {:?}", column_names);

        // Build column list: always need .minY, .maxY
        // Optionally include .ri (for per-row ranges) and .ci (for per-cell ranges)
        // Optionally include .minX, .maxX if they exist (for X-axis range)
        let mut columns_to_fetch = vec![".minY".to_string(), ".maxY".to_string()];

        let has_ri = column_names.contains(&".ri".to_string());
        if has_ri {
            columns_to_fetch.push(".ri".to_string());
        }
        let has_ci = column_names.contains(&".ci".to_string());
        if has_ci {
            columns_to_fetch.push(".ci".to_string());
        }
        let has_min_x = column_names.contains(&".minX".to_string());
        let has_max_x = column_names.contains(&".maxX".to_string());
        if has_min_x {
            columns_to_fetch.push(".minX".to_string());
        }
        if has_max_x {
            columns_to_fetch.push(".maxX".to_string());
        }

        // Log what kind of range we're dealing with
        if !has_ri && !has_ci {
            println!("  Global axis range (single row, applies to all facets)");
        } else if !has_ri {
            println!("  Per-column axis range (no .ri, applies to all rows)");
        } else if !has_ci {
            println!("  Per-row axis range (no .ci, applies to all columns)");
        } else {
            println!("  Per-cell axis range (both .ri and .ci)");
        }

        // Fetch all rows from Y-axis table
        // Note: Y-axis table has one row per row facet (indexed by .ri only)
        // Not one row per cell (col × row) because Y ranges are per row
        let expected_rows = facet_info.n_row_facets();
        println!(
            "  Fetching Y-axis ranges (expecting {} rows - one per row facet)...",
            expected_rows
        );
        let data = streamer
            .stream_tson(
                y_axis_table_id,
                Some(columns_to_fetch),
                0,
                expected_rows as i64,
            )
            .await?;

        println!("  Parsing {} bytes...", data.len());
        let df = tson_to_dataframe(&data)?;
        println!("  Parsed {} rows", df.nrow());

        // Get total row count from main table schema
        println!("  Getting main table row count...");
        let main_schema = streamer.get_schema(main_table_id).await?;
        let main_columns = extract_column_names_from_schema(&main_schema)?;
        eprintln!("DEBUG: Main data table columns: {:?}", main_columns);
        let total_rows = extract_row_count_from_schema(&main_schema)? as usize;
        println!("  Total rows: {}", total_rows);

        let mut axis_ranges = HashMap::new();
        let has_ci = df.columns().contains(&".ci".to_string());
        let has_ri = df.columns().contains(&".ri".to_string());
        let has_x_range = df.columns().contains(&".minX".to_string())
            && df.columns().contains(&".maxX".to_string());

        // Process each row in Y-axis table
        for i in 0..df.nrow() {
            let col_idx = if has_ci {
                match df.get_value(i, ".ci")? {
                    ggrs_core::data::Value::Int(v) => v as usize,
                    _ => return Err(format!("Invalid .ci at row {}", i).into()),
                }
            } else {
                0 // Will replicate to all columns below
            };

            let row_idx = if has_ri {
                let row_idx_from_table = match df.get_value(i, ".ri")? {
                    ggrs_core::data::Value::Int(v) => v as usize,
                    _ => return Err(format!("Invalid .ri at row {}", i).into()),
                };

                // Map table's .ri (filtered index 0-11) to original index (12-23 for page 2)
                // This is necessary because the Y-axis table from Tercen is pre-filtered by page
                facet_info
                    .row_facets
                    .groups
                    .get(row_idx_from_table)
                    .map(|g| g.original_index)
                    .ok_or_else(|| {
                        format!(
                            "Y-axis table row {} has .ri={} but FacetInfo only has {} row groups",
                            i,
                            row_idx_from_table,
                            facet_info.row_facets.groups.len()
                        )
                    })?
            } else {
                0 // Will replicate to all rows below
            };

            let min_y = match df.get_value(i, ".minY")? {
                ggrs_core::data::Value::Float(v) => v,
                _ => return Err(format!("Invalid .minY at row {}", i).into()),
            };

            let max_y = match df.get_value(i, ".maxY")? {
                ggrs_core::data::Value::Float(v) => v,
                _ => return Err(format!("Invalid .maxY at row {}", i).into()),
            };

            // X-axis: use from Y-axis table if available, otherwise will compute from data later
            let (min_x, max_x) = if has_x_range {
                let min_x = match df.get_value(i, ".minX")? {
                    ggrs_core::data::Value::Float(v) => v,
                    _ => return Err(format!("Invalid .minX at row {}", i).into()),
                };
                let max_x = match df.get_value(i, ".maxX")? {
                    ggrs_core::data::Value::Float(v) => v,
                    _ => return Err(format!("Invalid .maxX at row {}", i).into()),
                };
                (min_x, max_x)
            } else {
                // X range not in Y-axis table - use placeholder, will compute from data
                // This matches R plot_operator behavior which computes range from actual .x values
                (f64::NAN, f64::NAN)
            };

            println!(
                "  Range row {}: ci={}, ri={}, X [{}, {}], Y [{}, {}]",
                i, col_idx, row_idx, min_x, max_x, min_y, max_y
            );

            let x_axis = AxisData::Numeric(NumericAxisData {
                min_value: min_x,
                max_value: max_x,
                min_axis: min_x,
                max_axis: max_x,
                transform: None,
            });

            let y_axis = AxisData::Numeric(NumericAxisData {
                min_value: min_y,
                max_value: max_y,
                min_axis: min_y,
                max_axis: max_y,
                transform: None,
            });

            // Replicate range based on which index columns are present
            match (has_ci, has_ri) {
                (true, true) => {
                    // Per-cell range
                    axis_ranges.insert((col_idx, row_idx), (x_axis.clone(), y_axis.clone()));
                }
                (false, true) => {
                    // Per-row range: replicate to all columns
                    for col in 0..facet_info.n_col_facets() {
                        axis_ranges.insert((col, row_idx), (x_axis.clone(), y_axis.clone()));
                    }
                }
                (true, false) => {
                    // Per-column range: replicate to all rows
                    for row in 0..facet_info.n_row_facets() {
                        axis_ranges.insert((col_idx, row), (x_axis.clone(), y_axis.clone()));
                    }
                }
                (false, false) => {
                    // Global range: replicate to all cells
                    for col in 0..facet_info.n_col_facets() {
                        for row in 0..facet_info.n_row_facets() {
                            axis_ranges.insert((col, row), (x_axis.clone(), y_axis.clone()));
                        }
                    }
                }
            }
        }

        println!("  Loaded {} axis ranges", axis_ranges.len());
        Ok((axis_ranges, total_rows))
    }
    /// Compute X-axis ranges by scanning the main data table
    /// Set sequential X ranges when no X-axis table exists
    ///
    /// When there's no X-axis table, X values are sequential (1 to n_rows).
    /// This is much simpler than scanning data - just use the row count.
    fn set_sequential_x_ranges(
        n_rows: f64,
        axis_ranges: &mut HashMap<(usize, usize), (AxisData, AxisData)>,
    ) {
        // Sequential X range: 1 to n_rows (1-indexed)
        let min_x = 1.0;
        let max_x = n_rows;

        // Update all facet cells with the same sequential range
        for (x_axis, _y_axis) in axis_ranges.values_mut() {
            *x_axis = AxisData::Numeric(NumericAxisData {
                min_value: min_x,
                max_value: max_x,
                min_axis: min_x,
                max_axis: max_x,
                transform: None,
            });
        }
    }

    /// Load X-axis ranges from pre-computed X-axis table
    ///
    /// The X-axis table contains columns: .ci, .ticks, .minX, .maxX
    /// There should be one row per column facet (indexed by .ci)
    async fn load_x_ranges_from_table(
        client: &TercenClient,
        x_axis_table_id: &str,
        facet_info: &FacetInfo,
        axis_ranges: &mut HashMap<(usize, usize), (AxisData, AxisData)>,
        schema_cache: &Option<SchemaCache>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let streamer = Self::create_streamer(client, schema_cache);

        // Fetch the X-axis table schema
        println!("  Fetching X-axis table schema...");
        let schema = streamer.get_schema(x_axis_table_id).await?;
        let column_names = extract_column_names_from_schema(&schema)?;
        println!("  X-axis table columns: {:?}", column_names);

        // Check for required columns
        let has_ci = column_names.contains(&".ci".to_string());
        let has_min_x = column_names.contains(&".minX".to_string());
        let has_max_x = column_names.contains(&".maxX".to_string());

        if !has_min_x || !has_max_x {
            return Err("X-axis table missing .minX or .maxX columns".into());
        }

        // Build column list
        let mut columns_to_fetch = vec![".minX".to_string(), ".maxX".to_string()];
        if has_ci {
            columns_to_fetch.push(".ci".to_string());
        }

        // Log the range type
        if has_ci {
            println!("  Per-column X-axis range (indexed by .ci)");
        } else {
            println!("  Global X-axis range (single row, applies to all columns)");
        }

        // Fetch all rows from X-axis table
        let expected_rows = facet_info.n_col_facets();
        println!(
            "  Fetching X-axis ranges (expecting {} rows - one per col facet)...",
            expected_rows
        );
        let data = streamer
            .stream_tson(
                x_axis_table_id,
                Some(columns_to_fetch),
                0,
                expected_rows as i64,
            )
            .await?;

        println!("  Parsing {} bytes...", data.len());
        let df = tson_to_dataframe(&data)?;
        println!("  Parsed {} rows", df.nrow());

        let has_ci = df.columns().contains(&".ci".to_string());

        // Process each row in X-axis table
        for i in 0..df.nrow() {
            let col_idx = if has_ci {
                let col_idx_from_table = match df.get_value(i, ".ci")? {
                    ggrs_core::data::Value::Int(v) => v as usize,
                    _ => return Err(format!("Invalid .ci at row {}", i).into()),
                };

                // Map table's .ci (filtered index) to original index
                facet_info
                    .col_facets
                    .groups
                    .get(col_idx_from_table)
                    .map(|g| g.original_index)
                    .ok_or_else(|| {
                        format!(
                            "X-axis table row {} has .ci={} but FacetInfo only has {} col groups",
                            i,
                            col_idx_from_table,
                            facet_info.col_facets.groups.len()
                        )
                    })?
            } else {
                0 // Will replicate to all columns below
            };

            let min_x = match df.get_value(i, ".minX")? {
                ggrs_core::data::Value::Float(v) => v,
                _ => return Err(format!("Invalid .minX at row {}", i).into()),
            };

            let max_x = match df.get_value(i, ".maxX")? {
                ggrs_core::data::Value::Float(v) => v,
                _ => return Err(format!("Invalid .maxX at row {}", i).into()),
            };

            println!(
                "  X range row {}: ci={}, X [{}, {}]",
                i, col_idx, min_x, max_x
            );

            // Update axis_ranges based on whether we have per-column or global range
            if has_ci {
                // Per-column range: update all rows for this column
                for row_idx in 0..facet_info.n_row_facets() {
                    let row_original_idx = facet_info
                        .row_facets
                        .groups
                        .get(row_idx)
                        .expect("row_idx within n_row_facets() bounds must be valid")
                        .original_index;

                    if let Some((x_axis, _)) = axis_ranges.get_mut(&(col_idx, row_original_idx)) {
                        *x_axis = AxisData::Numeric(NumericAxisData {
                            min_value: min_x,
                            max_value: max_x,
                            min_axis: min_x,
                            max_axis: max_x,
                            transform: None,
                        });
                    }
                }
            } else {
                // Global range: update all cells
                for (_, (x_axis, _)) in axis_ranges.iter_mut() {
                    *x_axis = AxisData::Numeric(NumericAxisData {
                        min_value: min_x,
                        max_value: max_x,
                        min_axis: min_x,
                        max_axis: max_x,
                        transform: None,
                    });
                }
            }
        }

        println!("  Loaded X-axis ranges from table");
        Ok(())
    }

    /// Create a generic legend for level-based colors
    ///
    /// When we can't get actual category names, use generic labels: "Level 0", "Level 1", etc.
    fn create_generic_level_legend(
        factor_name: &str,
    ) -> Result<LegendScale, Box<dyn std::error::Error>> {
        let entries: Vec<(String, [u8; 3])> = (0..DEFAULT_PALETTE_LEVELS)
            .map(|i| {
                let label = format!("Level {}", i);
                let color = crate::tercen::categorical_color_from_level(i as i32);
                (label, color)
            })
            .collect();
        Ok(LegendScale::Discrete {
            entries,
            aesthetic_name: factor_name.to_string(),
        })
    }

    /// Load legend scale data during initialization
    ///
    /// For categorical colors, uses n_levels from color table schema.
    /// For continuous colors, extracts the min/max from the palette.
    /// For mixed-layer scenarios (some layers with colors, some without),
    /// creates a combined legend with sections for each type.
    fn load_legend_scale(
        color_infos: &[crate::tercen::ColorInfo],
        per_layer_colors: Option<&crate::tercen::PerLayerColorConfig>,
        layer_y_factor_names: &[String],
    ) -> Result<LegendScale, Box<dyn std::error::Error>> {
        // Handle mixed-layer scenarios
        if let Some(plc) = per_layer_colors {
            if plc.is_mixed() {
                // Mixed scenario: some layers have colors, some don't
                eprintln!("DEBUG: Building combined legend for mixed-layer scenario");
                return Self::build_combined_legend(plc, layer_y_factor_names);
            }

            // Not mixed - if all layers have constant colors, create a discrete legend
            if plc.has_constant_colors() && !plc.has_explicit_colors() {
                eprintln!("DEBUG: Building discrete legend for layer-based colors");
                return Self::build_layer_based_legend(plc, layer_y_factor_names);
            }
        }

        // Standard case: use legacy color_infos
        if color_infos.is_empty() {
            return Ok(LegendScale::None);
        }

        // Build combined aesthetic name from all categorical factor names
        let categorical_names: Vec<&str> = color_infos
            .iter()
            .filter(|ci| matches!(ci.mapping, crate::tercen::ColorMapping::Categorical(_)))
            .map(|ci| ci.factor_name.as_str())
            .collect();
        let combined_name = if categorical_names.is_empty() {
            color_infos[0].factor_name.clone()
        } else {
            categorical_names.join(", ")
        };

        // Only handle the first color factor for now
        let color_info = &color_infos[0];

        match &color_info.mapping {
            crate::tercen::ColorMapping::Continuous(palette) => {
                // For continuous colors, get the min/max and color stops from the palette
                if let Some((min_val, max_val)) = palette.range() {
                    // Convert Tercen ColorStops to GGRS LegendColorStops
                    let color_stops: Vec<LegendColorStop> = palette
                        .stops
                        .iter()
                        .map(|stop| LegendColorStop::new(stop.value, stop.color))
                        .collect();

                    eprintln!(
                        "DEBUG: Legend scale using {} color stops from palette (range: {} to {})",
                        color_stops.len(),
                        min_val,
                        max_val
                    );

                    Ok(LegendScale::Continuous {
                        min: min_val,
                        max: max_val,
                        aesthetic_name: color_info.factor_name.clone(),
                        color_stops,
                    })
                } else {
                    // Empty palette - no legend
                    Ok(LegendScale::None)
                }
            }
            crate::tercen::ColorMapping::Categorical(color_map) => {
                // For categorical colors, check if we have explicit mappings
                if !color_map.mappings.is_empty() {
                    // Explicit label→color mappings from palette
                    let mut entries: Vec<(String, [u8; 3])> = color_map
                        .mappings
                        .iter()
                        .map(|(label, color)| (label.clone(), *color))
                        .collect();
                    entries.sort_by(|a, b| a.0.cmp(&b.0));

                    Ok(LegendScale::Discrete {
                        entries,
                        aesthetic_name: combined_name.clone(),
                    })
                } else if let Some(ref labels) = color_info.color_labels {
                    // Use actual color labels from the color table with palette colors
                    eprintln!(
                        "DEBUG: Using {} color labels from color table for '{}'",
                        labels.len(),
                        combined_name
                    );
                    let entries: Vec<(String, [u8; 3])> = labels
                        .iter()
                        .enumerate()
                        .map(|(i, label)| {
                            let color = crate::tercen::categorical_color_from_level(i as i32);
                            (label.clone(), color)
                        })
                        .collect();
                    Ok(LegendScale::Discrete {
                        entries,
                        aesthetic_name: combined_name.clone(),
                    })
                } else if let Some(n_levels) = color_info.n_levels {
                    // Fallback: Use n_levels from color table schema with generic labels
                    eprintln!(
                        "DEBUG: Using n_levels={} with generic labels for '{}' (no color_labels)",
                        n_levels, combined_name
                    );
                    let entries: Vec<(String, [u8; 3])> = (0..n_levels)
                        .map(|i| {
                            let label = format!("Level {}", i);
                            let color = crate::tercen::categorical_color_from_level(i as i32);
                            (label, color)
                        })
                        .collect();
                    Ok(LegendScale::Discrete {
                        entries,
                        aesthetic_name: combined_name.clone(),
                    })
                } else {
                    // No explicit mappings and no n_levels - use default generic level labels
                    eprintln!(
                        "DEBUG: No explicit mappings or n_levels, using default generic level labels"
                    );
                    Self::create_generic_level_legend(&combined_name)
                }
            }
        }
    }

    /// Build a combined legend for mixed-layer scenarios
    ///
    /// Creates legend sections for:
    /// - Layers with explicit color factors (continuous or discrete)
    /// - Layers without color factors (discrete with Y-factor name and pre-computed layer color)
    fn build_combined_legend(
        per_layer_colors: &crate::tercen::PerLayerColorConfig,
        layer_y_factor_names: &[String],
    ) -> Result<LegendScale, Box<dyn std::error::Error>> {
        use crate::tercen::LayerColorConfig;

        let mut sections: Vec<LegendSection> = Vec::new();

        for (layer_idx, config) in per_layer_colors.layer_configs.iter().enumerate() {
            match config {
                LayerColorConfig::Continuous {
                    palette,
                    factor_name,
                    ..
                } => {
                    // Layer has continuous colors
                    if let Some((min_val, max_val)) = palette.range() {
                        let color_stops: Vec<LegendColorStop> = palette
                            .stops
                            .iter()
                            .map(|stop| LegendColorStop::new(stop.value, stop.color))
                            .collect();
                        sections.push(LegendSection::Continuous {
                            min: min_val,
                            max: max_val,
                            title: factor_name.clone(),
                            color_stops,
                        });
                        eprintln!(
                            "DEBUG: Added continuous section for layer {} ('{}')",
                            layer_idx, factor_name
                        );
                    }
                }
                LayerColorConfig::Categorical {
                    color_map,
                    factor_name,
                    ..
                } => {
                    // Layer has categorical colors
                    let entries: Vec<(String, [u8; 3])> = if !color_map.mappings.is_empty() {
                        color_map
                            .mappings
                            .iter()
                            .map(|(label, color)| (label.clone(), *color))
                            .collect()
                    } else {
                        // Fallback: generate from levels
                        Vec::new()
                    };

                    if !entries.is_empty() {
                        sections.push(LegendSection::Discrete {
                            entries,
                            title: factor_name.clone(),
                        });
                        eprintln!(
                            "DEBUG: Added discrete section for layer {} ('{}')",
                            layer_idx, factor_name
                        );
                    }
                }
                LayerColorConfig::Constant { color } => {
                    // Layer has no color factor - use Y-factor name and pre-computed color
                    let y_factor_name = layer_y_factor_names
                        .get(layer_idx)
                        .cloned()
                        .unwrap_or_else(|| format!("Layer {}", layer_idx + 1));

                    // Create a single-entry discrete section
                    sections.push(LegendSection::Discrete {
                        entries: vec![(y_factor_name.clone(), *color)],
                        title: String::new(), // No separate title, the entry IS the label
                    });
                    eprintln!(
                        "DEBUG: Added layer-based entry for layer {} (Y-factor: '{}', color: {:?})",
                        layer_idx, y_factor_name, color
                    );
                }
            }
        }

        if sections.is_empty() {
            Ok(LegendScale::None)
        } else if sections.len() == 1 {
            // Single section - convert to non-combined form
            let section = sections.into_iter().next().unwrap();
            match section {
                LegendSection::Continuous {
                    min,
                    max,
                    title,
                    color_stops,
                } => Ok(LegendScale::Continuous {
                    min,
                    max,
                    aesthetic_name: title,
                    color_stops,
                }),
                LegendSection::Discrete { entries, title } => Ok(LegendScale::Discrete {
                    entries,
                    aesthetic_name: title,
                }),
            }
        } else {
            Ok(LegendScale::Combined { sections })
        }
    }

    /// Build a discrete legend for layer-based colors (all layers with constant colors)
    fn build_layer_based_legend(
        per_layer_colors: &crate::tercen::PerLayerColorConfig,
        layer_y_factor_names: &[String],
    ) -> Result<LegendScale, Box<dyn std::error::Error>> {
        use crate::tercen::LayerColorConfig;

        let entries: Vec<(String, [u8; 3])> = per_layer_colors
            .layer_configs
            .iter()
            .enumerate()
            .filter_map(|(layer_idx, config)| {
                if let LayerColorConfig::Constant { color } = config {
                    let y_factor_name = layer_y_factor_names
                        .get(layer_idx)
                        .cloned()
                        .unwrap_or_else(|| format!("Layer {}", layer_idx + 1));
                    Some((y_factor_name, *color))
                } else {
                    None
                }
            })
            .collect();

        eprintln!(
            "DEBUG: Built layer-based legend with {} entries",
            entries.len()
        );

        Ok(LegendScale::Discrete {
            entries,
            aesthetic_name: String::new(), // No title for layer-based legend
        })
    }

    // Stream data for a specific facet cell in chunks
    // NOTE: Per-facet streaming not used - commented out since GGRS uses bulk mode
    /// Stream data in bulk across ALL facets (includes .ci and .ri columns)
    async fn stream_bulk_data(
        &self,
        data_range: Range,
    ) -> Result<DataFrame, Box<dyn std::error::Error>> {
        eprintln!(
            "DEBUG: stream_bulk_data called with range {}..{} (requesting {} rows)",
            data_range.start,
            data_range.end,
            data_range.end - data_range.start
        );

        let streamer = Self::create_streamer(&self.client, &self.schema_cache);

        // For bulk streaming, include facet indices and quantized coordinates
        // Note: We DON'T request .x/.y columns - axis ranges come from:
        //   - Y-axis table (always exists, Y is mandatory)
        //   - X-axis table (if continuous X axis defined)
        //   - Or 1..n_rows for sequential X (when no X-axis table)
        // The .xs/.ys columns are quantized (0-65535) for GGRS rendering
        let mut columns = vec![
            ".ci".to_string(),
            ".ri".to_string(),
            ".xs".to_string(),
            ".ys".to_string(),
        ];

        // Add chart-type specific columns
        match self.chart_kind {
            ChartKind::Bar => {
                // Bar charts need .y0s (baseline) for stacked bars
                columns.push(".y0s".to_string());
                eprintln!("DEBUG: Bar chart - fetching .y0s baseline column");
            }
            ChartKind::Line => {
                // Line charts may need ordering/grouping columns in the future
                eprintln!("DEBUG: Line chart - no extra columns needed yet");
            }
            ChartKind::Heatmap | ChartKind::Point => {
                // Standard columns are sufficient
            }
        }

        // NOTE: Don't add page_factors to columns!
        // Page factors exist in facet tables, not the main data table.
        // We've already filtered facets by page, so data filtering is via .ri matching.

        // Add color columns based on configuration
        // Determine which color handling mode we're in:
        // 1. Mixed-layer: some layers have colors, some don't (use per_layer_colors)
        // 2. Pure layer-based: no colors on any layer (use layer palette)
        // 3. Uniform colors: all layers share the same color config (legacy color_infos)

        let is_mixed_layer = self
            .per_layer_colors
            .as_ref()
            .map(|plc| plc.is_mixed())
            .unwrap_or(false);
        let use_layer_colors = self.color_infos.is_empty() && self.n_layers > 1 && !is_mixed_layer;

        // Always fetch .axisIndex when multiple layers exist (for shape cycling and color handling)
        if self.n_layers > 1 && !columns.contains(&".axisIndex".to_string()) {
            columns.push(".axisIndex".to_string());
            eprintln!(
                "DEBUG: Multi-layer ({} layers) - fetching .axisIndex for shape/color cycling",
                self.n_layers
            );
        }

        if is_mixed_layer {
            // Mixed-layer scenario: always need .axisIndex to determine which coloring to use
            columns.push(".axisIndex".to_string());
            eprintln!(
                "DEBUG: Mixed-layer coloring ({} layers) - fetching .axisIndex",
                self.n_layers
            );

            // Fetch color columns for layers that have explicit colors
            if let Some(ref plc) = self.per_layer_colors {
                use crate::tercen::LayerColorConfig;
                for (layer_idx, config) in plc.layer_configs.iter().enumerate() {
                    match config {
                        LayerColorConfig::Categorical { .. } => {
                            if !columns.contains(&".colorLevels".to_string()) {
                                columns.push(".colorLevels".to_string());
                                eprintln!(
                                    "DEBUG: Layer {} has categorical colors - fetching .colorLevels",
                                    layer_idx
                                );
                            }
                        }
                        LayerColorConfig::Continuous { factor_name, .. } => {
                            if !columns.contains(factor_name) {
                                columns.push(factor_name.clone());
                                eprintln!(
                                    "DEBUG: Layer {} has continuous colors - fetching '{}'",
                                    layer_idx, factor_name
                                );
                            }
                        }
                        LayerColorConfig::Constant { color } => {
                            eprintln!("DEBUG: Layer {} has constant color {:?}", layer_idx, color);
                        }
                    }
                }
            }
        } else if use_layer_colors {
            // Pure layer-based coloring: fetch .axisIndex
            columns.push(".axisIndex".to_string());
            eprintln!(
                "DEBUG: Multi-layer ({} layers) with no colors - will use layer-based coloring",
                self.n_layers
            );
        } else {
            // Legacy uniform colors: all layers share the same color config
            for color_info in &self.color_infos {
                match &color_info.mapping {
                    crate::tercen::ColorMapping::Categorical(_) => {
                        // Add .colorLevels for categorical colors
                        if !columns.contains(&".colorLevels".to_string()) {
                            columns.push(".colorLevels".to_string());
                        }
                    }
                    crate::tercen::ColorMapping::Continuous(_) => {
                        // Add the factor column for continuous colors
                        columns.push(color_info.factor_name.clone());
                    }
                }
            }
        }

        // Fetch the requested range directly (GGRS handles chunking)
        let offset = data_range.start as i64;
        let limit = (data_range.end - data_range.start) as i64;

        eprintln!(
            "DEBUG: Calling stream_tson with offset={}, limit={}",
            offset, limit
        );
        eprintln!("DEBUG: Requested columns: {:?}", columns);

        // Stream data from Tercen (no caching - GGRS handles caching)
        let tson_data = streamer
            .stream_tson(&self.main_table_id, Some(columns.clone()), offset, limit)
            .await?;

        eprintln!("DEBUG: stream_tson returned {} bytes", tson_data.len());

        if tson_data.is_empty() {
            eprintln!("DEBUG: Empty TSON data, returning empty DataFrame");
            return Ok(ggrs_core::data::DataFrame::new());
        }

        // Parse TSON to DataFrame - contains .ci, .ri, .xs, .ys, and color factors
        let mut df = tson_to_dataframe(&tson_data)?;
        eprintln!("DEBUG: Parsed DataFrame with {} rows", df.nrow());
        eprintln!("DEBUG: Returned columns: {:?}", df.columns());

        // DEBUG: Print heatmap column info (first chunk only)
        if data_range.start == 0 {
            let polars_df = df.inner();
            if let Ok(n_x_levels) = polars_df.column(".nXLevels") {
                if let Ok(n_x_i64) = n_x_levels.i64() {
                    let n_levels = n_x_i64.get(0).unwrap_or(0);
                    eprintln!("DEBUG HEATMAP: Total X levels (columns) = {}", n_levels);
                }
            }
            // Compare .xs, .ys, .xLevels
            if let (Ok(xs_col), Ok(ys_col), Ok(xl_col)) = (
                polars_df.column(".xs"),
                polars_df.column(".ys"),
                polars_df.column(".xLevels"),
            ) {
                if let (Ok(xs_i64), Ok(ys_i64), Ok(xl_i64)) =
                    (xs_col.i64(), ys_col.i64(), xl_col.i64())
                {
                    let tuples: Vec<(i64, i64, i64)> = xs_i64
                        .iter()
                        .zip(ys_i64.iter())
                        .zip(xl_i64.iter())
                        .take(10)
                        .filter_map(|((x, y), l)| match (x, y, l) {
                            (Some(x), Some(y), Some(l)) => Some((x, y, l)),
                            _ => None,
                        })
                        .collect();
                    eprintln!("DEBUG HEATMAP: First 10 (xs, ys, xLevels): {:?}", tuples);
                }
            }
        }

        // NO FILTERING! Operator is dumb - just streams raw data.
        // GGRS handles all filtering using original_index mapping.

        // Map color values to RGB based on the coloring mode
        // Priority order:
        // 1. Per-layer colors (multi-layer: respects .axisIndex for per-layer color config)
        // 2. Legacy color_infos (single-layer: explicit color factors from step)
        // 3. Layer-based coloring (multiple layers, no explicit colors — palette by .axisIndex)

        if let Some(ref plc) = self.per_layer_colors {
            // Multi-layer: per-layer color config (handles mixed, explicit, and constant)
            eprintln!(
                "DEBUG: Adding per-layer colors for {} layers (explicit={}, mixed={}, constant={})",
                self.n_layers,
                plc.has_explicit_colors(),
                plc.is_mixed(),
                plc.has_constant_colors()
            );
            df = crate::tercen::color_processor::add_mixed_layer_colors(df, plc)?;
            eprintln!("DEBUG: Per-layer colors added successfully");
        } else if !self.color_infos.is_empty() {
            // Single-layer: legacy uniform colors (explicit color factors)
            eprintln!(
                "DEBUG: Adding color columns for {} color factors (legacy path)",
                self.color_infos.len()
            );
            df = crate::tercen::color_processor::add_color_columns(df, &self.color_infos)?;
            eprintln!("DEBUG: Color columns added successfully");
        } else if use_layer_colors {
            // Pure layer-based coloring (no color factors on any layer)
            eprintln!(
                "DEBUG: Adding layer-based colors for {} layers using palette {:?}",
                self.n_layers, self.layer_palette_name
            );
            df = crate::tercen::color_processor::add_layer_colors(
                df,
                self.layer_palette_name.as_deref(),
            )?;
            eprintln!("DEBUG: Layer colors added successfully");
        }

        Ok(df)
    }

    // NOTE: Dequantization now happens in GGRS, not in the operator
    // Coordinates: .xs/.ys (uint16 0-65535) → .x/.y (actual data values)
    // This transformation is backend-agnostic and happens in GGRS before rendering
    // Uses the pre-computed axis ranges for each specific facet cell

    // NOTE: Facet filtering not used - commented out since GGRS does internal filtering in bulk mode
    // Filter DataFrame to only include rows for a specific facet cell
    // For single facet: Returns data as-is (no filtering needed)
    // For multiple facets: Filters by .ci and .ri, then drops those columns
}

impl StreamGenerator for TercenStreamGenerator {
    fn n_col_facets(&self) -> usize {
        // In heatmap mode, we have a single panel (1x1 facets)
        if self.heatmap_mode.is_some() {
            return 1;
        }
        self.facet_info.n_col_facets()
    }

    fn n_row_facets(&self) -> usize {
        // In heatmap mode, we have a single panel (1x1 facets)
        if self.heatmap_mode.is_some() {
            return 1;
        }
        self.facet_info.n_row_facets()
    }

    fn n_total_data_rows(&self) -> usize {
        // For heatmaps, return the number of tiles (aggregated data rows)
        // instead of raw data rows
        if let Some((n_cols, n_rows)) = self.heatmap_mode {
            let n_tiles = n_cols * n_rows;
            eprintln!(
                "DEBUG: Heatmap mode - returning {} tiles as total rows ({}×{})",
                n_tiles, n_cols, n_rows
            );
            return n_tiles;
        }
        self.total_rows
    }

    fn query_col_facet_labels(&self) -> DataFrame {
        use polars::prelude::{NamedFrom, Series};

        // Create a Polars Series from the labels
        let labels: Vec<String> = self
            .facet_info
            .col_facets
            .groups
            .iter()
            .map(|group| group.label.clone())
            .collect();

        if labels.is_empty() {
            return ggrs_core::data::DataFrame::new();
        }

        // Use the first non-empty column name from the facet metadata, or "label" as fallback
        let column_name = self
            .facet_info
            .col_facets
            .column_names
            .first()
            .and_then(|name| {
                if name.is_empty() {
                    None
                } else {
                    Some(name.clone())
                }
            })
            .unwrap_or_else(|| "label".to_string());

        let series = Series::new(column_name.into(), labels);
        let polars_df = polars::frame::DataFrame::new(vec![series.into_column()])
            .expect("DataFrame creation from single series should not fail");

        ggrs_core::data::DataFrame::from_polars(polars_df)
    }

    fn query_row_facet_labels(&self) -> DataFrame {
        use polars::prelude::{NamedFrom, Series};

        // Create a Polars Series from the labels
        let labels: Vec<String> = self
            .facet_info
            .row_facets
            .groups
            .iter()
            .map(|group| group.label.clone())
            .collect();

        if labels.is_empty() {
            return ggrs_core::data::DataFrame::new();
        }

        // Use the first non-empty column name from the facet metadata, or "label" as fallback
        let column_name = self
            .facet_info
            .row_facets
            .column_names
            .first()
            .and_then(|name| {
                if name.is_empty() {
                    None
                } else {
                    Some(name.clone())
                }
            })
            .unwrap_or_else(|| "label".to_string());

        let series = Series::new(column_name.into(), labels);
        let polars_df = polars::frame::DataFrame::new(vec![series.into_column()])
            .expect("DataFrame creation from single series should not fail");

        ggrs_core::data::DataFrame::from_polars(polars_df)
    }

    fn query_x_axis(&self, col_idx: usize, row_idx: usize) -> AxisData {
        // In heatmap mode, return categorical axis with facet labels
        if let Some((n_cols, _)) = self.heatmap_mode {
            // Get labels from column facet schema
            let categories: Vec<String> = self
                .facet_info
                .col_facets
                .groups
                .iter()
                .map(|g| g.label.clone())
                .collect();

            // If we have labels, return categorical; otherwise fall back to numeric
            if !categories.is_empty() && categories.len() == n_cols {
                return AxisData::Categorical(CategoricalAxisData { categories });
            }
            // Fallback: return numeric if labels don't match grid size
            return AxisData::Numeric(NumericAxisData {
                min_value: -0.5,
                max_value: n_cols as f64 - 0.5,
                min_axis: -0.5,
                max_axis: n_cols as f64 - 0.5,
                transform: None,
            });
        }

        // Translate grid position to original indices
        // axis_ranges is keyed by (original_col_idx, original_row_idx)
        let original_col_idx = self.get_original_col_idx(col_idx);
        let original_row_idx = self.get_original_row_idx(row_idx);

        self.axis_ranges
            .get(&(original_col_idx, original_row_idx))
            .map(|(x_axis, _)| x_axis.clone())
            .unwrap_or_else(|| {
                panic!(
                    "No X-axis range for cell ({}, {}) [original: ({}, {})]. \
                    axis_ranges has {} entries. This indicates missing axis range data.",
                    col_idx,
                    row_idx,
                    original_col_idx,
                    original_row_idx,
                    self.axis_ranges.len()
                )
            })
    }

    fn query_y_axis(&self, col_idx: usize, row_idx: usize) -> AxisData {
        // In heatmap mode, return categorical axis with facet labels
        if let Some((_, n_rows)) = self.heatmap_mode {
            // Get labels from row facet schema
            let categories: Vec<String> = self
                .facet_info
                .row_facets
                .groups
                .iter()
                .map(|g| g.label.clone())
                .collect();

            // If we have labels, return categorical; otherwise fall back to numeric
            if !categories.is_empty() && categories.len() == n_rows {
                return AxisData::Categorical(CategoricalAxisData { categories });
            }
            // Fallback: return numeric if labels don't match grid size
            return AxisData::Numeric(NumericAxisData {
                min_value: -0.5,
                max_value: n_rows as f64 - 0.5,
                min_axis: -0.5,
                max_axis: n_rows as f64 - 0.5,
                transform: None,
            });
        }

        // Translate grid position to original indices
        // axis_ranges is keyed by (original_col_idx, original_row_idx)
        let original_col_idx = self.get_original_col_idx(col_idx);
        let original_row_idx = self.get_original_row_idx(row_idx);

        self.axis_ranges
            .get(&(original_col_idx, original_row_idx))
            .map(|(_, y_axis)| y_axis.clone())
            .unwrap_or_else(|| {
                panic!(
                    "No Y-axis range for cell ({}, {}) [original: ({}, {})]. \
                    axis_ranges has {} entries. This indicates missing axis range data.",
                    col_idx,
                    row_idx,
                    original_col_idx,
                    original_row_idx,
                    self.axis_ranges.len()
                )
            })
    }

    fn query_legend_scale(&self) -> LegendScale {
        // Return cached legend scale (loaded during initialization)
        self.cached_legend_scale.clone()
    }

    fn query_color_metadata(&self) -> ggrs_core::stream::ColorMetadata {
        // Tercen pre-computes colors in add_color_columns(), add_layer_colors(), or add_mixed_layer_colors()
        // The .color column contains ready-to-use packed RGB values
        // Legend metadata is provided via query_legend_scale()
        // No scale training needed - colors are ready to use

        let has_mixed_layer_colors = self
            .per_layer_colors
            .as_ref()
            .map(|plc| plc.is_mixed() || plc.has_explicit_colors())
            .unwrap_or(false);
        let has_explicit_colors = !self.color_infos.is_empty();
        let has_layer_colors = self.color_infos.is_empty() && self.n_layers > 1;

        if has_mixed_layer_colors || has_explicit_colors || has_layer_colors {
            // Colors are pre-computed by Tercen (either from color factors, layers, or mixed)
            ggrs_core::stream::ColorMetadata::Precomputed
        } else {
            // No color aesthetic configured (single layer, no colors)
            ggrs_core::stream::ColorMetadata::Unknown
        }
    }

    fn facet_spec(&self) -> &FacetSpec {
        &self.facet_spec
    }

    fn aes(&self) -> &Aes {
        &self.aes
    }

    fn preferred_chunk_size(&self) -> Option<usize> {
        // Return the chunk size from operator config
        // This allows Tercen operator to communicate its optimal chunk size
        // based on gRPC message efficiency and memory constraints
        Some(self.chunk_size)
    }

    // NOTE: Per-facet streaming not used - GGRS uses bulk mode for faceted plots
    fn query_data_chunk(&self, _col_idx: usize, _row_idx: usize, _data_range: Range) -> DataFrame {
        panic!(
            "query_data_chunk should not be called - GGRS uses bulk mode (query_data_multi_facet)"
        )
    }

    fn query_data_multi_facet(&self, data_range: Range) -> DataFrame {
        // For heatmaps, aggregate all data by (ci, ri) and return mean values
        // This ensures the displayed color reflects the aggregate (mean) rather than
        // the last data point drawn (which would depend on streaming order)
        if self.heatmap_mode.is_some() {
            // Check if we already have cached aggregated data
            {
                let cache_read = self.heatmap_cached_data.read().unwrap();
                if cache_read.is_some() {
                    // Data already aggregated and returned on first call
                    // Return empty DataFrame for subsequent calls
                    if data_range.start > 0 {
                        eprintln!(
                            "DEBUG: Heatmap data already returned, returning empty for range {}..{}",
                            data_range.start, data_range.end
                        );
                        return DataFrame::new();
                    }
                    // Return the cached data on first call
                    eprintln!("DEBUG: Returning cached aggregated heatmap data");
                    return cache_read.as_ref().unwrap().clone();
                }
            }

            // First call - aggregate and cache
            eprintln!("DEBUG: First heatmap data request - aggregating all data");
            let aggregated = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async { self.aggregate_heatmap_data().await })
            })
            .unwrap_or_else(|e| {
                panic!(
                    "Failed to aggregate heatmap data: {}. \
                    This indicates a data processing error.",
                    e
                )
            });

            // Cache the aggregated data
            {
                let mut cache_write = self.heatmap_cached_data.write().unwrap();
                *cache_write = Some(aggregated.clone());
            }

            eprintln!(
                "DEBUG: Returning {} aggregated heatmap rows",
                aggregated.nrow()
            );
            return aggregated;
        }

        // Non-heatmap: stream data as usual
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.stream_bulk_data(data_range).await })
        })
        .unwrap_or_else(|e| {
            panic!(
                "Failed to fetch bulk data from Tercen: {}. \
                This indicates a network error or invalid table configuration.",
                e
            )
        })
    }

    fn get_original_col_idx(&self, col_idx: usize) -> usize {
        // Look up the FacetGroup at col_idx and return its original_index
        // For pagination: col_idx is grid position (0-11), original_index is data .ci value
        self.facet_info
            .col_facets
            .groups
            .get(col_idx)
            .map(|group| group.original_index)
            .unwrap_or_else(|| {
                panic!(
                    "Invalid col_idx {}: FacetInfo only has {} column groups. \
                    This is a bug in facet metadata construction.",
                    col_idx,
                    self.facet_info.col_facets.groups.len()
                )
            })
    }

    fn get_original_row_idx(&self, row_idx: usize) -> usize {
        // Look up the FacetGroup at row_idx and return its original_index
        // For pagination: row_idx is grid position (0-11), original_index is data .ri value (12-23 for male)
        self.facet_info
            .row_facets
            .groups
            .get(row_idx)
            .map(|group| group.original_index)
            .unwrap_or_else(|| {
                panic!(
                    "Invalid row_idx {}: FacetInfo only has {} row groups. \
                    This is a bug in facet metadata construction.",
                    row_idx,
                    self.facet_info.row_facets.groups.len()
                )
            })
    }
}
