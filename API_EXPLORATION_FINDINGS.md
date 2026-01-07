# Tercen API Exploration: Per-Facet Row Counts

**Date**: 2026-01-06
**Goal**: Find efficient way to get per-facet row counts without streaming all data

## Dataset Context
- **Test dataset**: Crabs (1000 rows total)
- **Facets**: 2 columns (sex: F, M) × 5 rows (variable: FL, RW, CL, CW, BD) = 10 cells
- **Expected**: ~100 rows per facet cell on average

## API Structures Explored

### 1. CubeQueryTask
```
Generated tables (schema_ids):
- Main table (qt_hash): Contains all data with .ci, .ri columns
- Column table: Facet group definitions (2 rows)
- Row table: Facet group definitions (5 rows)
- Y-axis table: Axis metadata (.ri, .ticks, .minY, .maxY)
```

### 2. Table Schemas
Each schema provides:
- `nRows`: **Total** row count for the table
- `columns`: Column names and types
- **NO per-facet statistics**

Example:
```
Main Table Schema:
  Type: CubeQueryTableSchema
  nRows: 1000  ← TOTAL rows only
  Columns:
    - .ci(int32)       ← Column facet index
    - .ri(int32)       ← Row facet index
    - .y(double)       ← Data values
    - ... (other columns)
```

### 3. Facet Tables
```
Column Table:
  nRows: 2  ← Number of column facets
  Columns: sex(string)

Row Table:
  nRows: 5  ← Number of row facets
  Columns: variable(string)
```

These tell us **how many facet groups** exist, but NOT how many data rows belong to each.

### 4. Y-Axis Table
```
Y-axis Table:
  nRows: 5
  Columns:
    - .ri(int32)     ← Row index
    - .ticks(double) ← Axis tick values
    - .minY(double)  ← Min Y value for this row
    - .maxY(double)  ← Max Y value for this row
```

Provides axis ranges per row, but **NO row counts**.

## Conclusion

### ❌ Per-Facet Row Counts NOT Available

The Tercen API does **NOT** provide per-facet row counts in any metadata structure:
- ✅ Total row count: Available in schema
- ✅ Facet dimensions: Available in facet tables
- ✅ Axis ranges: Available in Y-axis table
- ❌ **Per-facet row counts: NOT AVAILABLE**

### Current Approach is Correct

The current implementation in `stream_generator.rs::count_rows_per_facet()` that streams through all data to count rows per facet is the **only way** to get this information.

### Optimization Opportunities

Instead of eliminating the row counting phase, we should optimize **how** we count:

1. **Reduce Memory Usage**:
   - Stream only `.ci` and `.ri` columns (already doing this ✓)
   - Use smaller chunk sizes during counting (currently 50K)
   - Immediately discard data after counting (already doing this ✓)

2. **Parallel Counting** (future):
   - If Tercen supports range queries, could count multiple ranges in parallel
   - Would require investigating if API supports `WHERE .ci = 0 AND .ri = 0` style filtering

3. **Cache Results** (future):
   - For workflows that don't change, cache per-facet counts
   - Invalidate cache when data changes

## Memory Usage Analysis

From `memory_usage_chunk_8096.png`:
- **Phase 2** (row counting): ~20 MB for 10 seconds
- **Phase 3** (GGRS rendering): ~27 MB for 6 seconds

The counting phase is **necessary** but could be more memory-efficient by:
- Processing in smaller chunks (8096 rows instead of 50K)
- Streaming incrementally without buffering

## Recommendation

**Keep the current row counting approach** but optimize:
1. Reduce chunk size during counting to lower peak memory
2. Add progress reporting so users understand what's happening
3. Document that this is a necessary phase (no API shortcut exists)
