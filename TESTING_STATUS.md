# Implementation Status - Testing Phase

## Completed (Ready for Testing)

### ✅ Phase 1-4: Core Infrastructure
- gRPC client with authentication
- Data streaming with chunking
- CSV parsing and facet filtering
- Logging to Tercen

### ✅ Phase 5: Facet Metadata Loading
- `FacetMetadata` struct for column/row facets
- `FacetInfo` to manage both dimensions
- Loads facet tables from Tercen
- Parses multi-column facets correctly

### ✅ Phase 6: GGRS Integration (Partial)
- **`TercenStreamGenerator`** - Fully implements GGRS `StreamGenerator` trait:
  - ✅ `n_col_facets()` / `n_row_facets()` - Returns facet counts
  - ✅ `n_data_rows()` - Returns row count per facet cell
  - ✅ `query_col_facet_labels()` / `query_row_facet_labels()` - Returns facet labels
  - ✅ `query_x_axis()` / `query_y_axis()` - Returns axis ranges with 5% padding
  - ✅ `query_legend_scale()` - Returns legend metadata
  - ✅ `query_data_chunk()` - Streams data for specific facet cell
  - ✅ `facet_spec()` / `aes()` - Returns GGRS configuration
- **Axis Range Calculator** - Pre-computes min/max for all facet cells
- **Data Conversion** - Converts Tercen CSV to GGRS DataFrame
- **Async/Sync Bridge** - Uses `tokio::task::block_in_place()` for trait compatibility

### ✅ Test Binary
- Standalone `test_stream_generator` binary
- Tests all StreamGenerator functionality
- No task required - just token and table IDs
- Helper script `test_local.sh` for easy testing

## Testing Instructions

See [`TEST_LOCAL.md`](./TEST_LOCAL.md) for detailed testing instructions.

### Quick Test

```bash
# Get table IDs from a task first
export TERCEN_URI=https://tercen.com:5400
export TERCEN_TOKEN=your_token
export TERCEN_TASK_ID=task_id
cargo run  # Look for qt_hash, column_hash, row_hash

# Then test the stream generator
./test_local.sh "your_token" "qt_hash" "column_hash" "row_hash"
```

## What to Test

The test binary will verify:

1. **Connection** - Authenticates with Tercen
2. **Facet Loading** - Loads column and row facet metadata
3. **Axis Calculation** - Scans data and computes axis ranges for each facet cell
4. **Data Querying** - Queries first 100 rows from facet (0, 0)
5. **Data Format** - Verifies GGRS DataFrame structure

Expected behavior:
- Should connect successfully with valid token
- Should load facet metadata (or report 1x1 if no faceting)
- Should compute axis ranges (scans entire main table)
- Should return filtered data for specific facet cells
- Should convert data to GGRS format correctly

## Remaining Work (Phase 7+)

### ⏭️ Plot Generation
- Create plot spec from operator properties
- Call GGRS `PlotGenerator` with our StreamGenerator
- Render to PNG using `ImageRenderer`

### ⏭️ Result Upload
- Encode PNG to base64
- Create Tercen Table with `.content`, `filename`, `mimetype`
- Wrap in `OperatorResult`
- Serialize to JSON
- Upload via `FileService.uploadTable()`
- Update task with `fileResultId`

### ⏭️ Polish
- Error handling improvements
- Progress reporting during long operations
- Operator properties support (theme, dimensions, title)
- Full faceting support (map Tercen facets to GGRS FacetSpec)

## Dependencies Added

```toml
serde_json = "1.0"           # JSON serialization for OperatorResult
base64 = "0.22"              # Base64 encoding for PNG
ggrs-core = { path = "..." } # GGRS plotting library
```

## Build Targets

```bash
# Main operator
cargo build --bin ggrs_plot_operator

# Test binary
cargo build --bin test_stream_generator

# All targets
cargo build --all-targets
```

## Known Limitations (TODOs)

1. **Facet Labels** - Currently returns empty DataFrames (not used by GGRS yet)
2. **Row Counts** - `n_data_rows()` returns hardcoded 10,000 (needs pre-computation)
3. **Legend** - Returns `LegendScale::None` (no color mapping yet)
4. **FacetSpec** - Returns `FacetSpec::none()` (needs Tercen→GGRS mapping)
5. **Extra Columns** - All non-x/y columns treated as strings (needs type detection)

These don't block basic plotting but should be implemented for full feature support.

## Performance Notes

- **Axis range computation**: Scans entire main table once during initialization
- **Data streaming**: Queries data in 10K row chunks per facet cell
- **Facet filtering**: Applied after streaming (server-side filtering would be better)
- **Memory**: Only holds one chunk in memory at a time

## Architecture Benefits

✅ **Lazy Loading** - Data fetched only when GGRS needs it
✅ **No Arrow Complexity** - Direct CSV → DataFrame conversion
✅ **Streaming** - Handles large datasets without loading all into memory
✅ **Facet-Aware** - Pre-computes axis ranges per facet cell
✅ **Testable** - Can test without full Tercen task environment

## Next Session

After testing verifies the StreamGenerator works correctly:

1. Test with real Tercen data
2. Verify facet filtering works
3. Check axis ranges are correct
4. Confirm data format is compatible with GGRS
5. Then proceed with plot generation and PNG rendering
