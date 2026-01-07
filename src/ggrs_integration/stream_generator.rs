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
    /// Create a new stream generator from workflow and step IDs
    ///
    /// This is the primary constructor used by the operator.
    pub async fn from_workflow_step(
        client: Arc<TercenClient>,
        workflow_id: &str,
        step_id: &str,
        chunk_size: usize,
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

        eprintln!(
            "DEBUG: Streaming facet ({}, {}) rows {}-{} (len={})",
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

        while current_offset < end_offset {
            let remaining = end_offset - current_offset;
            let limit = remaining.min(chunk_size);

            eprintln!(
                "DEBUG:   Fetching chunk: offset={}, limit={}",
                current_offset, limit
            );

            let fetch_start = std::time::Instant::now();
            let tson_data = streamer
                .stream_tson(
                    &self.main_table_id,
                    Some(columns.clone()),
                    current_offset,
                    limit,
                )
                .await?;
            let fetch_time = fetch_start.elapsed();

            if tson_data.is_empty() {
                break;
            }

            // Parse TSON to DataFrame
            let parse_start = std::time::Instant::now();
            let df = tson_to_dataframe(&tson_data)?;
            let fetched_rows = df.nrow();
            let parse_time = parse_start.elapsed();

            eprintln!(
                "TIMING: Fetch={:.2}ms, Parse={:.2}ms for {} rows",
                fetch_time.as_secs_f64() * 1000.0,
                parse_time.as_secs_f64() * 1000.0,
                fetched_rows
            );

            // Filter by facet indices if needed
            let filter_start = std::time::Instant::now();
            let filtered_df = self.filter_by_facet(df, col_idx, row_idx)?;
            let filter_time = filter_start.elapsed();

            eprintln!(
                "TIMING: Filter={:.2}ms for {} rows",
                filter_time.as_secs_f64() * 1000.0,
                filtered_df.nrow()
            );
            eprintln!(
                "DEBUG:   Filtered to {} rows for facet ({}, {})",
                filtered_df.nrow(),
                col_idx,
                row_idx
            );

            // Dequantize coordinates: .xs/.ys (uint16 0-65535) → .x/.y (actual data values)
            // This MUST happen before the data reaches the backends
            let dequant_start = std::time::Instant::now();
            let dequantized_df =
                self.dequantize_coordinates(filtered_df.inner().clone(), col_idx, row_idx)?;
            let dequant_time = dequant_start.elapsed();

            eprintln!(
                "TIMING: Dequantize={:.2}ms for {} rows",
                dequant_time.as_secs_f64() * 1000.0,
                dequantized_df.height()
            );

            // Accumulate the dequantized Polars DataFrame
            if dequantized_df.height() > 0 {
                all_chunks.push(dequantized_df);
            }

            current_offset += fetched_rows as i64;

            // Safety check: if we didn't get any rows from this chunk, stop
            if fetched_rows == 0 {
                break;
            }
        }

        eprintln!(
            "DEBUG: Accumulated {} chunks for facet ({}, {})",
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
    fn dequantize_coordinates(
        &self,
        mut df: polars::frame::DataFrame,
        col_idx: usize,
        row_idx: usize,
    ) -> Result<polars::frame::DataFrame, Box<dyn std::error::Error>> {
        // Get axis ranges for this facet cell
        let (x_axis, y_axis) = self
            .axis_ranges
            .get(&(col_idx, row_idx))
            .ok_or("Axis ranges not found for facet cell")?;

        // Extract min/max from axis data
        let (x_min, x_max) = match x_axis {
            AxisData::Numeric(data) => (data.min_value, data.max_value),
            _ => return Err("X-axis is not numeric".into()),
        };

        let (y_min, y_max) = match y_axis {
            AxisData::Numeric(data) => (data.min_value, data.max_value),
            _ => return Err("Y-axis is not numeric".into()),
        };

        // Dequantize: quantized_value / 65535 * (max - min) + min
        // .xs and .ys are stored as i64 but represent uint16 values (0-65535)
        const QUANTIZE_MAX: f64 = 65535.0;

        // Get .xs column, convert to f64, dequantize, create .x column
        if let Ok(xs_col) = df.column(".xs") {
            let xs_series = xs_col.as_materialized_series();
            if let Ok(xs_i64) = xs_series.i64() {
                let x_values: Vec<f64> = xs_i64
                    .into_iter()
                    .map(|opt| {
                        opt.map(|quantized| {
                            let normalized = (quantized as f64) / QUANTIZE_MAX;
                            normalized * (x_max - x_min) + x_min
                        })
                        .unwrap_or(f64::NAN)
                    })
                    .collect();

                use polars::prelude::NamedFrom;
                let x_series = polars::prelude::Series::from_vec(".x".into(), x_values);
                df.with_column(x_series.into_column())
                    .map_err(|e| format!("Failed to add .x column: {}", e))?;
            }
        }

        // Get .ys column, convert to f64, dequantize, create .y column
        if let Ok(ys_col) = df.column(".ys") {
            let ys_series = ys_col.as_materialized_series();
            if let Ok(ys_i64) = ys_series.i64() {
                let y_values: Vec<f64> = ys_i64
                    .into_iter()
                    .map(|opt| {
                        opt.map(|quantized| {
                            let normalized = (quantized as f64) / QUANTIZE_MAX;
                            normalized * (y_max - y_min) + y_min
                        })
                        .unwrap_or(f64::NAN)
                    })
                    .collect();

                use polars::prelude::NamedFrom;
                let y_series = polars::prelude::Series::from_vec(".y".into(), y_values);
                df.with_column(y_series.into_column())
                    .map_err(|e| format!("Failed to add .y column: {}", e))?;
            }
        }

        Ok(df)
    }

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
