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
use polars::prelude::{col, IntoColumn, IntoLazy};
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
fn extract_column_names_from_schema(
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

        // Create default aesthetics
        // Dequantization happens in GGRS render.rs using axis ranges
        // After dequantization, columns are .x and .y (actual data values)
        let aes = Aes::new().x(".x").y(".y");

        // Create facet spec based on facet metadata
        let facet_spec = if facet_info.has_faceting() {
            // For now, create a simple grid spec
            // TODO: Map from Tercen crosstab spec to proper FacetSpec
            FacetSpec::none()
        } else {
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
    ) -> Self {
        // Aesthetics use dequantized coordinates: .x and .y (actual data values)
        // Dequantization happens in stream_facet_data() before data reaches renderers
        let aes = Aes::new().x(".x").y(".y");

        // Create facet spec based on facet metadata
        let facet_spec = if facet_info.has_faceting() {
            // TODO: Map from Tercen crosstab spec to proper FacetSpec
            FacetSpec::none()
        } else {
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
        let expected_rows = facet_info.n_col_facets() * facet_info.n_row_facets();
        println!(
            "  Fetching Y-axis ranges (expecting {} rows)...",
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

            axis_ranges.insert((col_idx, row_idx), (x_axis, y_axis));
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

    /// Stream data for a specific facet cell in chunks
    async fn stream_facet_data(
        &self,
        col_idx: usize,
        row_idx: usize,
        data_range: Range,
    ) -> Result<DataFrame, Box<dyn std::error::Error>> {
        let streamer = TableStreamer::new(&self.client);

        // Stream quantized coordinates from Tercen
        // For single facet: Only need .xs, .ys (4 bytes per row)
        // For multiple facets: Need .ci, .ri for filtering (8 bytes per row)
        let columns = if self.facet_info.total_facets() == 1 {
            vec![".xs".to_string(), ".ys".to_string()]
        } else {
            vec![
                ".ci".to_string(),
                ".ri".to_string(),
                ".xs".to_string(),
                ".ys".to_string(),
            ]
        };

        // Use configured chunk_size for all requests to Tercen
        let chunk_size = self.chunk_size as i64;
        let mut all_chunks: Vec<polars::frame::DataFrame> = Vec::new();
        let mut current_offset = data_range.start as i64;
        let end_offset = data_range.end as i64;

        while current_offset < end_offset {
            let remaining = end_offset - current_offset;
            let limit = remaining.min(chunk_size);

            let tson_data = streamer
                .stream_tson(
                    &self.main_table_id,
                    Some(columns.clone()),
                    current_offset,
                    limit,
                )
                .await?;

            if tson_data.is_empty() {
                break;
            }

            // Parse TSON to DataFrame
            let df = tson_to_dataframe(&tson_data)?;
            let fetched_rows = df.nrow();

            // Filter by facet indices if needed
            let filtered_df = self.filter_by_facet(df, col_idx, row_idx)?;

            // NOTE: Dequantization now happens in GGRS, not here in the operator
            // The data is returned with quantized coordinates (.xs, .ys) and will be
            // dequantized by GGRS based on axis ranges right before rendering

            // Accumulate the filtered Polars DataFrame (still has .xs/.ys)
            if filtered_df.nrow() > 0 {
                all_chunks.push(filtered_df.inner().clone());
            }

            current_offset += fetched_rows as i64;

            // Safety check: if we didn't get any rows from this chunk, stop
            if fetched_rows == 0 {
                break;
            }
        }

        // Concatenate all chunks vertically using Polars vstack
        if all_chunks.is_empty() {
            return Ok(ggrs_core::data::DataFrame::new());
        }

        let mut result_df = all_chunks[0].clone();
        for chunk in all_chunks.iter().skip(1) {
            result_df
                .vstack_mut(chunk)
                .map_err(|e| format!("Failed to vstack chunks: {}", e))?;
        }

        Ok(ggrs_core::data::DataFrame::from_polars(result_df))
    }

    /// Stream data in bulk across ALL facets (includes .ci and .ri columns)
    async fn stream_bulk_data(
        &self,
        data_range: Range,
    ) -> Result<DataFrame, Box<dyn std::error::Error>> {
        let streamer = TableStreamer::new(&self.client);

        // For bulk streaming, ALWAYS include facet indices
        let columns = vec![
            ".ci".to_string(),
            ".ri".to_string(),
            ".xs".to_string(),
            ".ys".to_string(),
        ];

        // Fetch the requested range directly (GGRS handles chunking)
        let offset = data_range.start as i64;
        let limit = (data_range.end - data_range.start) as i64;

        let tson_data = streamer
            .stream_tson(&self.main_table_id, Some(columns), offset, limit)
            .await?;

        if tson_data.is_empty() {
            return Ok(ggrs_core::data::DataFrame::new());
        }

        // Parse TSON to DataFrame - contains .ci, .ri, .xs, .ys
        let df = tson_to_dataframe(&tson_data)?;

        Ok(df)
    }

    /// Dequantize coordinates: .xs/.ys (uint16 0-65535) → .x/.y (actual data values)
    ///
    /// This transformation is backend-agnostic and must happen BEFORE data reaches renderers.
    /// Uses the pre-computed axis ranges for this specific facet cell.
    ///
    /// Filter DataFrame to only include rows for a specific facet cell
    ///
    /// For single facet: Returns data as-is (no filtering needed)
    /// For multiple facets: Filters by .ci and .ri, then drops those columns
    fn filter_by_facet(
        &self,
        df: DataFrame,
        col_idx: usize,
        row_idx: usize,
    ) -> Result<DataFrame, Box<dyn std::error::Error>> {
        use polars::prelude::lit;

        let single_facet = self.facet_info.total_facets() == 1;
        let mut polars_df = df.inner().clone();

        // Filter by facet indices if multiple facets
        if !single_facet {
            // Filter: .ci == col_idx AND .ri == row_idx
            polars_df = polars_df
                .lazy()
                .filter(
                    col(".ci")
                        .eq(lit(col_idx as i64))
                        .and(col(".ri").eq(lit(row_idx as i64))),
                )
                .collect()
                .map_err(|e| format!("Failed to filter by facet: {}", e))?;

            // Drop .ci and .ri columns after filtering (renderers don't need them)
            let col_names_to_drop: Vec<String> = vec![".ci".to_string(), ".ri".to_string()];
            polars_df = polars_df.drop_many(col_names_to_drop);
        }

        // Return DataFrame with .xs, .ys (will be dequantized to .x, .y in caller)
        Ok(ggrs_core::data::DataFrame::from_polars(polars_df))
    }
}

impl StreamGenerator for TercenStreamGenerator {
    fn n_col_facets(&self) -> usize {
        self.facet_info.n_col_facets()
    }

    fn n_row_facets(&self) -> usize {
        self.facet_info.n_row_facets()
    }

    fn n_total_data_rows(&self) -> usize {
        // Return total row count across ALL facets
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

        let series = Series::new("label".into(), labels);
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

        if labels.is_empty() {
            return ggrs_core::data::DataFrame::new();
        }

        let series = Series::new("label".into(), labels);
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
        self.axis_ranges
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
            })
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

    fn query_data_chunk(&self, col_idx: usize, row_idx: usize, data_range: Range) -> DataFrame {
        // Block on async within sync trait method
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.stream_facet_data(col_idx, row_idx, data_range).await })
        })
        .unwrap_or_else(|e| {
            eprintln!("Error querying data chunk: {}", e);
            DataFrame::new()
        })
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
