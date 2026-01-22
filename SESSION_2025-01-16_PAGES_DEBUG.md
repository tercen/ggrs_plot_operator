# Pages Feature Debugging Session - 2025-01-16

## Problem Summary

The pages feature (splitting plots by Gender) is generating two PNG files (`plot_1.png` and `plot_2.png`), but **the second page (male) contains no data points**. The first page (female) renders correctly with all expected data.

## Root Cause Analysis

### User's Key Insight
The user asked: **"Is it possible GGRS is trying to place data points in the 'lower' facets, like it was a single plot?"**

This led to discovering the actual problem!

### The Issue: `.ri` Index Mismatch

When we filter facets for pagination:
- **Female page**: Facets 0-11 (original indices) → Remapped to 0-11 (same)
- **Male page**: Facets 12-23 (original indices) → Remapped to 0-11 (for GGRS)

The data still has **original `.ri` values** (12-23 for male), but GGRS expects **0-based indices** (0-11).

### Why This Causes Empty Plots

GGRS render code (`render.rs:1028-1031`):
```rust
if ri >= n_rows {
    eprintln!("WARNING: ri={} >= n_rows={}, skipping point", ri, n_rows);
    continue;  // Silently skips the point!
}
```

For male page:
- `n_rows = 12` (12 filtered facets)
- Data has `.ri = 12, 13, ..., 23` (original indices)
- Check: `12 >= 12` → **TRUE** → **ALL points skipped!**

### The Dequantization Error

Before the silent skipping, GGRS tries to dequantize coordinates (`stream.rs:645-651`):
```rust
let ri = ri_col_data.get(i).unwrap_or(0) as usize;
let (y_min, y_max) = y_ranges.get(&ri).ok_or_else(|| {
    GgrsError::InvalidInput(format!("No Y range found for row facet index {}", ri))
})?;
```

It reads `.ri = 12` from data and looks it up in `y_ranges` HashMap which only has keys `0..11` → **Error!**

## What We've Implemented

### 1. Added `original_index` Field to `FacetGroup`

**File**: `src/tercen/facets.rs:13-24`

```rust
pub struct FacetGroup {
    /// Index for GGRS (0-based after filtering)
    pub index: usize,
    /// Original index from full table (before filtering)
    pub original_index: usize,
    pub label: String,
    pub values: HashMap<String, String>,
}
```

### 2. Updated Facet Remapping Logic

**File**: `src/tercen/facets.rs:206-210`

When filtering facets for a page:
```rust
for (new_idx, group) in metadata.groups.iter_mut().enumerate() {
    eprintln!("  Remapping facet {} from original_index {} to index {}",
        group.label, group.original_index, new_idx);
    group.index = new_idx;  // Remap to 0-based
    // original_index is preserved!
}
```

### 3. Updated StreamGenerator to Use `original_index`

**File**: `src/ggrs_integration/stream_generator.rs`

#### Extract Original Indices for Filtering (line 280)
```rust
let original_indices: Vec<i64> = facet_info
    .row_facets
    .groups
    .iter()
    .map(|g| g.original_index as i64)  // Use original_index!
    .collect();
```

#### Axis Range Remapping (line 157)
```rust
for (new_row_idx, group) in facet_info.row_facets.groups.iter().enumerate() {
    let original_row_idx = group.original_index;  // Use original_index!
    for col_idx in 0..facet_info.n_col_facets() {
        if let Some(ranges) = axis_ranges.get(&(col_idx, original_row_idx)) {
            remapped_ranges.insert((col_idx, new_row_idx), ranges.clone());
        }
    }
}
```

#### Data `.ri` Column Remapping (lines 954-992)
```rust
if let Some(ref remap) = self.ri_remap {
    // Remap .ri values: 12→0, 13→1, ..., 23→11
    let remapped_ri: Int64Chunked = ri_series.into_iter().map(|opt_val| {
        opt_val.and_then(|val| remap.get(&val).copied())
    }).collect();

    polars_df.with_column(remapped_ri.into_series())?;
    df = ggrs_core::data::DataFrame::from_polars(polars_df);
}
```

## Current Status: Remapping Partially Works!

### Debug Output Shows Remapping IS Happening

```
DEBUG: First 5 .ri values BEFORE remapping: [Some(12), Some(12), ...]
DEBUG: First 5 .ri values AFTER remapping: [Some(0), Some(0), ...]
```

✅ The remapping logic executes correctly!

### But FINAL DataFrame Shows Mixed Results

For female page - all chunks show correct values:
```
DEBUG: First 5 .ri values in FINAL DataFrame: [Some(0), Some(0), ...]
DEBUG: First 5 .ri values in FINAL DataFrame: [Some(8), Some(8), ...]
DEBUG: First 5 .ri values in FINAL DataFrame: [Some(7), Some(7), ...]
```

For male page - **ONE chunk has unremap mapped values**:
```
DEBUG: First 5 .ri values in FINAL DataFrame: [Some(0), Some(0), ...]  ✅ Remapped
DEBUG: First 5 .ri values in FINAL DataFrame: [Some(12), Some(12), ...] ❌ NOT remapped!
```

### The Mystery: Why Does One Chunk Fail?

The remapping code processes all chunks the same way:
1. Filter by `valid_row_indices` (keeps only male rows with `.ri = 12-23`)
2. Remap `.ri` using `ri_remap` HashMap (`{12:0, 13:1, ..., 23:11}`)
3. Clone DataFrame for color column addition
4. Return final DataFrame

**Hypothesis**: There may be a code path where:
- A DataFrame is returned WITHOUT going through the remapping step, OR
- The Polars `with_column()` mutation isn't persisting in the returned DataFrame

## Next Steps for Tomorrow

### 1. Identify Which Chunk Has Unremap mapped Data

Run test with more detailed logging to find:
- Which offset/limit range produces `.ri = 12` in FINAL DataFrame?
- Is it during the initial data query phase, or during rendering?

### 2. Check Polars API Usage

The current remapping code:
```rust
let mut polars_df = df.inner().clone();
polars_df.with_column(remapped_ri.into_series())?;
df = ggrs_core::data::DataFrame::from_polars(polars_df);
```

Questions:
- Does `with_column()` actually mutate in place?
- Do we need to use the returned `&mut DataFrame` reference?
- Is the clone necessary, or does it create issues?

### 3. Verify Color Column Addition Doesn't Overwrite

After remapping (line 990), we call:
```rust
df = self.add_color_columns(df)?;  // Line 1001
```

Inside `add_color_columns` (line 1025):
```rust
let mut polars_df = df.inner().clone();
```

Does this clone preserve the remapped `.ri` column?

### 4. Add Comprehensive Logging

Add debug output at every DataFrame transformation:
```rust
// After filtering
eprintln!("DEBUG: After filtering: first .ri = {:?}", ...);

// After remapping
eprintln!("DEBUG: After remapping: first .ri = {:?}", ...);

// After wrapping in GGRS DataFrame
eprintln!("DEBUG: After wrapping: first .ri = {:?}", ...);

// Before adding colors
eprintln!("DEBUG: Before colors: first .ri = {:?}", ...);

// After adding colors
eprintln!("DEBUG: After colors: first .ri = {:?}", ...);

// Just before return
eprintln!("DEBUG: Returning DataFrame: first .ri = {:?}", ...);
```

### 5. Alternative Approach: Fix in GGRS?

If the Polars API is proving difficult, consider modifying GGRS to accept an `.ri` offset parameter:
```rust
// Pass offset to GGRS
generator.set_ri_offset(12);  // For male page

// GGRS internally adjusts: ri_adjusted = ri - offset
```

This would keep the original `.ri` values but adjust them during rendering.

## Files Modified

1. **src/tercen/facets.rs**
   - Added `original_index: usize` field to `FacetGroup`
   - Updated all `FacetGroup` construction sites
   - Modified `load_with_filter()` to preserve original indices

2. **src/ggrs_integration/stream_generator.rs**
   - Line 157: Use `group.original_index` for axis range remapping
   - Line 280: Use `group.original_index` for extracting valid indices
   - Lines 954-992: Added `.ri` column remapping logic with extensive debug output
   - Lines 1169-1176: Added FINAL DataFrame `.ri` verification logging

## Test Command

```bash
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJodHRwOi8vMTI3LjAuMC4xOjU0MDAiLCJleHAiOjE3NzExOTI2OTAsImRhdGEiOntiZCI6IiIsInUiOiJ0ZXN0IiwiZSI6MTc3MTE5MjY5MDYwNn19.FbLXcNoW91Sl-PQ3hd-R5ousg7IL04O1dU0IyAiBCiA"
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="b9659735-27db-4480-b398-4e391431480f"
cargo run --profile dev-release --bin test_stream_generator
```

Or use:
```bash
./test_local.sh
```

## Expected Behavior

Both `plot_1.png` and `plot_2.png` should contain data points:
- **plot_1.png**: Female facets (Gender=female) with ~22K rows
- **plot_2.png**: Male facets (Gender=male) with ~22K rows

## Current Behavior

- **plot_1.png**: ✅ Contains data points (female facets working)
- **plot_2.png**: ❌ Empty (all male data points skipped due to `.ri` index mismatch)

## Key Insight from User

> "Is it possible it is trying to place the data points in the 'lower' facets, like it was a single plot? Or is the pixel position mapping correct?"

This question revealed that GGRS uses `.ri` values for two purposes:
1. **Panel routing**: Determining which facet panel to render into
2. **Y-axis dequantization**: Looking up Y-axis range for the row facet

Both fail when `.ri = 12` but `n_rows = 12` (causing `ri >= n_rows` check to fail).
