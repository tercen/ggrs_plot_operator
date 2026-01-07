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
        Some(e_schema::Object::Schema(_)) => {
            Err("Schema variant not supported (expected TableSchema, ComputedTableSchema, or CubeQueryTableSchema)".into())
        }
        None => {
            Err("Schema object is None".into())
        }
    }
}

/// Extract and print column names from schema
fn print_schema_columns(schema: &crate::tercen::client::proto::ESchema) {
    use crate::tercen::client::proto::{e_column_schema, e_schema};

    let columns = match &schema.object {
        Some(e_schema::Object::Tableschema(ts)) => &ts.columns,
        Some(e_schema::Object::Computedtableschema(cts)) => &cts.columns,
        Some(e_schema::Object::Cubequerytableschema(cqts)) => &cqts.columns,
        _ => {
            eprintln!("    Cannot extract columns from this schema type");
            return;
        }
    };

    eprintln!("    Available columns ({} total):", columns.len());
    for col_schema in columns {
        if let Some(obj) = &col_schema.object {
            let name = match obj {
                e_column_schema::Object::Column(col) => &col.name,
                e_column_schema::Object::Columnschema(cs) => &cs.name,
            };
            eprintln!("      - {}", name);
        }
    }
}

/// Tercen implementation of GGRS StreamGenerator
///
/// Streams data from Tercen using optimized .xs/.ys quantized columns.
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

    /// GGRS aesthetic mappings
    aes: Aes,

    /// GGRS facet specification
    facet_spec: FacetSpec,

    /// Chunk size for streaming (reserved for future use)
    #[allow(dead_code)]
    chunk_size: usize,
}

impl TercenStreamGenerator {
    /// Create a new stream generator
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
        // .xs and .ys are quantized uint16 coordinates (0-65535)
        // GGRS will dequantize using axis ranges automatically
                // Use synthetic .x (row index) for X-axis since there's no real X factor
        // Use quantized .ys for Y-axis
        let aes = Aes::new().x(".x").y(".ys");

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

    /// Load axis ranges from pre-computed Y-axis table and compute X-axis from data
    ///
    /// The Y-axis table contains columns: .ri, .minY, .maxY, .ticks
    /// There should be one row per facet cell (indexed by .ci and .ri)
    ///
    /// For X-axis (synthetic indices), we get total row count from schema
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
        // The Y-axis table has one row per facet cell (n_col_facets × n_row_facets)
        let expected_rows = facet_info.n_col_facets() * facet_info.n_row_facets();
        println!(
            "  Fetching Y-axis ranges from table (expecting {} rows = {} cols × {} rows)...",
            expected_rows,
            facet_info.n_col_facets(),
            facet_info.n_row_facets()
        );
        let data = streamer
            .stream_tson(
                y_axis_table_id,
                Some(columns_to_fetch),
                0,
                expected_rows as i64,
            )
            .await?;

        println!("  Got {} bytes, parsing...", data.len());
        let df = tson_to_dataframe(&data)?;

        println!("  Parsed {} rows", df.nrow());
        println!("  Columns: {:?}", df.columns());

        // Get total row count from main table schema (no per-facet counting needed with bulk streaming!)
        println!("  Getting total row count from main table schema...");
        let main_schema = streamer.get_schema(main_table_id).await?;
        let total_rows = extract_row_count_from_schema(&main_schema)? as usize;
        println!("  Total rows in main table: {}", total_rows);

        let mut axis_ranges = HashMap::new();

        // Check if we have .ci column (for column facets)
        let has_ci = df.columns().contains(&".ci".to_string());

        // Process each row
        for i in 0..df.nrow() {
            // Get facet indices
            let col_idx = if has_ci {
                match df.get_value(i, ".ci")? {
                    ggrs_core::data::Value::Int(v) => v as usize,
                    _ => 0,
                }
            } else {
                0 // Default to column 0 if no .ci
            };

            let row_idx = match df.get_value(i, ".ri")? {
                ggrs_core::data::Value::Int(v) => v as usize,
                _ => return Err(format!("Invalid .ri value at row {}", i).into()),
            };

            // Get Y-axis min/max
            let min_y = match df.get_value(i, ".minY")? {
                ggrs_core::data::Value::Float(v) => v,
                _ => return Err(format!("Invalid .minY value at row {}", i).into()),
            };

            let max_y = match df.get_value(i, ".maxY")? {
                ggrs_core::data::Value::Float(v) => v,
                _ => return Err(format!("Invalid .maxY value at row {}", i).into()),
            };

            // For X-axis range, we'll use total_rows across all facets
            // This is a simplification - we don't know per-facet row counts from the y-axis table
            let n_rows = total_rows; // Use total as an estimate

            println!(
                "  Facet ({}, {}): Y range [{}, {}] (using total row count {} for X-axis)",
                col_idx, row_idx, min_y, max_y, n_rows
            );

            // X-axis is synthetic indices (0 to n_rows-1)
            let x_axis = AxisData::Numeric(NumericAxisData {
                min_value: 0.0,
                max_value: n_rows as f64,
                min_axis: 0.0,
                max_axis: n_rows as f64,
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

        // Verify we have ranges for all expected facet cells
        let expected_cells = facet_info.n_col_facets() * facet_info.n_row_facets();
        if axis_ranges.len() != expected_cells {
            println!(
                "  WARNING: Expected {} facet cells but got {} ranges",
                expected_cells,
                axis_ranges.len()
            );
        }

        println!("  Loaded axis ranges for {} facet cells", axis_ranges.len());

        Ok((axis_ranges, total_rows))
    }

    /// Count rows per facet cell from the main data table
    ///
    /// This scans the main data table and counts how many rows belong to each
    /// (col_idx, row_idx) combination. This is used to set proper X-axis ranges
    /// when X-axis represents row indices.
    async fn count_rows_per_facet(
        client: &TercenClient,
        table_id: &str,
        facet_info: &FacetInfo,
    ) -> Result<HashMap<(usize, usize), usize>, Box<dyn std::error::Error>> {
        println!("  Counting rows per facet cell...");

        let streamer = TableStreamer::new(client);

        // Get the schema to find total row count and available columns
        let schema = streamer.get_schema(table_id).await?;
        let total_rows = extract_row_count_from_schema(&schema)?;
        println!("    Main table has {} total rows", total_rows);
        print_schema_columns(&schema);

        let chunk_size = 50000i64; // Use a reasonable chunk size
        let mut offset = 0;

        // Track row counts for each facet cell
        let mut cell_row_counts: HashMap<(usize, usize), usize> = HashMap::new();

        // Initialize all cells to 0
        for col_idx in 0..facet_info.n_col_facets() {
            for row_idx in 0..facet_info.n_row_facets() {
                cell_row_counts.insert((col_idx, row_idx), 0);
            }
        }

        // Only need .ci and .ri columns for counting
        let columns = Some(vec![".ci".to_string(), ".ri".to_string()]);

        // Stream through all data
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

            // Count rows for each facet cell
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

                *cell_row_counts.entry((col_idx, row_idx)).or_insert(0) += 1;
            }

            offset += row_count as i64;

            // Show progress every few chunks
            if offset % (chunk_size * 5) < chunk_size {
                println!(
                    "    Progress: {}/{} rows counted",
                    offset.min(total_rows),
                    total_rows
                );
            }
        }

        println!("    Counted rows for {} facet cells", cell_row_counts.len());
        for ((col_idx, row_idx), count) in &cell_row_counts {
            println!("      Facet ({}, {}): {} rows", col_idx, row_idx, count);
        }

        Ok(cell_row_counts)
    }

    /// Compute axis ranges and total row count for all facet cells
    ///
    /// This scans the main data table and calculates:
    /// - min/max for x and y axes for each (col_idx, row_idx) combination
    /// - total row count across all facets
    async fn compute_axis_ranges(
        client: &TercenClient,
        table_id: &str,
        facet_info: &FacetInfo,
        chunk_size: usize,
    ) -> Result<(HashMap<(usize, usize), (AxisData, AxisData)>, usize), Box<dyn std::error::Error>>
    {
        println!("Computing axis ranges for all facet cells...");

        let streamer = TableStreamer::new(client);

        // First, get the schema to find total row count
        println!("  [PROGRESS] Getting table schema...");
        let schema = streamer.get_schema(table_id).await?;
        let total_rows = extract_row_count_from_schema(&schema)?;
        println!("  [PROGRESS] Table has {} total rows", total_rows);

        let chunk_size = chunk_size as i64;
        let mut offset = 0;

        // Track min/max and row counts for each facet cell
        let mut cell_stats: HashMap<(usize, usize), (f64, f64, f64, f64)> = HashMap::new();
        let mut cell_row_counts: HashMap<(usize, usize), usize> = HashMap::new();

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
                cell_row_counts.insert((col_idx, row_idx), 0);
            }
        }

        // Define columns to retrieve from main table
        // Note: .x might not exist (if no x-axis factor), but .y is always present
        // For this case, request only .y and facet indices initially
        let columns = Some(vec![".ci".to_string(), ".ri".to_string(), ".y".to_string()]);

        // Stream through all data
        loop {
            // Calculate how many rows we should request (don't request more than total_rows)
            let remaining = total_rows - offset;
            if remaining <= 0 {
                println!(
                    "  [PROGRESS] Reached end of table (offset {} >= total_rows {})",
                    offset, total_rows
                );
                break;
            }
            let limit = remaining.min(chunk_size);

            println!(
                "  [PROGRESS] Fetching chunk at offset {} (limit {})...",
                offset, limit
            );
            let arrow_data = streamer
                .stream_tson(table_id, columns.clone(), offset, limit)
                .await?;

            println!("  [PROGRESS] Got {} bytes", arrow_data.len());

            if arrow_data.is_empty() {
                println!("  [PROGRESS] Empty chunk, ending loop");
                break;
            }

            println!("  [PROGRESS] Parsing TSON...");
            // Parse TSON to DataFrame
            let df = tson_to_dataframe(&arrow_data)?;
            let row_count = df.nrow();
            println!("  [PROGRESS] Parsed {} rows", row_count);
            println!("  [PROGRESS] Columns: {:?}", df.columns());

            println!("  [PROGRESS] Processing {} rows...", row_count);

            // Update stats for each facet cell
            for i in 0..row_count {
                // Extract facet indices
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

                // Extract x and y values
                // If .x doesn't exist, generate synthetic x based on sorted sequence
                let x = df
                    .get_value(i, ".x")
                    .ok()
                    .and_then(|v| v.as_f64())
                    .or(Some((offset + i as i64) as f64)); // Use global row index if no .x column
                let y = df.get_value(i, ".y").ok().and_then(|v| v.as_f64());

                if i < 3 {
                    println!(
                        "      Row {}: ci={}, ri={}, x={:?}, y={:?}",
                        i, col_idx, row_idx, x, y
                    );
                }

                if let (Some(x), Some(y)) = (x, y) {
                    if let Some(stats) = cell_stats.get_mut(&(col_idx, row_idx)) {
                        stats.0 = stats.0.min(x); // x_min
                        stats.1 = stats.1.max(x); // x_max
                        stats.2 = stats.2.min(y); // y_min
                        stats.3 = stats.3.max(y); // y_max
                    }
                    // Increment row count for this facet cell
                    *cell_row_counts.get_mut(&(col_idx, row_idx)).unwrap() += 1;
                }
            }

            offset += limit;

            if row_count < limit as usize {
                println!(
                    "  [PROGRESS] Last chunk had {} < {} rows (unexpected)",
                    row_count, limit
                );
                break;
            }
        }

        // Convert stats to AxisData
        let mut ranges = HashMap::new();
        for ((col_idx, row_idx), (x_min, x_max, y_min, y_max)) in cell_stats {
            // Add 5% padding to axes
            let x_padding = (x_max - x_min) * 0.05;
            let y_padding = (y_max - y_min) * 0.05;

            let x_axis = AxisData::Numeric(NumericAxisData {
                min_value: x_min,
                max_value: x_max,
                min_axis: x_min - x_padding,
                max_axis: x_max + x_padding,
                transform: None,
            });

            let y_axis = AxisData::Numeric(NumericAxisData {
                min_value: y_min,
                max_value: y_max,
                min_axis: y_min - y_padding,
                max_axis: y_max + y_padding,
                transform: None,
            });

            ranges.insert((col_idx, row_idx), (x_axis, y_axis));
        }

        println!("Computed ranges for {} facet cells", ranges.len());

        // Print row counts for debugging
        for ((col_idx, row_idx), count) in &cell_row_counts {
            println!("  Facet ({}, {}): {} rows", col_idx, row_idx, count);
        }

        // Return total row count instead of per-facet counts
        let total_rows = total_rows as usize;
        println!("  Total rows across all facets: {}", total_rows);

        Ok((ranges, total_rows))
    }

    /// Stream all data for a specific facet cell
    ///
    /// Returns a GGRS DataFrame containing all points for this facet.
    /// Stream data in bulk across ALL facets with .ci and .ri columns
    async fn stream_bulk_data(
        &self,
        data_range: Range,
    ) -> Result<DataFrame, Box<dyn std::error::Error>> {
        use polars::prelude::*;

        let streamer = TableStreamer::new(&self.client);

        // For bulk streaming, ALWAYS include facet indices
        let columns = vec![
            ".ci".to_string(),
            ".ri".to_string(),
            ".xs".to_string(),
            ".ys".to_string(),
        ];

        eprintln!(
            "DEBUG: Bulk streaming rows {}-{} (len={}) with facet indices",
            data_range.start,
            data_range.end,
            data_range.len()
        );

        // Fetch the requested range directly (GGRS handles chunking)
        let offset = data_range.start as i64;
        let limit = (data_range.end - data_range.start) as i64;

        let tson_data = streamer
            .stream_tson(&self.main_table_id, Some(columns.clone()), offset, limit)
            .await?;

        if tson_data.is_empty() {
            return Ok(ggrs_core::data::DataFrame::new());
        }

        // Parse TSON to DataFrame - columns include .ci, .ri, .xs, .ys
        let df = tson_to_dataframe(&tson_data)?;

        // DEBUG: Check .ys values from TSON - sample across the dataset
        if df.has_column(".ys") {
            if let Ok(ys_col) = df.column(".ys") {
                let n = ys_col.len();
                // Sample: first 3, middle 3, last 3
                let samples = vec![
                    0, 1, 2,                    // First 3
                    n/4, n/4+1, n/4+2,          // Quarter point
                    n/2, n/2+1, n/2+2,          // Middle
                    3*n/4, 3*n/4+1, 3*n/4+2,    // Three-quarter point
                    n-3, n-2, n-1               // Last 3
                ];
                for &i in &samples {
                    if i < n {
                        if let Some(val) = ys_col.get(i) {
                            eprintln!("  BULK .ys[{}] = {:?}", i, val);
                        }
                    }
                }
            }
        }

        // Add .x column (synthetic X values = global row indices) using Polars operations
        let nrows = df.nrow();
        let x_values: Vec<f64> = (0..nrows).map(|i| (offset + i as i64) as f64).collect();
        let x_series = Series::new(".x".into(), x_values);

        // Add the new column to the Polars DataFrame
        let mut polars_df = df.inner().clone();
        let _result = polars_df
            .with_column(x_series.into_column())
            .map_err(|e| format!("Failed to add .x column: {}", e))?;

        Ok(ggrs_core::data::DataFrame::from_polars(polars_df))
    }

    async fn stream_facet_data(
        &self,
        col_idx: usize,
        row_idx: usize,
        data_range: Range,
    ) -> Result<DataFrame, Box<dyn std::error::Error>> {
        let streamer = TableStreamer::new(&self.client);

        // Define columns to retrieve
        // .xs and .ys are uint16 quantized coordinates (0-65535)
        // GGRS will handle conversion to actual coordinate space using axis ranges
        // For single facet: Only need quantized coordinates
        // For multiple facets: Need .ci/.ri for filtering
        let columns = if self.facet_info.total_facets() == 1 {
            // Single facet: Only need quantized coordinates (4 bytes per row)
            vec![".xs".to_string(), ".ys".to_string()]
        } else {
            // Multiple facets: Need facet indices for filtering
            vec![
                ".ci".to_string(),
                ".ri".to_string(),
                ".xs".to_string(),
                ".ys".to_string(),
            ]
        };

        eprintln!(
            "DEBUG: GGRS requesting facet ({}, {}) rows {}-{} (len={})",
            col_idx,
            row_idx,
            data_range.start,
            data_range.end,
            data_range.len()
        );

        // Use configured chunk_size for all requests to Tercen
        let chunk_size = self.chunk_size as i64;

        let mut all_chunks: Vec<polars::frame::DataFrame> = Vec::new();
        let mut current_offset = data_range.start as i64;
        let end_offset = data_range.end as i64;
        let mut facet_local_index = data_range.start; // Track X values across chunks

        while current_offset < end_offset {
            let remaining = end_offset - current_offset;
            let limit = remaining.min(chunk_size);

            eprintln!(
                "DEBUG:   Fetching chunk: offset={}, limit={}",
                current_offset, limit
            );

            let fetch_start = std::time::Instant::now();
            let arrow_data = streamer
                .stream_tson(
                    &self.main_table_id,
                    Some(columns.clone()),
                    current_offset,
                    limit,
                )
                .await?;
            let fetch_time = fetch_start.elapsed();

            if arrow_data.is_empty() {
                break;
            }

            // Parse TSON to DataFrame
            let parse_start = std::time::Instant::now();
            let df = tson_to_dataframe(&arrow_data)?;
            let fetched_rows = df.nrow();
            let parse_time = parse_start.elapsed();

            eprintln!("TIMING: Fetch={:.2}ms, Parse={:.2}ms for {} rows",
                     fetch_time.as_secs_f64() * 1000.0,
                     parse_time.as_secs_f64() * 1000.0,
                     fetched_rows);

            // Filter by facet indices and add synthetic x values (STAY COLUMNAR)
            let filter_start = std::time::Instant::now();
            let filtered_df =
                self.filter_dataframe_by_facet(df, col_idx, row_idx, facet_local_index)?;
            let filter_time = filter_start.elapsed();
            eprintln!("TIMING: Filter+Convert={:.2}ms for {} rows",
                     filter_time.as_secs_f64() * 1000.0, filtered_df.nrow());
            eprintln!(
                "DEBUG:   Filtered to {} rows for facet ({}, {})",
                filtered_df.nrow(),
                col_idx,
                row_idx
            );

            // Accumulate the filtered Polars DataFrame
            if filtered_df.nrow() > 0 {
                all_chunks.push(filtered_df.inner().clone());
            }

            // Update offsets
            facet_local_index += filtered_df.nrow();
            current_offset += fetched_rows as i64;

            // Safety check: if we didn't get any rows from this chunk, stop
            if fetched_rows == 0 {
                break;
            }
        }

        eprintln!(
            "DEBUG: Total accumulated {} chunks for facet ({}, {})",
            all_chunks.len(),
            col_idx,
            row_idx
        );

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


        // Debug: Show first and last X values
        if result_df.height() > 0 {
            if let Ok(x_col) = result_df.column(".x") {
                if let Ok(first_x) = x_col.as_materialized_series().get(0) {
                    if let Ok(last_x) = x_col.as_materialized_series().get(result_df.height() - 1) {
                    }
                }
            }
        }

        Ok(ggrs_core::data::DataFrame::from_polars(result_df))
    }

    /// Filter DataFrame to only include rows for a specific facet cell
    ///
    /// Converts quantized .xs/.ys (uint16) to float64 .x/.y coordinates.
    /// For synthetic X axis, uses sequential position within facet.
    /// The global_offset parameter represents the starting facet-local index for this chunk.
    fn filter_dataframe_by_facet(
        &self,
        df: DataFrame,
        col_idx: usize,
        row_idx: usize,
        global_offset: usize,
    ) -> Result<DataFrame, Box<dyn std::error::Error>> {
        use polars::prelude::{col, lit, NamedFrom, Series};

        let single_facet = self.facet_info.total_facets() == 1;
        let mut polars_df = df.inner().clone();

        // Step 1: Filter by facet indices (if multiple facets)
        if !single_facet {
            // Filter: .ci == col_idx AND .ri == row_idx
            // Use lazy API for better performance
            polars_df = polars_df
                .lazy()
                .filter(
                    col(".ci")
                        .eq(lit(col_idx as i64))
                        .and(col(".ri").eq(lit(row_idx as i64))),
                )
                .collect()
                .map_err(|e| format!("Failed to filter by facet: {}", e))?;

            // Drop .ci and .ri columns after filtering (not needed by GGRS)
            let col_names_to_drop: Vec<String> = vec![".ci".to_string(), ".ri".to_string()];
            polars_df = polars_df.drop_many(col_names_to_drop);
        }

        // Step 2: Add synthetic .x column based on sequential position within this facet
        let filtered_nrows = polars_df.height();
        let x_values: Vec<f64> = (0..filtered_nrows)
            .map(|i| (global_offset + i) as f64)
            .collect();
        let x_series = Series::new(".x".into(), x_values);

        polars_df
            .with_column(x_series.into_column())
            .map_err(|e| format!("Failed to add .x column: {}", e))?;

        // Step 3: Dequantize .ys to .y using axis ranges (BACKEND-AGNOSTIC!)
        // This conversion happens ONCE here, not twice in Cairo and WebGPU backends
        if let Ok(ys_col) = polars_df.column(".ys") {
            let conv_start = std::time::Instant::now();

            // Get Y-axis range for this facet
            let y_axis_data = self.axis_ranges
                .get(&(col_idx, row_idx))
                .ok_or_else(|| format!("No axis ranges for facet ({}, {})", col_idx, row_idx))?
                .1.clone(); // Second element is Y-axis

            let (min_y, max_y) = match y_axis_data {
                ggrs_core::stream::AxisData::Numeric(num_data) => {
                    (num_data.min_value, num_data.max_value)
                }
                _ => {
                    return Err("Categorical Y axis not supported for quantized coordinates".into());
                }
            };
            let y_range = max_y - min_y;

            // Convert uint16 quantized values (0-65535) to actual Y values
            let y_values: Vec<f64> = ys_col
                .as_materialized_series()
                .i64()
                .map_err(|e| format!("Failed to convert .ys to i64: {}", e))?
                .into_iter()
                .map(|opt_val| {
                    opt_val
                        .map(|ys_val| min_y + (ys_val as f64 / 65535.0) * y_range)
                        .unwrap_or(f64::NAN)
                })
                .collect();

            let y_series = Series::new(".y".into(), y_values);
            polars_df
                .with_column(y_series.into_column())
                .map_err(|e| format!("Failed to add .y column: {}", e))?;

            let conv_time = conv_start.elapsed();
            eprintln!("TIMING: Coordinate conversion for {} rows took {:.2}ms",
                     filtered_nrows, conv_time.as_secs_f64() * 1000.0);
        }

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
