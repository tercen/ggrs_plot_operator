//! Tercen Stream Generator - Bridges Tercen data with GGRS plotting
//!
//! This module implements the GGRS `StreamGenerator` trait for Tercen,
//! enabling lazy loading of data directly from Tercen's gRPC API.

use crate::tercen::{tson_to_dataframe, FacetInfo, TableStreamer, TercenClient};
use ggrs_core::{
    aes::Aes,
    data::DataFrame,
    legend::LegendScale,
    stream::{AxisData, FacetSpec, NumericAxisData, Range, StreamGenerator},
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
/// Streams quantized coordinates (.xs, .ys) from Tercen.
/// GGRS handles dequantization using axis ranges.
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
}

impl TercenStreamGenerator {
    /// Create a new stream generator with explicit table IDs
    ///
    /// This loads facet metadata and axis ranges from pre-computed tables.
    #[allow(dead_code)]
    pub async fn new(
        client: Arc<TercenClient>,
        main_table_id: String,
        col_facet_table_id: String,
        row_facet_table_id: String,
        y_axis_table_id: Option<String>,
        chunk_size: usize,
        color_infos: Vec<crate::tercen::ColorInfo>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Load facet metadata
        let facet_info = FacetInfo::load(&client, &col_facet_table_id, &row_facet_table_id).await?;

        println!(
            "Loaded facets: {} columns × {} rows = {} cells",
            facet_info.n_col_facets(),
            facet_info.n_row_facets(),
            facet_info.total_facets()
        );

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
            "DEBUG: TercenStreamGenerator initialized with total_rows = {}",
            total_rows
        );

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
                    eprintln!("DEBUG: Continuous palette with {} color stops", palette.stops.len());
                    for (i, stop) in palette.stops.iter().enumerate() {
                        eprintln!(
                            "  Stop {}: value={:.2}, color=RGB({}, {}, {})",
                            i, stop.value, stop.color[0], stop.color[1], stop.color[2]
                        );
                    }
                }
                crate::tercen::ColorMapping::Categorical(color_map) => {
                    eprintln!("DEBUG: Categorical palette with {} categories", color_map.mappings.len());
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
    pub fn new_with_ranges(
        client: Arc<TercenClient>,
        main_table_id: String,
        facet_info: FacetInfo,
        axis_ranges: HashMap<(usize, usize), (AxisData, AxisData)>,
        total_rows: usize,
        chunk_size: usize,
        color_infos: Vec<crate::tercen::ColorInfo>,
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

            let row_idx = match df.get_value(i, ".ri")? {
                ggrs_core::data::Value::Int(v) => v as usize,
                _ => return Err(format!("Invalid .ri at row {}", i).into()),
            };

            let min_y = match df.get_value(i, ".minY")? {
                ggrs_core::data::Value::Float(v) => v,
                _ => return Err(format!("Invalid .minY at row {}", i).into()),
            };

            let max_y = match df.get_value(i, ".maxY")? {
                ggrs_core::data::Value::Float(v) => v,
                _ => return Err(format!("Invalid .maxY at row {}", i).into()),
            };

            println!(
                "  Facet ({}, {}): Y [{}, {}]",
                col_idx, row_idx, min_y, max_y
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

    // Stream data for a specific facet cell in chunks
    // NOTE: Per-facet streaming not used - commented out since GGRS uses bulk mode
    // async fn stream_facet_data(
    //     &self,
    //     col_idx: usize,
    //     row_idx: usize,
    //     data_range: Range,
    // ) -> Result<DataFrame, Box<dyn std::error::Error>> {
    //     let streamer = TableStreamer::new(&self.client);
    //
    //     // Stream quantized coordinates from Tercen
    //     // For single facet: Only need .xs, .ys (4 bytes per row)
    //     // For multiple facets: Need .ci, .ri for filtering (8 bytes per row)
    //     let columns = if self.facet_info.total_facets() == 1 {
    //         vec![".xs".to_string(), ".ys".to_string()]
    //     } else {
    //         vec![
    //             ".ci".to_string(),
    //             ".ri".to_string(),
    //             ".xs".to_string(),
    //             ".ys".to_string(),
    //         ]
    //     };
    //
    //     // Use configured chunk_size for all requests to Tercen
    //     let chunk_size = self.chunk_size as i64;
    //     let mut all_chunks: Vec<polars::frame::DataFrame> = Vec::new();
    //     let mut current_offset = data_range.start as i64;
    //     let end_offset = data_range.end as i64;
    //
    //     while current_offset < end_offset {
    //         let remaining = end_offset - current_offset;
    //         let limit = remaining.min(chunk_size);
    //
    //         let tson_data = streamer
    //             .stream_tson(
    //                 &self.main_table_id,
    //                 Some(columns.clone()),
    //                 current_offset,
    //                 limit,
    //             )
    //             .await?;
    //
    //         if tson_data.is_empty() {
    //             break;
    //         }
    //
    //         // Parse TSON to DataFrame
    //         let df = tson_to_dataframe(&tson_data)?;
    //         let fetched_rows = df.nrow();
    //
    //         // Filter by facet indices if needed
    //         let filtered_df = self.filter_by_facet(df, col_idx, row_idx)?;
    //
    //         // NOTE: Dequantization now happens in GGRS, not here in the operator
    //         // The data is returned with quantized coordinates (.xs, .ys) and will be
    //         // dequantized by GGRS based on axis ranges right before rendering
    //
    //         // Accumulate the filtered Polars DataFrame (still has .xs/.ys)
    //         if filtered_df.nrow() > 0 {
    //             all_chunks.push(filtered_df.inner().clone());
    //         }
    //
    //         current_offset += fetched_rows as i64;
    //
    //         // Safety check: if we didn't get any rows from this chunk, stop
    //         if fetched_rows == 0 {
    //             break;
    //         }
    //     }
    //
    //     // Concatenate all chunks vertically using Polars vstack
    //     if all_chunks.is_empty() {
    //         return Ok(ggrs_core::data::DataFrame::new());
    //     }
    //
    //     let mut result_df = all_chunks[0].clone();
    //     for chunk in all_chunks.iter().skip(1) {
    //         result_df
    //             .vstack_mut(chunk)
    //             .map_err(|e| format!("Failed to vstack chunks: {}", e))?;
    //     }
    //
    //     Ok(ggrs_core::data::DataFrame::from_polars(result_df))
    // }

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
                    eprintln!("DEBUG add_color_columns: Using .colorLevels column for categorical colors");

                    // Get .colorLevels column instead of the factor column
                    let levels_series = polars_df
                        .column(".colorLevels")
                        .map_err(|e| format!("Categorical colors require .colorLevels column: {}", e))?;

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
                    eprintln!("DEBUG add_color_columns: Using explicit category mappings for '{}'", color_col_name);

                    // Get the color factor column
                    let color_series = polars_df
                        .column(color_col_name)
                        .map_err(|e| format!("Color column '{}' not found: {}", color_col_name, e))?;

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
    // fn filter_by_facet(
    //     &self,
    //     df: DataFrame,
    //     col_idx: usize,
    //     row_idx: usize,
    // ) -> Result<DataFrame, Box<dyn std::error::Error>> {
    //     use polars::prelude::lit;
    //
    //     let single_facet = self.facet_info.total_facets() == 1;
    //     let mut polars_df = df.inner().clone();
    //
    //     // Filter by facet indices if multiple facets
    //     if !single_facet {
    //         // Filter: .ci == col_idx AND .ri == row_idx
    //         polars_df = polars_df
    //             .lazy()
    //             .filter(
    //                 col(".ci")
    //                     .eq(lit(col_idx as i64))
    //                     .and(col(".ri").eq(lit(row_idx as i64))),
    //             )
    //             .collect()
    //             .map_err(|e| format!("Failed to filter by facet: {}", e))?;
    //
    //         // Drop .ci and .ri columns after filtering (renderers don't need them)
    //         let col_names_to_drop: Vec<String> = vec![".ci".to_string(), ".ri".to_string()];
    //         polars_df = polars_df.drop_many(col_names_to_drop);
    //     }
    //
    //     // Return DataFrame with .xs, .ys (will be dequantized to .x, .y in caller)
    //     Ok(ggrs_core::data::DataFrame::from_polars(polars_df))
    // }
}

impl StreamGenerator for TercenStreamGenerator {
    fn n_col_facets(&self) -> usize {
        let count = self.facet_info.n_col_facets();
        eprintln!("DEBUG PHASE 2: n_col_facets() returning {}", count);
        count
    }

    fn n_row_facets(&self) -> usize {
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
        // For now, return empty legend
        // TODO: Implement legend based on color aesthetics
        LegendScale::None
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
}
