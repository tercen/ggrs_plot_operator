//! Tercen Stream Generator - Bridges Tercen data with GGRS plotting
//!
//! This module implements the GGRS `StreamGenerator` trait for Tercen,
//! enabling lazy loading of data directly from Tercen's gRPC API.

use crate::tercen::{tson_to_dataframe, FacetInfo, TableStreamer, TercenClient};
use ggrs_core::{
    aes::Aes,
    data::DataFrame,
    legend::{ColorStop as LegendColorStop, LegendScale},
    stream::{AxisData, CategoricalAxisData, FacetSpec, NumericAxisData, Range, StreamGenerator},
};
use polars::prelude::IntoColumn;
use std::collections::HashMap;
use std::sync::Arc;

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

    /// Color information (factors and palettes)
    color_infos: Vec<crate::tercen::ColorInfo>,

    /// Cached legend scale (loaded during initialization)
    cached_legend_scale: LegendScale,

    /// Page factor names (kept for metadata/debugging)
    /// Not used for filtering - GGRS handles everything via original_index
    #[allow(dead_code)]
    page_factors: Vec<String>,

    /// Heatmap mode: when set, overrides facet counts to 1x1 and uses grid dimensions for axes
    /// Tuple is (n_columns, n_rows) representing the heatmap grid dimensions
    heatmap_mode: Option<(usize, usize)>,
}

impl TercenStreamGenerator {
    /// Create a new stream generator with explicit table IDs
    ///
    /// This loads facet metadata and axis ranges from pre-computed tables.
    /// Note: page_filter is used to load only the facets for this page (e.g., female or male),
    /// but data is NOT filtered - GGRS handles data matching via original_index
    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        client: Arc<TercenClient>,
        main_table_id: String,
        col_facet_table_id: String,
        row_facet_table_id: String,
        y_axis_table_id: Option<String>,
        chunk_size: usize,
        color_infos: Vec<crate::tercen::ColorInfo>,
        page_factors: Vec<String>,
        page_filter: Option<&HashMap<String, String>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
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

        // Load axis ranges from pre-computed Y-axis table (if available)
        let (axis_ranges, total_rows) = if let Some(ref y_table_id) = y_axis_table_id {
            println!("Loading axis ranges from Y-axis table: {}", y_table_id);
            Self::load_axis_ranges_from_table(&client, y_table_id, &main_table_id, &facet_info)
                .await?
        } else {
            println!("No Y-axis table provided, falling back to data scanning");
            Self::compute_axis_ranges(&client, &main_table_id, &facet_info, chunk_size).await?
        };

        eprintln!(
            "DEBUG: axis_ranges has {} entries (before remapping), total_rows: {}",
            axis_ranges.len(),
            total_rows
        );

        // NOTE: axis_ranges now keyed by original_index (not filtered index)
        // load_axis_ranges_from_table() already maps table's .ri (0-11) → original_index (12-23)
        // This ensures data[.ri=12] can look up y_ranges[12] correctly
        eprintln!("DEBUG: axis_ranges keyed by original_index for data matching");

        eprintln!(
            "DEBUG: TercenStreamGenerator initialized with total_rows = {}",
            total_rows
        );

        // Load legend scale data
        println!("Loading legend scale data...");
        let cached_legend_scale = Self::load_legend_scale(&client, &color_infos).await?;
        eprintln!("DEBUG: Cached legend scale: {:?}", cached_legend_scale);

        // Create default aesthetics
        // Dequantization happens in GGRS render.rs using axis ranges
        // After dequantization, columns are .x and .y (actual data values)
        // Add color aesthetic if colors are defined
        let mut aes = Aes::new().x(".x").y(".y");
        eprintln!("DEBUG: color_infos.len() = {}", color_infos.len());
        if !color_infos.is_empty() {
            eprintln!("DEBUG: Adding .color aesthetic to Aes");
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
            aes = aes.color(".color");
        } else {
            eprintln!("DEBUG: No color_infos, NOT adding .color aesthetic");
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
            cached_legend_scale,
            page_factors,
            heatmap_mode: None,
        })
    }

    /// Create a new stream generator from workflow and step IDs
    ///
    /// This is the primary constructor used by the operator.
    pub async fn from_workflow_step(
        _client: Arc<TercenClient>,
        _workflow_id: &str,
        _step_id: &str,
        _chunk_size: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Implementation would go here - loading workflow/step data
        // For now, placeholder error
        Err("from_workflow_step not yet implemented".into())
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
            cached_legend_scale: LegendScale::None, // TODO: Load async if needed
            page_factors,
            heatmap_mode: None,
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

    /// Load axis ranges from pre-computed Y-axis table
    ///
    /// The Y-axis table contains columns: .ri, .minY, .maxY (and optionally .ci)
    /// There should be one row per facet cell (indexed by .ci and .ri)
    async fn load_axis_ranges_from_table(
        client: &TercenClient,
        y_axis_table_id: &str,
        main_table_id: &str,
        facet_info: &FacetInfo,
    ) -> Result<
        (
            HashMap<(usize, usize), (AxisData, AxisData)>,
            usize, // total rows across all facets
        ),
        Box<dyn std::error::Error>,
    > {
        let streamer = TableStreamer::new(client);

        // First, get the schema to see which columns exist
        println!("  Fetching Y-axis table schema...");
        let schema = streamer.get_schema(y_axis_table_id).await?;
        let column_names = extract_column_names_from_schema(&schema)?;
        println!("  Y-axis table columns: {:?}", column_names);

        // Build column list: always need .ri, .minY, .maxY
        // Optionally include .ci if it exists (for column facets)
        let mut columns_to_fetch =
            vec![".ri".to_string(), ".minY".to_string(), ".maxY".to_string()];
        let has_ci = column_names.contains(&".ci".to_string());
        if has_ci {
            columns_to_fetch.push(".ci".to_string());
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
        let total_rows = extract_row_count_from_schema(&main_schema)? as usize;
        println!("  Total rows: {}", total_rows);

        let mut axis_ranges = HashMap::new();
        let has_ci = df.columns().contains(&".ci".to_string());

        // Process each row in Y-axis table
        for i in 0..df.nrow() {
            let col_idx = if has_ci {
                match df.get_value(i, ".ci")? {
                    ggrs_core::data::Value::Int(v) => v as usize,
                    _ => 0,
                }
            } else {
                0
            };

            let row_idx_from_table = match df.get_value(i, ".ri")? {
                ggrs_core::data::Value::Int(v) => v as usize,
                _ => return Err(format!("Invalid .ri at row {}", i).into()),
            };

            // Map table's .ri (filtered index 0-11) to original index (12-23 for page 2)
            // This is necessary because the Y-axis table from Tercen is pre-filtered by page
            let row_idx = facet_info
                .row_facets
                .groups
                .get(row_idx_from_table)
                .map(|g| g.original_index)
                .unwrap_or(row_idx_from_table);

            let min_y = match df.get_value(i, ".minY")? {
                ggrs_core::data::Value::Float(v) => v,
                _ => return Err(format!("Invalid .minY at row {}", i).into()),
            };

            let max_y = match df.get_value(i, ".maxY")? {
                ggrs_core::data::Value::Float(v) => v,
                _ => return Err(format!("Invalid .maxY at row {}", i).into()),
            };

            println!(
                "  Facet (table .ri={}) -> original_index={}: Y [{}, {}]",
                row_idx_from_table, row_idx, min_y, max_y
            );

            // X-axis is quantized coordinate space (0-65535)
            let x_axis = AxisData::Numeric(NumericAxisData {
                min_value: 0.0,
                max_value: 65535.0,
                min_axis: 0.0,
                max_axis: 65535.0,
                transform: None,
            });

            let y_axis = AxisData::Numeric(NumericAxisData {
                min_value: min_y,
                max_value: max_y,
                min_axis: min_y,
                max_axis: max_y,
                transform: None,
            });

            // If there's no .ci column, the Y range applies to all columns in this row
            if has_ci {
                axis_ranges.insert((col_idx, row_idx), (x_axis.clone(), y_axis.clone()));
            } else {
                // Replicate the same Y range for all column facets
                for col in 0..facet_info.n_col_facets() {
                    axis_ranges.insert((col, row_idx), (x_axis.clone(), y_axis.clone()));
                }
            }
        }

        println!("  Loaded {} axis ranges", axis_ranges.len());
        Ok((axis_ranges, total_rows))
    }
    /// Compute axis ranges by scanning the main data table
    ///
    /// This is the fallback when no pre-computed Y-axis table is available.
    async fn compute_axis_ranges(
        client: &TercenClient,
        table_id: &str,
        facet_info: &FacetInfo,
        chunk_size: usize,
    ) -> Result<(HashMap<(usize, usize), (AxisData, AxisData)>, usize), Box<dyn std::error::Error>>
    {
        println!("Computing axis ranges...");

        let streamer = TableStreamer::new(client);
        let schema = streamer.get_schema(table_id).await?;
        let total_rows = extract_row_count_from_schema(&schema)?;
        println!("  Total rows: {}", total_rows);

        let chunk_size = chunk_size as i64;
        let mut offset = 0;

        // Track min/max for each facet cell
        let mut cell_stats: HashMap<(usize, usize), (f64, f64, f64, f64)> = HashMap::new();

        // Initialize all cells
        for col_idx in 0..facet_info.n_col_facets() {
            for row_idx in 0..facet_info.n_row_facets() {
                cell_stats.insert(
                    (col_idx, row_idx),
                    (
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                    ),
                );
            }
        }

        let columns = Some(vec![".ci".to_string(), ".ri".to_string(), ".y".to_string()]);

        // Stream through data
        loop {
            let remaining = total_rows - offset;
            if remaining <= 0 {
                break;
            }
            let limit = remaining.min(chunk_size);

            let tson_data = streamer
                .stream_tson(table_id, columns.clone(), offset, limit)
                .await?;

            if tson_data.is_empty() {
                break;
            }

            let df = tson_to_dataframe(&tson_data)?;
            let row_count = df.nrow();

            // Update min/max for each row
            for i in 0..row_count {
                let col_idx = df
                    .get_value(i, ".ci")
                    .ok()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as usize;

                let row_idx = df
                    .get_value(i, ".ri")
                    .ok()
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as usize;

                let y = df
                    .get_value(i, ".y")
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                let stats = cell_stats.entry((col_idx, row_idx)).or_insert((
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                ));

                stats.2 = stats.2.min(y);
                stats.3 = stats.3.max(y);
            }

            offset += row_count as i64;

            if offset % (chunk_size * 5) < chunk_size {
                println!("    Progress: {}/{}", offset.min(total_rows), total_rows);
            }

            if row_count == 0 {
                break;
            }
        }

        // Convert to axis ranges
        let mut axis_ranges = HashMap::new();
        for ((col_idx, row_idx), (_min_x, _max_x, min_y, max_y)) in cell_stats {
            let x_axis = AxisData::Numeric(NumericAxisData {
                min_value: 0.0,
                max_value: 65535.0,
                min_axis: 0.0,
                max_axis: 65535.0,
                transform: None,
            });

            let y_axis = AxisData::Numeric(NumericAxisData {
                min_value: min_y,
                max_value: max_y,
                min_axis: min_y,
                max_axis: max_y,
                transform: None,
            });

            axis_ranges.insert((col_idx, row_idx), (x_axis, y_axis));
        }

        println!("  Computed {} axis ranges", axis_ranges.len());
        Ok((axis_ranges, total_rows as usize))
    }

    /// Create a generic legend for level-based colors
    ///
    /// When we can't get actual category names, use generic labels: "Level 0", "Level 1", etc.
    /// Assumes 8 levels (the default tercen palette size)
    fn create_generic_level_legend(
        factor_name: &str,
    ) -> Result<LegendScale, Box<dyn std::error::Error>> {
        let categories: Vec<String> = (0..8).map(|i| format!("Level {}", i)).collect();
        Ok(LegendScale::Discrete {
            values: categories,
            aesthetic_name: factor_name.to_string(),
        })
    }

    /// Load legend scale data during initialization
    ///
    /// For categorical colors with .colorLevels, this streams the color table
    /// to extract unique category names.
    /// For continuous colors, this extracts the min/max from the palette.
    async fn load_legend_scale(
        client: &TercenClient,
        color_infos: &[crate::tercen::ColorInfo],
    ) -> Result<LegendScale, Box<dyn std::error::Error>> {
        if color_infos.is_empty() {
            return Ok(LegendScale::None);
        }

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
                    let mut values: Vec<String> = color_map.mappings.keys().cloned().collect();
                    values.sort();

                    Ok(LegendScale::Discrete {
                        values,
                        aesthetic_name: color_info.factor_name.clone(),
                    })
                } else if let Some(ref color_table_id) = color_info.color_table_id {
                    // Level-based categorical colors (.colorLevels)
                    // Try to stream the color table to get unique category names
                    eprintln!(
                        "DEBUG: Loading category names from color table {}",
                        color_table_id
                    );

                    let streamer = TableStreamer::new(client);

                    // Stream the entire color table (it's usually small)
                    // The table contains the mapping from level index to category name
                    let tson_data = streamer
                        .stream_tson(color_table_id, None, 0, 100000)
                        .await?;

                    let df = tson_to_dataframe(&tson_data)?;
                    eprintln!(
                        "DEBUG: Color table has {} rows, columns: {:?}",
                        df.nrow(),
                        df.columns()
                    );

                    // Check if we got any data from the color table
                    if df.nrow() > 0 {
                        // Extract unique category names from the factor column
                        // The color table should have a column matching the factor name
                        if let Ok(column) = df.column(&color_info.factor_name) {
                            // Collect unique values and convert to strings
                            use std::collections::HashSet;
                            let mut seen = HashSet::new();
                            let mut categories: Vec<String> = Vec::new();

                            for val in column {
                                let s = val.to_string();
                                if seen.insert(s.clone()) {
                                    categories.push(s);
                                }
                            }

                            categories.sort();
                            eprintln!(
                                "DEBUG: Extracted {} unique categories: {:?}",
                                categories.len(),
                                categories
                            );

                            Ok(LegendScale::Discrete {
                                values: categories,
                                aesthetic_name: color_info.factor_name.clone(),
                            })
                        } else {
                            eprintln!(
                                "DEBUG: Color table does not have column '{}'",
                                color_info.factor_name
                            );
                            // Fall back to generic level labels
                            Self::create_generic_level_legend(&color_info.factor_name)
                        }
                    } else {
                        // Color table is empty - fall back to generic level labels
                        eprintln!("DEBUG: Color table is empty, using generic level labels");
                        Self::create_generic_level_legend(&color_info.factor_name)
                    }
                } else {
                    // No explicit mappings and no color table - use generic level labels
                    eprintln!(
                        "DEBUG: No explicit mappings or color table, using generic level labels"
                    );
                    Self::create_generic_level_legend(&color_info.factor_name)
                }
            }
        }
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

        let streamer = TableStreamer::new(&self.client);

        // For bulk streaming, ALWAYS include facet indices
        let mut columns = vec![
            ".ci".to_string(),
            ".ri".to_string(),
            ".xs".to_string(),
            ".ys".to_string(),
        ];

        // For heatmaps, include level columns for proper grid positioning
        // .xLevels = column index in heatmap grid
        // .nXLevels = total number of columns
        columns.push(".xLevels".to_string());
        columns.push(".nXLevels".to_string());

        // NOTE: Don't add page_factors to columns!
        // Page factors exist in facet tables, not the main data table.
        // We've already filtered facets by page, so data filtering is via .ri matching.

        // Add color columns
        // For categorical colors, we need .colorLevels (int32) not the factor column
        // For continuous colors, we need the factor column (f64)
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

        // Map color values to RGB if color factors are defined
        if !self.color_infos.is_empty() {
            eprintln!(
                "DEBUG: Adding color columns for {} color factors",
                self.color_infos.len()
            );
            df = self.add_color_columns(df)?;
            eprintln!("DEBUG: Color columns added successfully");
        }

        Ok(df)
    }

    /// Add RGB color columns to DataFrame based on color factors
    ///
    /// For each color factor, interpolates values using the palette and adds
    /// three new columns: `.color_r`, `.color_g`, `.color_b` (u8 values 0-255)
    ///
    /// Currently supports single color factor (first in color_infos).
    /// Multiple color factors would require a strategy (e.g., blend, choose first, etc.)
    fn add_color_columns(
        &self,
        df: ggrs_core::data::DataFrame,
    ) -> Result<ggrs_core::data::DataFrame, Box<dyn std::error::Error>> {
        use polars::prelude::*;

        // For now, only use the first color factor
        // TODO: Handle multiple color factors (blend? choose first? user option?)
        let color_info = &self.color_infos[0];

        let mut polars_df = df.inner().clone();

        // Generate RGB values based on mapping type
        let nrows = polars_df.height();
        let mut r_values = Vec::with_capacity(nrows);
        let mut g_values = Vec::with_capacity(nrows);
        let mut b_values = Vec::with_capacity(nrows);

        match &color_info.mapping {
            crate::tercen::ColorMapping::Continuous(palette) => {
                let color_col_name = &color_info.factor_name;
                eprintln!(
                    "DEBUG add_color_columns: Using continuous color mapping for '{}'",
                    color_col_name
                );

                // Get the color factor column
                let color_series = polars_df
                    .column(color_col_name)
                    .map_err(|e| format!("Color column '{}' not found: {}", color_col_name, e))?;

                // Extract f64 values
                let color_values = color_series.f64().map_err(|e| {
                    format!(
                        "Color column '{}' is not f64 for continuous mapping: {}",
                        color_col_name, e
                    )
                })?;

                // Debug: Print first few color factor values to verify we're getting expected data
                let sample_values: Vec<f64> =
                    color_values.iter().take(5).flatten().collect();
                if !sample_values.is_empty() {
                    let min_val = color_values.min().unwrap_or(0.0);
                    let max_val = color_values.max().unwrap_or(0.0);
                    eprintln!(
                        "DEBUG add_color_columns: {} values range [{:.2}, {:.2}], first 5: {:?}",
                        color_col_name, min_val, max_val, sample_values
                    );
                }

                // Map each value to RGB using palette interpolation
                for opt_value in color_values.iter() {
                    if let Some(value) = opt_value {
                        let rgb = crate::tercen::interpolate_color(value, palette);
                        r_values.push(rgb[0]);
                        g_values.push(rgb[1]);
                        b_values.push(rgb[2]);
                    } else {
                        // Handle null values with a default color (gray)
                        r_values.push(128);
                        g_values.push(128);
                        b_values.push(128);
                    }
                }
            }

            crate::tercen::ColorMapping::Categorical(color_map) => {
                eprintln!("DEBUG add_color_columns: Using categorical color mapping");
                eprintln!(
                    "DEBUG add_color_columns: Category map has {} entries",
                    color_map.mappings.len()
                );

                // For categorical colors, Tercen uses .colorLevels column (int32) with level indices
                // If color_map has explicit mappings, use them; otherwise generate from levels
                let use_levels = color_map.mappings.is_empty();

                if use_levels {
                    eprintln!(
                        "DEBUG add_color_columns: Using .colorLevels column for categorical colors"
                    );

                    // Get .colorLevels column instead of the factor column
                    let levels_series = polars_df.column(".colorLevels").map_err(|e| {
                        format!("Categorical colors require .colorLevels column: {}", e)
                    })?;

                    // Schema says int32 but it comes back as i64, so accept both
                    let levels = levels_series
                        .i64()
                        .map_err(|e| format!(".colorLevels column is not i64: {}", e))?;

                    // Map each level to RGB using default categorical palette
                    for opt_level in levels.iter() {
                        if let Some(level) = opt_level {
                            let rgb = crate::tercen::categorical_color_from_level(level as i32);
                            r_values.push(rgb[0]);
                            g_values.push(rgb[1]);
                            b_values.push(rgb[2]);
                        } else {
                            // Handle null values with a default color (gray)
                            r_values.push(128);
                            g_values.push(128);
                            b_values.push(128);
                        }
                    }
                } else {
                    // Use explicit category→color mappings from palette
                    let color_col_name = &color_info.factor_name;
                    eprintln!(
                        "DEBUG add_color_columns: Using explicit category mappings for '{}'",
                        color_col_name
                    );

                    // Get the color factor column
                    let color_series = polars_df.column(color_col_name).map_err(|e| {
                        format!("Color column '{}' not found: {}", color_col_name, e)
                    })?;

                    let color_values = color_series.str().map_err(|e| {
                        format!(
                            "Color column '{}' is not string for categorical mapping: {}",
                            color_col_name, e
                        )
                    })?;

                    for opt_value in color_values.iter() {
                        if let Some(category) = opt_value {
                            let rgb = color_map
                                .mappings
                                .get(category)
                                .unwrap_or(&color_map.default_color);
                            r_values.push(rgb[0]);
                            g_values.push(rgb[1]);
                            b_values.push(rgb[2]);
                        } else {
                            r_values.push(128);
                            g_values.push(128);
                            b_values.push(128);
                        }
                    }
                }
            }
        }

        // Convert RGB values to hex color strings for GGRS
        let color_hex_strings: Vec<String> = (0..r_values.len())
            .map(|i| format!("#{:02X}{:02X}{:02X}", r_values[i], g_values[i], b_values[i]))
            .collect();

        // Add color column as hex strings
        polars_df.with_column(Series::new(".color".into(), color_hex_strings))?;

        // Debug: Print first color values
        if polars_df.height() > 0 {
            if let Ok(color_col) = polars_df.column(".color") {
                let str_col = color_col.str().unwrap();
                let first_colors: Vec<&str> = str_col
                    .into_iter()
                    .take(3)
                    .map(|opt| opt.unwrap_or("NULL"))
                    .collect();
                eprintln!("DEBUG: First 3 .color hex values: {:?}", first_colors);
            }
        }

        Ok(ggrs_core::data::DataFrame::from_polars(polars_df))
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
            eprintln!("DEBUG PHASE 2: n_col_facets() returning 1 (heatmap mode)");
            return 1;
        }
        let count = self.facet_info.n_col_facets();
        eprintln!("DEBUG PHASE 2: n_col_facets() returning {}", count);
        count
    }

    fn n_row_facets(&self) -> usize {
        // In heatmap mode, we have a single panel (1x1 facets)
        if self.heatmap_mode.is_some() {
            eprintln!("DEBUG PHASE 2: n_row_facets() returning 1 (heatmap mode)");
            return 1;
        }
        let count = self.facet_info.n_row_facets();
        eprintln!("DEBUG PHASE 2: n_row_facets() returning {}", count);
        count
    }

    fn n_total_data_rows(&self) -> usize {
        // Return total row count across ALL facets
        eprintln!(
            "DEBUG PHASE 2: n_total_data_rows() returning {}",
            self.total_rows
        );
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

        eprintln!(
            "DEBUG PHASE 2: query_col_facet_labels() returning {} labels: {:?}",
            labels.len(),
            labels.iter().take(3).collect::<Vec<_>>()
        );

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
            .unwrap_or_else(|_| polars::frame::DataFrame::empty());

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

        eprintln!(
            "DEBUG PHASE 2: query_row_facet_labels() returning {} labels",
            labels.len()
        );
        if !labels.is_empty() {
            eprintln!(
                "  First 3 labels: {:?}",
                labels.iter().take(3).collect::<Vec<_>>()
            );
        }

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
            .unwrap_or_else(|_| polars::frame::DataFrame::empty());

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
                eprintln!(
                    "DEBUG PHASE 2: query_x_axis({}, {}) returning categorical with {} labels",
                    col_idx,
                    row_idx,
                    categories.len()
                );
                return AxisData::Categorical(CategoricalAxisData { categories });
            }

            // Fallback: return numeric if labels don't match grid size
            eprintln!(
                "DEBUG PHASE 2: query_x_axis({}, {}) returning heatmap range [-0.5, {}]",
                col_idx,
                row_idx,
                n_cols as f64 - 0.5
            );
            return AxisData::Numeric(NumericAxisData {
                min_value: -0.5,
                max_value: n_cols as f64 - 0.5,
                min_axis: -0.5,
                max_axis: n_cols as f64 - 0.5,
                transform: None,
            });
        }

        self.axis_ranges
            .get(&(col_idx, row_idx))
            .map(|(x_axis, _)| x_axis.clone())
            .unwrap_or_else(|| {
                AxisData::Numeric(NumericAxisData {
                    min_value: 0.0,
                    max_value: 1.0,
                    min_axis: 0.0,
                    max_axis: 1.0,
                    transform: None,
                })
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
                eprintln!(
                    "DEBUG PHASE 2: query_y_axis({}, {}) returning categorical with {} labels",
                    col_idx,
                    row_idx,
                    categories.len()
                );
                return AxisData::Categorical(CategoricalAxisData { categories });
            }

            // Fallback: return numeric if labels don't match grid size
            eprintln!(
                "DEBUG PHASE 2: query_y_axis({}, {}) returning heatmap range [-0.5, {}]",
                col_idx,
                row_idx,
                n_rows as f64 - 0.5
            );
            return AxisData::Numeric(NumericAxisData {
                min_value: -0.5,
                max_value: n_rows as f64 - 0.5,
                min_axis: -0.5,
                max_axis: n_rows as f64 - 0.5,
                transform: None,
            });
        }

        let axis = self
            .axis_ranges
            .get(&(col_idx, row_idx))
            .map(|(_, y_axis)| y_axis.clone())
            .unwrap_or_else(|| {
                AxisData::Numeric(NumericAxisData {
                    min_value: 0.0,
                    max_value: 1.0,
                    min_axis: 0.0,
                    max_axis: 1.0,
                    transform: None,
                })
            });

        // Log first call for each facet
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            if let AxisData::Numeric(ref data) = axis {
                eprintln!(
                    "DEBUG PHASE 2: query_y_axis({}, {}) called",
                    col_idx, row_idx
                );
                eprintln!(
                    "  Returning Y range: [{}, {}]",
                    data.min_value, data.max_value
                );
            }
        }

        axis
    }

    fn query_legend_scale(&self) -> LegendScale {
        // Return cached legend scale (loaded during initialization)
        self.cached_legend_scale.clone()
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
        // Block on async within sync trait method
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.stream_bulk_data(data_range).await })
        })
        .unwrap_or_else(|e| {
            eprintln!("Error querying bulk data: {}", e);
            DataFrame::new()
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
            .unwrap_or(col_idx) // Fallback to col_idx if not found (shouldn't happen)
    }

    fn get_original_row_idx(&self, row_idx: usize) -> usize {
        // Look up the FacetGroup at row_idx and return its original_index
        // For pagination: row_idx is grid position (0-11), original_index is data .ri value (12-23 for male)
        self.facet_info
            .row_facets
            .groups
            .get(row_idx)
            .map(|group| group.original_index)
            .unwrap_or(row_idx) // Fallback to row_idx if not found (shouldn't happen)
    }
}
