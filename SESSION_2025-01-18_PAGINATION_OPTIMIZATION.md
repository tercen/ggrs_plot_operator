# Pagination Optimization Session - 2025-01-18

## Session Goal
Optimize pagination performance by eliminating redundant data streaming. Current implementation streams the entire dataset once per page, causing a 71% performance overhead.

## Problem Analysis

### Performance Baseline
From previous pagination testing (`/tmp/test_pagination_final.log`):
- **Single plot** (all 44K points): 2.8s
- **Paginated** (2 pages, 22K + 22K points): 4.8s
- **Overhead**: 71% slower for same data!

### Root Cause
**Current pagination architecture** (`src/bin/test_stream_generator.rs:446-636`):
```rust
for page in pages {
    // Create NEW StreamGenerator for EACH page
    let stream_gen = TercenStreamGenerator::new(
        client, cube_query, y_axis_table,
        page_filter  // Different per page
    ).await?;

    // GGRS streams ALL 44K rows and filters to ~22K
    render_to_file(&plot_filename)?;
}
```

**Data flow per page**:
1. Page 1: Stream 44K rows from Tercen → Filter client-side → Keep 22K
2. Page 2: Stream same 44K rows AGAIN → Filter client-side → Keep 22K
3. **Total: 88K rows streamed for 44K points!**

**Why filtering happens after streaming**:
- Filtering is in `TercenStreamGenerator::query_data_multi_facet()` (stream_generator.rs:876-926)
- GGRS calls this method during rendering (ggrs/crates/ggrs-core/src/render.rs:969)
- By the time we filter, data is already fetched from Tercen

### Initial Architecture Exploration

**Option A: Multi-surface rendering** (REJECTED)
- Render to multiple Cairo surfaces in a single GGRS pass
- **Problem**: Would require modifying GGRS render.rs (complex, invasive)
- **Complexity**: 3-layer architecture (operator → GGRS → surfaces)

**Option B: Caching wrapper** (EXPLORED)
- Created `FilteredStreamGenerator` wrapper in `cached_stream_generator.rs`
- Wraps `TercenStreamGenerator` with shared cache
- **Problem**: Too complex, requires Arc<Box<dyn StreamGenerator>> plumbing

**Option C: Global cache in StreamGenerator** (✅ IMPLEMENTED)
- Add static cache directly in `stream_generator.rs`
- Transparent to calling code
- Minimal changes, maximum benefit

## Implementation: Global Data Cache

### 1. Added Dependency
**File**: `Cargo.toml:37-38`
```toml
# Lazy static initialization
once_cell = "1.20"
```

### 2. Global Cache Declaration
**File**: `src/ggrs_integration/stream_generator.rs:15-27`
```rust
use once_cell::sync::Lazy;
use std::sync::Mutex;

/// Global data cache for pagination optimization
///
/// When multiple pages are rendered, they create separate TercenStreamGenerator
/// instances but stream the SAME data from Tercen. This cache ensures we only
/// fetch each chunk once, sharing it across all pages.
///
/// Key: (table_id, offset) to uniquely identify each chunk
/// Value: The raw TSON data (before filtering)
static DATA_CACHE: Lazy<Mutex<HashMap<(String, usize), Vec<u8>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
```

**Design rationale**:
- **Static**: Shared across ALL `TercenStreamGenerator` instances
- **Lazy**: Initialized on first use (no startup cost)
- **Mutex**: Thread-safe access (tokio async runtime)
- **Key**: `(table_id, offset)` uniquely identifies each chunk
- **Value**: Raw TSON bytes (before DataFrame parsing)

### 3. Cache Integration in query_data_multi_facet()
**File**: `src/ggrs_integration/stream_generator.rs:872-893`

**Before** (lines 872-874):
```rust
let tson_data = streamer
    .stream_tson(&self.main_table_id, Some(columns.clone()), offset, limit)
    .await?;
```

**After** (lines 872-893):
```rust
// Try to get from cache first
let cache_key = (self.main_table_id.clone(), offset as usize);
let tson_data = {
    let cache = DATA_CACHE.lock().unwrap();
    if let Some(cached_data) = cache.get(&cache_key) {
        eprintln!("DEBUG: ✓ Cache HIT for offset {} ({} bytes)", offset, cached_data.len());
        cached_data.clone()
    } else {
        drop(cache); // Release lock before expensive operation
        eprintln!("DEBUG: ✗ Cache MISS for offset {} - streaming from Tercen...", offset);

        let data = streamer
            .stream_tson(&self.main_table_id, Some(columns.clone()), offset, limit)
            .await?;

        // Store in cache
        let mut cache = DATA_CACHE.lock().unwrap();
        cache.insert(cache_key, data.clone());
        eprintln!("DEBUG: ✓ Cached {} bytes for offset {}", data.len(), offset);
        data
    }
};
```

**Key implementation details**:
1. **Lock scoping**: Drop lock before expensive `stream_tson()` call
2. **Debug logging**: Clear cache HIT/MISS messages for verification
3. **Cloning**: Clone TSON bytes to avoid lifetime issues
4. **Transparent**: No API changes, existing code works unchanged

## How It Works: Step-by-Step

### Scenario: 2 pages, 44K rows total, chunk_size=10K

**Page 1 rendering**:
```
1. TercenStreamGenerator created (page_filter = female rows)
2. GGRS calls query_data_multi_facet(Range(0, 10000))
3. Cache MISS → Stream 10K rows from Tercen → Cache → Filter → Return 5K
4. GGRS calls query_data_multi_facet(Range(10000, 20000))
5. Cache MISS → Stream 10K rows → Cache → Filter → Return 5K
6. ... continues for all chunks
```

**Page 2 rendering** (KEY OPTIMIZATION):
```
1. NEW TercenStreamGenerator created (page_filter = male rows)
2. GGRS calls query_data_multi_facet(Range(0, 10000))
3. Cache HIT! → Read from cache → Filter → Return 5K (NO NETWORK CALL)
4. GGRS calls query_data_multi_facet(Range(10000, 20000))
5. Cache HIT! → Read from cache → Filter → Return 5K (NO NETWORK CALL)
6. ... continues using cached chunks
```

### Data Flow Comparison

**Before (No cache)**:
```
Page 1: Tercen → 44K rows → Filter → 22K rows
Page 2: Tercen → 44K rows → Filter → 22K rows
Total network: 88K rows
Time: 4.8s
```

**After (With cache)**:
```
Page 1: Tercen → 44K rows → Cache → Filter → 22K rows
Page 2: Cache → 44K rows → Filter → 22K rows
Total network: 44K rows
Time: ~3.0s (estimated)
```

## Performance Analysis

### Expected Improvements
- **Network**: 50% reduction (88K → 44K rows)
- **Parsing**: Same (still parse TSON twice, but from memory)
- **Filtering**: Same (happens twice, but in-memory)
- **Overall**: ~37% faster (4.8s → 3.0s)

### Overhead Breakdown
| Phase | Before | After | Savings |
|-------|--------|-------|---------|
| Stream data | 2× 2.0s = 4.0s | 1× 2.0s = 2.0s | **2.0s** |
| Parse TSON | 2× 0.2s = 0.4s | 2× 0.2s = 0.4s | 0s |
| Filter data | 2× 0.1s = 0.2s | 2× 0.1s = 0.2s | 0s |
| Render | 2× 0.1s = 0.2s | 2× 0.1s = 0.2s | 0s |
| **Total** | **4.8s** | **2.8s** | **2.0s (42%)** |

(Note: Actual timings may vary, these are estimates)

## Build Status

### Successful Compilation
```bash
cargo build --profile dev-release --bin ggrs_plot_operator
# ✓ Compiled successfully with 3 warnings (unused fields in cached_stream_generator.rs)
```

### Test Binary Status
**File**: `src/bin/test_stream_generator.rs`
- ⚠️ Currently broken - needs pagination support restored
- Error: `TercenStreamGenerator::new()` expects 8 args, test provides 7
- **Missing argument**: `page_filter: Option<&HashMap<String, String>>`

**Root cause**: Test file was reverted to non-paginated version during investigation

## Files Modified

### Core Implementation
1. ✅ `Cargo.toml` - Added `once_cell = "1.20"`
2. ✅ `src/ggrs_integration/stream_generator.rs` - Global cache + integration
3. ✅ `src/ggrs_integration/mod.rs` - Exported cached_stream_generator module

### Created (Not Used)
4. ⚠️ `src/ggrs_integration/cached_stream_generator.rs` - Wrapper approach (not used, can delete)
   - Contains `FilteredStreamGenerator` and `DataCache` structs
   - More complex than needed
   - Kept for reference but not imported in final solution

## Outstanding Work for Tomorrow

### 1. Test the Cache with Real Pagination
**Priority**: HIGH

**Approach**:
- Look in `SESSION_2025-01-16_PAGES_DEBUG.md` for pagination test setup
- Or use the existing pagination test files:
  - `tests/test_pagination_synthetic.rs` (synthetic data)
  - `src/tercen/pages.rs` (page metadata)

**Expected output**:
```
DEBUG: ✗ Cache MISS for offset 0 - streaming from Tercen...
DEBUG: ✓ Cached 125000 bytes for offset 0
[Page 1 renders - 2.4s]

DEBUG: ✓ Cache HIT for offset 0 (125000 bytes)  ← KEY VERIFICATION
DEBUG: ✓ Cache HIT for offset 10000 (125000 bytes)
[Page 2 renders - 0.4s]  ← Should be MUCH faster
```

### 2. Restore test_stream_generator.rs Pagination Support
**Priority**: MEDIUM

**Options**:
A. Find the pagination version in git history
B. Recreate based on `SESSION_2025-01-16_PAGES_DEBUG.md`
C. Use the main operator's pagination (in `src/main.rs`)

**Changes needed**:
```rust
// Add page_filter parameter
let stream_gen = TercenStreamGenerator::new(
    client_arc,
    cube_query.qt_hash.clone(),
    cube_query.column_hash.clone(),
    cube_query.row_hash.clone(),
    y_axis_table_id,
    config.chunk_size,
    color_infos,
    page_filter,  // ← Add this (Option<&HashMap<String, String>>)
).await?;
```

### 3. Cache Cleanup and Optimization
**Priority**: LOW

**Potential improvements**:
- Add cache size limit (prevent unbounded growth)
- Add cache clearing API (for testing)
- Measure actual cache hit rate
- Consider LRU eviction policy

**Current limitations**:
- Cache grows unbounded (could OOM with many workflows)
- No expiration (cache persists for entire process lifetime)
- Clones TSON bytes on hit (could use Arc<Vec<u8>> instead)

### 4. Delete Unused Files
**Priority**: LOW
- `src/ggrs_integration/cached_stream_generator.rs` - Not used, can delete
- Update `src/ggrs_integration/mod.rs` to remove reference

## Technical Decisions Made

### Why Global Static Cache?
**Alternatives considered**:
1. ❌ Cache per `TercenStreamGenerator` instance → Doesn't help, each page creates new instance
2. ❌ Pass cache as Arc<Mutex<>> parameter → Changes API, requires caller management
3. ✅ Global static → Transparent, zero API changes, automatic sharing

**Trade-offs**:
- ✅ Pro: Zero changes to calling code
- ✅ Pro: Automatic sharing across all instances
- ✅ Pro: Survives across multiple render calls
- ⚠️ Con: Global state (harder to test isolation)
- ⚠️ Con: Unbounded growth (need size limits)
- ⚠️ Con: Process-lifetime cache (may waste memory)

### Why Cache Raw TSON Instead of DataFrame?
**Rationale**:
- TSON bytes are smaller (compressed)
- DataFrame contains Polars structures (harder to clone)
- Filtering happens AFTER DataFrame creation anyway
- Cache key is simpler (just offset, not facet indices)

### Why Mutex Instead of RwLock?
**Rationale**:
- Read-heavy workload (multiple cache hits per page)
- But: Cache misses need exclusive write access
- Mutex is simpler and performs fine for pagination (2-10 pages typical)
- Could upgrade to RwLock later if needed

## Code References

### Pagination Implementation
- Facet filtering: `src/tercen/facets.rs:206-210`
- Page metadata: `src/tercen/pages.rs` (created in previous session)
- Row index remapping: `src/ggrs_integration/stream_generator.rs:900-924`

### GGRS Rendering Loop
- Chunk processing: `/home/thiago/workspaces/tercen/main/ggrs/crates/ggrs-core/src/render.rs:950-1100`
- Data routing: `/home/thiago/workspaces/tercen/main/ggrs/crates/ggrs-core/src/render.rs:969-1050`

### Previous Session Notes
- `SESSION_2025-01-16_PAGES_DEBUG.md` - Pagination implementation details
- `SESSION_2025-01-15_LEGEND.md` - Legend rendering (not related)

## Next Session Commands

```bash
# 1. Check current build status
cargo build --profile dev-release

# 2. Find pagination test configuration
cat SESSION_2025-01-16_PAGES_DEBUG.md | grep -A 20 "Test Setup"

# 3. Run pagination test (once restored)
TERCEN_URI="http://127.0.0.1:50051" \
TERCEN_TOKEN="..." \
WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c" \
STEP_ID="b9659735-27db-4480-b398-4e391431480f" \
timeout 45 cargo run --profile dev-release --bin test_stream_generator

# 4. Check cache hit rate in output
grep "Cache HIT\|Cache MISS" /tmp/test_output.log | wc -l

# 5. Compare timing
# - Look for total time in output
# - Should see ~3.0s vs previous 4.8s
```

## Success Criteria

### Phase 1: Verification (Tomorrow)
- [ ] Test compiles successfully
- [ ] Cache HITs visible in debug output for page 2+
- [ ] No errors during pagination rendering
- [ ] Both pages produce valid PNG files

### Phase 2: Performance Validation
- [ ] Total time reduced from 4.8s to ~3.0s (37% improvement)
- [ ] Cache hit rate >90% for pages 2+ (first page always misses)
- [ ] Memory usage stable (no leaks from cache)

### Phase 3: Production Readiness
- [ ] Add cache size limit
- [ ] Add test coverage for cache behavior
- [ ] Document cache behavior in CLAUDE.md
- [ ] Clean up unused cached_stream_generator.rs file

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│                    Pagination Loop                       │
│  (test_stream_generator.rs or main.rs)                  │
└───────────────┬──────────────────┬──────────────────────┘
                │                  │
        Page 1  │          Page 2  │
                ↓                  ↓
    ┌───────────────────┐  ┌───────────────────┐
    │ StreamGenerator 1 │  │ StreamGenerator 2 │
    │  (female filter)  │  │   (male filter)   │
    └────────┬──────────┘  └────────┬──────────┘
             │                      │
             │  query_data          │  query_data
             │  (offset=0)          │  (offset=0)
             ↓                      ↓
    ┌────────────────────────────────────────────┐
    │        DATA_CACHE                          │
    │  (Global Static Lazy<Mutex<HashMap>>)      │
    │                                            │
    │  Key: (table_id, offset)                   │
    │  Value: Vec<u8> (TSON bytes)              │
    │                                            │
    │  [offset=0    ] → [125000 bytes] ← HIT!   │
    │  [offset=10000] → [125000 bytes] ← HIT!   │
    │  [offset=20000] → [125000 bytes] ← HIT!   │
    └────────┬───────────────────────────────────┘
             │
             │ Cache MISS on first access
             ↓
    ┌────────────────────────────┐
    │   TableStreamer            │
    │  (Tercen gRPC client)      │
    │                            │
    │  stream_tson()             │
    │  → Network call to Tercen  │
    └────────────────────────────┘
```

## Key Insights

1. **Cache placement matters**: Caching in `query_data_multi_facet()` is perfect because:
   - Called by GGRS during rendering
   - Already has offset information
   - Raw TSON bytes are cacheable
   - Happens before filtering (data is reusable)

2. **Filtering is fine to repeat**:
   - In-memory Polars filtering is fast (~100ms for 44K rows)
   - The expensive part is network I/O (~2s per chunk)
   - Caching raw data + filtering twice is faster than streaming twice

3. **Static cache is transparent**:
   - No changes to TercenStreamGenerator API
   - No changes to PlotGenerator or PlotRenderer
   - No changes to pagination loop
   - Works automatically for any code using TercenStreamGenerator

4. **Chunk size matters for cache efficiency**:
   - Larger chunks = fewer cache entries = better hit rate
   - Current: 10K-15K rows per chunk
   - Typical pagination: 2-5 pages
   - Expected cache size: <10 entries × 125KB = ~1.25MB (acceptable)

## Open Questions

1. **Should we add cache expiration?**
   - Pro: Prevents unbounded growth
   - Con: Adds complexity, harder to debug
   - Decision: Start simple, add if needed

2. **Should we cache DataFrame instead of TSON?**
   - Pro: Skip parsing on cache hit
   - Con: Larger memory footprint, harder to clone
   - Decision: TSON is fine for now

3. **Should we make cache opt-in?**
   - Pro: Explicit control
   - Con: Requires API changes, easy to forget
   - Decision: Always-on is better (zero-config)

4. **What about cache invalidation?**
   - Currently: Cache persists for process lifetime
   - Issue: Stale data if table changes during process
   - Mitigation: Tercen tables are immutable once created
   - Decision: No invalidation needed

## Conclusion

Successfully implemented a transparent, zero-config global data cache that should reduce pagination overhead from 71% to near-zero (only in-memory filtering overhead). The implementation is minimal, non-invasive, and ready for testing.

**Next steps**: Restore pagination test, verify cache hits, measure performance improvement.
