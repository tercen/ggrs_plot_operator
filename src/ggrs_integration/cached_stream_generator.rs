// Cached Stream Generator for Pagination Optimization
//
// This wrapper caches data streamed from Tercen and serves it to multiple
// page-filtered StreamGenerators, avoiding redundant data transfers.
//
// **Problem**: Current pagination streams ALL data once per page
// - Page 1: Stream 44K rows → Filter → Keep 22K
// - Page 2: Stream same 44K rows AGAIN → Filter → Keep 22K
// - Total: 88K rows streamed for 44K points (71% slowdown)
//
// **Solution**: Cache chunks as they're streamed
// - First page: Stream 44K rows → Cache → Filter → Keep 22K
// - Second page: Read from cache → Filter → Keep 22K
// - Total: 44K rows streamed for 44K points (no overhead!)

use ggrs_core::aes::Aes;
use ggrs_core::data::DataFrame;
use ggrs_core::legend::LegendScale;
use ggrs_core::stream::{AxisData, FacetSpec, Range, StreamGenerator};
use polars::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Cached data chunk with offset tracking
#[derive(Clone)]
struct CachedChunk {
    /// Starting row offset of this chunk
    #[allow(dead_code)]
    offset: usize,
    /// The cached DataFrame
    data: DataFrame,
}

/// Shared cache for streamed data chunks
///
/// This is wrapped in Arc<Mutex<>> so multiple FilteredStreamGenerator
/// instances can share the same cache.
pub struct DataCache {
    /// Cached chunks, keyed by offset
    chunks: HashMap<usize, CachedChunk>,
    /// Total rows in the dataset
    #[allow(dead_code)]
    total_rows: usize,
}

impl DataCache {
    #[allow(dead_code)]
    fn new(total_rows: usize) -> Self {
        Self {
            chunks: HashMap::new(),
            total_rows,
        }
    }

    /// Get cached chunk or return None if not cached
    fn get(&self, offset: usize) -> Option<&CachedChunk> {
        self.chunks.get(&offset)
    }

    /// Store a chunk in the cache
    fn insert(&mut self, offset: usize, data: DataFrame) {
        self.chunks.insert(offset, CachedChunk { offset, data });
    }
}

/// Wrapper around TercenStreamGenerator that filters data for a specific page
///
/// Multiple instances can share the same underlying generator and cache,
/// each filtering for a different page.
pub struct FilteredStreamGenerator {
    /// The underlying stream generator (shared across all pages)
    inner: Arc<Box<dyn StreamGenerator>>,
    /// Shared data cache
    cache: Arc<Mutex<DataCache>>,
    /// Valid row indices for this page (from page filter)
    /// Maps original .ri values to filtered 0-based indices
    row_index_map: HashMap<i64, i64>,
    /// Number of column facets (same for all pages)
    n_col_facets: usize,
    /// Number of row facets for THIS page (after filtering)
    n_row_facets: usize,
}

impl FilteredStreamGenerator {
    /// Create a new filtered stream generator
    ///
    /// # Arguments
    /// * `inner` - The underlying TercenStreamGenerator (shared)
    /// * `cache` - Shared data cache
    /// * `valid_row_indices` - Original row indices to keep for this page
    /// * `n_col_facets` - Number of column facets
    /// * `n_row_facets` - Number of row facets for this page
    pub fn new(
        inner: Arc<Box<dyn StreamGenerator>>,
        cache: Arc<Mutex<DataCache>>,
        valid_row_indices: Vec<i64>,
        n_col_facets: usize,
        n_row_facets: usize,
    ) -> Self {
        // Build row index remapping: original_index → new_index
        let row_index_map: HashMap<i64, i64> = valid_row_indices
            .iter()
            .enumerate()
            .map(|(new_idx, &orig_idx)| (orig_idx, new_idx as i64))
            .collect();

        Self {
            inner,
            cache,
            row_index_map,
            n_col_facets,
            n_row_facets,
        }
    }

    /// Filter a DataFrame to keep only rows for this page
    fn filter_dataframe(&self, df: &DataFrame) -> DataFrame {
        // Access the inner Polars DataFrame
        let mut polars_df = df.inner().clone();

        // Get .ri column
        let ri_series = match polars_df.column(".ri") {
            Ok(col) => col,
            Err(_) => return df.clone(), // No .ri column, return unchanged
        };

        // Create filter mask: keep rows where .ri is in our valid set
        let ri_i64 = ri_series.i64().expect(".ri must be Int64");
        let mask: BooleanChunked = ri_i64
            .into_iter()
            .map(|opt_val| opt_val.is_some_and(|val| self.row_index_map.contains_key(&val)))
            .collect();

        // Filter the DataFrame
        polars_df = polars_df.filter(&mask).expect("Filter failed");

        // Remap .ri values: original_index → new_index
        let ri_series = polars_df.column(".ri").expect(".ri column missing");
        let ri_i64 = ri_series.i64().expect(".ri must be Int64");

        let mut remapped_ri: Int64Chunked = ri_i64
            .into_iter()
            .map(|opt_val| opt_val.and_then(|orig_idx| self.row_index_map.get(&orig_idx).copied()))
            .collect();

        // Set the column name to ".ri" to replace the original column
        remapped_ri.rename(".ri".into());

        // Replace .ri column with remapped values
        polars_df
            .with_column(remapped_ri.into_series())
            .expect("Failed to replace .ri column");

        // Convert back to GGRS DataFrame
        DataFrame::from_polars(polars_df)
    }
}

impl StreamGenerator for FilteredStreamGenerator {
    fn n_col_facets(&self) -> usize {
        self.n_col_facets
    }

    fn n_row_facets(&self) -> usize {
        self.n_row_facets
    }

    fn n_total_data_rows(&self) -> usize {
        // We don't know the total rows after filtering
        // Use the inner generator's total as an upper bound
        self.inner.n_total_data_rows()
    }

    fn query_col_facet_labels(&self) -> DataFrame {
        // Delegate to inner generator (column facets are not filtered)
        self.inner.query_col_facet_labels()
    }

    fn query_row_facet_labels(&self) -> DataFrame {
        // Delegate to inner generator (row facets are filtered but labels come from metadata)
        // NOTE: The inner generator should have already filtered facets based on page
        self.inner.query_row_facet_labels()
    }

    fn facet_spec(&self) -> &FacetSpec {
        // Delegate to inner generator
        self.inner.facet_spec()
    }

    fn aes(&self) -> &Aes {
        // Delegate to inner generator
        self.inner.aes()
    }

    fn preferred_chunk_size(&self) -> Option<usize> {
        self.inner.preferred_chunk_size()
    }

    fn query_x_axis(&self, col_idx: usize, row_idx: usize) -> AxisData {
        // Delegate to inner generator
        self.inner.query_x_axis(col_idx, row_idx)
    }

    fn query_y_axis(&self, col_idx: usize, row_idx: usize) -> AxisData {
        // Delegate to inner generator
        // Note: row_idx here is the FILTERED index (0-based)
        // The inner generator should have remapped Y-axis ranges already
        self.inner.query_y_axis(col_idx, row_idx)
    }

    fn query_legend_scale(&self) -> LegendScale {
        // Delegate to inner generator
        self.inner.query_legend_scale()
    }

    fn query_data_chunk(&self, _col_idx: usize, _row_idx: usize, _data_range: Range) -> DataFrame {
        // Not used in optimized multi-facet rendering
        unimplemented!("Use query_data_multi_facet instead")
    }

    fn query_data_multi_facet(&self, data_range: Range) -> DataFrame {
        let offset = data_range.start;
        let end = data_range.end;

        // Try to get from cache first
        {
            let cache = self.cache.lock().unwrap();
            if let Some(cached_chunk) = cache.get(offset) {
                eprintln!("DEBUG: Cache HIT for offset {}", offset);
                // Filter and return cached data
                return self.filter_dataframe(&cached_chunk.data);
            }
        }

        // Cache miss - stream from underlying generator
        eprintln!(
            "DEBUG: Cache MISS for offset {} - streaming from Tercen",
            offset
        );
        let chunk = self.inner.query_data_multi_facet(Range::new(offset, end));

        // Store in cache
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(offset, chunk.clone());
        }

        // Filter and return
        self.filter_dataframe(&chunk)
    }
}
