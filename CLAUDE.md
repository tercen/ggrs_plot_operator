# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

The **ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with the Tercen platform. It receives tabular data through the Tercen gRPC API, generates high-performance plots using GGRS, and returns PNG images back to Tercen for visualization.

## ⚠️ IMPORTANT: Current Status & Known Issues

**Phase**: 🚧 Version 0.0.2 IN PROGRESS - Faceting Support | **Status**: ✅ Phase 1 & 2 COMPLETE, Phase 3 pending

**Deployment Status**: ✅ Working (with logging disabled)

**Latest Changes (2025-01-13)**:
- ✅ **Phase 1 COMPLETE** - Operator reads all data correctly (facet counts, labels, Y ranges from metadata)
- ✅ **Phase 2 COMPLETE** - GGRS bulk streaming implemented
  - Modified `render.rs` and `render_v2.rs` to use `query_data_multi_facet()`
  - Added `filter_by_rows()` method to GGRS DataFrame for facet filtering
  - GGRS now filters data internally by `.ri` and `.ci` indices
  - Both GGRS and operator compile cleanly with zero warnings
- ✅ **Faceting support implemented** - Row/column/grid faceting now enabled
- ✅ **Plot size increased** - Default 2000×2000px for better facet visibility
- 🎯 **Next: Phase 3** - Test full multi-facet rendering with real workflow data

**Critical Issue**: EventService returns `UnimplementedError` in production
- **Impact**: All logging via TercenLogger is disabled
- **Workaround**: All `logger.log()` calls commented out in main.rs
- **Details**: See `DEPLOYMENT_DEBUG.md`

**Build Profile**: Using `--profile dev-release` (4-5 min) instead of `--release` (12+ min)
- Adequate performance for development/testing
- Switch to `--release` for production releases if needed

**What's Working**:
- ✅ gRPC connection and authentication
- ✅ TaskService (get, runTask)
- ✅ TableSchemaService (streamTable)
- ✅ FileService (upload) - for result uploads
- ✅ Full plot generation pipeline (475K rows → PNG in 9.5s)
- ✅ GPU acceleration (OpenGL backend: 0.5s vs CPU: 3.1s)
- ✅ Columnar architecture with Polars
- ✅ TSON streaming and dequantization
- ✅ Proto files via submodule (tercen_grpc_api)
- ✅ **Result upload with full Tercen model format**

**What's Blocked**:
- ❌ EventService.create() - All logging disabled

**Completed**:
- ✅ CI/CD release workflow (tag-based Docker build and publish)

---

## 📋 Current Issue Logbook (2025-01-09)

### Issue: Result Upload "columns missing" Error

**Error**: `Worker process failed - Tbl -- from_tson_table -- columns missing`

**Context**: Phase 8 result upload implementation. Operator generates PNG successfully, but upload via `TableSchemaService.uploadTable()` fails during Sarno worker processing.

**Investigation Timeline**:

1. **Initial Implementation** (❌ Failed)
   - Tried: Added `.ci` and `.ri` columns (int32, value 0) - thought they were mandatory
   - Result: Still failed with same error
   - Reasoning: Assumed these were required for facet linking based on initial error message

2. **Namespace Prefixing** (❌ Failed)
   - Tried: Added namespace prefix to all non-dot columns (`ds10.filename`, `ds10.mimetype`)
   - Result: Still failed
   - Reasoning: User pointed out all non-dot columns MUST have namespace prefix

3. **Sarno Format Discovery** (❌ Failed)
   - Tried: Simplified to `{"cols": [...]}` structure with TSON type integers
   - Used wrong type codes: 7, 8, 9, 10
   - Result: Error "expected type as LSTSTR,LSTU8, ... ,LSTF64"
   - Reasoning: User provided analysis showing Sarno expects simple format, not OperatorResult wrapper

4. **TSON Type Code Fix** (❌ Failed)
   - Tried: Updated to correct TsonSpec constants (105=int32, 106=int64, 111=float64, 112=string)
   - Result: Still fails (testing in progress)
   - Reasoning: Found correct type codes in dtson/lib/src/tson.dart

5. **Compare with R Implementation** (✅ Current)
   - Investigation: Analyzed R's `file_to_tercen` from teRcen package
   - Key findings:
     - R does NOT include `.ci`/`.ri` in file_to_tercen output
     - R uses plain `filename`, `mimetype`, `.content` columns initially
     - Namespace is added LATER in operator flow (separate function)
     - R includes `plot_width` and `plot_height` (numeric/double)
   - Actions taken:
     - ✅ Removed `.ci` and `.ri` columns
     - ✅ Added namespace prefix back to filename/mimetype
     - ✅ Added `plot_width` and `plot_height` columns (f64)
   - Current structure:
     ```
     .content              (string - base64)
     {ns}.filename         (string)
     {ns}.mimetype         (string)
     {ns}.plot_width       (f64)
     {ns}.plot_height      (f64)
     ```

6. **Proto Files Submodule** (✅ Completed)
   - Action: Replaced local `protos/` with `tercen_grpc_api` submodule
   - Reasoning: User noted C# client uses submodule; ensures sync with canonical API
   - Status: ✅ Build verified, submodule working

7. **TypedData Discovery** (🧪 Testing)
   - Investigation: User provided Sarno API internals showing it expects TypedData, not generic lists
   - Key finding: Sarno expects `data` field to contain **TypedData** (Uint32List, Float64List, CStringList), NOT generic LST!
   - Root cause identified: We were encoding as `TsonValue::LST(vec![TsonValue::F64(...)])` (generic list)
   - Fix applied: Changed to typed lists:
     - String: `TsonValue::LSTSTR(vec![String].into())` for CStringList
     - Float64: `TsonValue::LSTF64(vec![f64])` for Float64List
     - Int32: `TsonValue::LSTI32(vec![i32])` for Int32List
     - Int64: `TsonValue::LSTI64(vec![i64])` for Int64List (already correct)
   - Files modified: `src/tercen/table_convert.rs`
   - Also updated: `Cargo.lock` to latest ggrs-core (08cbf29) which removes debug messages
   - Status: ✅ Compiles cleanly, deployed to test

### ✅ RESOLUTION (2025-01-09)

**Root Cause**: We were sending a **simplified Sarno format** (`{"cols": [...]}`) instead of the **full Tercen model format**.

**The Fix**:
- Changed from: `{"cols": [...], "meta_data": {...}}` (custom simplified format)
- Changed to: `{"kind": "Table", "nRows": ..., "properties": {...}, "columns": [...]}` (full Tercen model)

**Key Insight from Python Example**:
User provided working Python OperatorResult structure showing:
- `properties`: `{"kind": "TableProperties", "name": "", ...}` - Empty string is fine!
- Each column has: `id`, `name`, `type`, `nRows`, `size`, `metaData`, `cValues`, `values`
- The issue was NOT empty string for `properties.name`
- The issue was NOT missing TypedData (though that was also needed earlier)
- **The real issue**: We weren't sending the full Tercen model structure at all

**Python toJson/fromJson**:
- Python has auto-generated base classes (BaseObject.py) with toJson/fromJson methods
- These are custom to Tercen's JSON format (not standard protobuf JSON)
- Uses `kind` fields and camelCase (e.g., `nRows` not `n_rows`)
- **For Rust**: No equivalent auto-generated code exists in Tercen ecosystem
- We must manually construct TSON using the same structure as Python toJson

**Final Working Structure**:
```json
{
  "kind": "OperatorResult",
  "tables": [
    {
      "kind": "Table",
      "nRows": 1,
      "properties": {
        "kind": "TableProperties",
        "name": "",
        "sortOrder": [],
        "ascending": true
      },
      "columns": [
        {
          "kind": "Column",
          "id": "",
          "name": ".content",
          "type": "string",
          "nRows": 1,
          "size": 1,
          "metaData": {...},
          "cValues": {"kind": "CValues"},
          "values": [<base64-data>]
        }
      ]
    }
  ],
  "joinOperators": []
}
```

**Files Modified**:
- `src/tercen/result.rs` - Full Tercen model TSON serialization
- `src/tercen/table_convert.rs` - TableProperties with empty name (matches Python)
- `src/main.rs` - Pass plot dimensions to save_result
- `build.rs` - Use tercen_grpc_api submodule
- `.gitmodules` - Added tercen_grpc_api submodule

---

## 📋 Faceting Implementation (2025-01-12) - Version 0.0.2

### Implementation Summary

**Goal**: Enable multi-facet plots where each row facet (`.ri`) has its own panel and independent Y-axis range.

**Changes Made**:

1. **FacetSpec Configuration** (`src/ggrs_integration/stream_generator.rs`):
   - Changed from hardcoded `FacetSpec::none()` to dynamic detection
   - Uses `.ri` and `.ci` as faceting variables (index-based faceting)
   - Supports three modes:
     - **Row faceting**: `FacetSpec::row(".ri").scales(FacetScales::FreeY)`
     - **Column faceting**: `FacetSpec::col(".ci")`
     - **Grid faceting**: `FacetSpec::grid(".ri", ".ci").scales(FacetScales::FreeY)`
   - Each row facet gets independent Y-axis scaling

2. **Plot Size Increase** (`src/config.rs`):
   - Changed default from 800×600px to 2000×2000px
   - Better visibility for multi-facet layouts

### How It Works

**Data Flow**:
1. **Setup Phase**:
   - TercenStreamGenerator loads facet metadata from row.csv/column.csv
   - Returns correct counts via `n_row_facets()`, `n_col_facets()`
   - Returns facet labels via `query_row_facet_labels()`
   - Returns per-facet Y ranges via `query_y_axis(col_idx, row_idx)`

2. **Rendering Phase** (Current - Per-Facet Chunking):
   ```
   for row_idx in 0..n_row_facets {
       for col_idx in 0..n_col_facets {
           for chunk in chunks {
               data = query_data_chunk(col_idx, row_idx, chunk)  // Filters by .ri == row_idx
               dequantize(.xs/.ys → .x/.y using Y range for this facet)
               render(data, panel[row_idx, col_idx])
           }
       }
   }
   ```

**Key Insight**:
- Data contains `.ci`, `.ri` indices (0, 1, 2, ...) not actual facet variable values
- `.ri = 0` means "first row in row.csv", `.ri = 1` means "second row in row.csv"
- GGRS uses `.ri` as a faceting variable and groups by its values
- No data scanning needed - facet counts come from metadata table sizes

### Performance Characteristics

**Current Implementation (Per-Facet Chunking)**:
- ✅ **Works correctly** - Each facet renders in its own panel
- ✅ **Independent Y-axes** - Each row facet has its own Y range
- ⚠️ **Performance overhead** - Data re-streamed N times for N facets
- Example: 10 row facets × 475K rows = 4.75M rows transferred (10x redundancy)

**Future Optimization (Bulk Streaming)**:
- 🎯 Use `query_data_multi_facet()` instead (already implemented in TercenStreamGenerator)
- Stream data once with all `.ci`, `.ri`, `.xs`, `.ys` columns
- Route points to panels using indices: `panel[data.ri]`
- Requires GGRS render.rs modification to support bulk mode
- Would reduce 10 facets × 475K = 4.75M to just 475K rows (single pass)

### Files Modified

- `src/ggrs_integration/stream_generator.rs` - FacetSpec creation logic (lines 122-139, 182-199)
- `src/config.rs` - Default plot dimensions (lines 40-41)

### Testing Status

- ✅ Code compiles cleanly
- ✅ Clippy passes with zero warnings
- ⚠️ **Needs testing** with actual faceted workflow

**Next Steps for Testing**:
1. Test with workflow containing row facets
2. Verify each facet renders in separate panel with correct Y range
3. Measure performance impact of per-facet chunking
4. Consider bulk streaming optimization in GGRS render.rs

**Current Git State** (2025-01-13):
- Modified: `CLAUDE.md`, `README.md` - Documentation updates
- Modified: `src/config.rs` - Increased default plot size to 2000×2000px
- Modified: `src/ggrs_integration/stream_generator.rs` - Added faceting support
- Modified: Test outputs (`plot.png`, memory usage files) - Normal test artifacts

---

## Quick Reference

### Common Commands

```bash
# Build (USE THIS for dev/testing)
cargo build --profile dev-release  # 4-5 min, optimized enough
cargo build --release               # 12+ min, only for production

# Test and Quality (RUN THESE before considering code complete!)
cargo fmt                           # Format code
cargo fmt --check                   # Check formatting
cargo clippy -- -D warnings         # Lint with zero warnings
cargo build                         # Verify compilation
cargo test                          # Run tests

# Local testing (RECOMMENDED method)
./test_local.sh                     # Uses test_stream_generator binary
# Or manually:
TERCEN_URI="http://127.0.0.1:50051" \
TERCEN_TOKEN="your_token" \
WORKFLOW_ID="workflow_id" \
STEP_ID="step_id" \
cargo run --profile dev-release --bin test_stream_generator

# Docker
docker build -t ggrs_plot_operator:local .
docker run --rm ggrs_plot_operator:local

# CI/CD
git push origin main                         # Triggers CI workflow
git tag 0.1.0 && git push origin 0.1.0      # Create release (NO 'v' prefix)
```

See `BUILD.md` for comprehensive build instructions.
See `TEST_LOCAL.md` and `WORKFLOW_TEST_INSTRUCTIONS.md` for testing details.

## Quick Debugging

**Operator not connecting?**
- Check `TERCEN_URI` and `TERCEN_TOKEN` env vars
- Verify token format (should start with `eyJ`)
- Test connectivity: `curl -v $TERCEN_URI`

**Build failing?**
- Run: `cargo clean && cargo build --profile dev-release`
- Check proto submodule: `git submodule update --init --recursive`
- Update deps: `cargo update`

**Tests failing?**
- Use test script: `./test_local.sh`
- Check test_stream_generator binary exists
- Verify WORKFLOW_ID and STEP_ID are valid

**Faceting issues?**
- Verify `.ci` and `.ri` columns exist in data
- Check facet metadata tables (column.csv, row.csv) are populated
- Confirm row/column counts match data indices
- Look for "FacetSpec" in logs to see what mode was detected

**See `DEPLOYMENT_DEBUG.md` for detailed troubleshooting.**

## Module Structure

```
ggrs_plot_operator/
├── src/
│   ├── main.rs                      # Entry point (⚠️ logging disabled!)
│   ├── tercen/                      # Pure Tercen gRPC client (future crate)
│   │   ├── client.rs               # TercenClient with auth
│   │   ├── table.rs                # TableStreamer (chunked streaming)
│   │   ├── tson_convert.rs         # TSON → Polars DataFrame (columnar)
│   │   ├── facets.rs               # Facet metadata loading
│   │   ├── result.rs               # Result upload (Phase 8)
│   │   ├── logger.rs               # TercenLogger (currently disabled)
│   │   └── error.rs                # TercenError types
│   ├── ggrs_integration/           # GGRS-specific code
│   │   └── stream_generator.rs     # TercenStreamGenerator impl
│   └── bin/
│       └── test_stream_generator.rs # Test binary (USE THIS for testing!)
├── tercen_grpc_api/                # Git submodule (canonical proto files)
│   └── protos/
│       ├── tercen.proto            # Service definitions
│       └── tercen_model.proto      # Data model definitions
├── build.rs                        # Proto compilation (references submodule)
├── Cargo.toml                      # Dependencies (ggrs-core from GitHub)
└── .gitmodules                     # Submodule configuration
```

**Key Design Principles**:
- `src/tercen/` has NO GGRS dependencies for future extraction as separate crate
- Proto files via submodule ensure sync with canonical Tercen gRPC API
- GGRS library also from GitHub (`github.com/tercen/ggrs`)

**Key Files to Read**:
- `DEPLOYMENT_DEBUG.md` - ⚠️ Current issues and workarounds
- `docs/09_FINAL_DESIGN.md` - Complete architecture
- `docs/10_IMPLEMENTATION_PHASES.md` - Implementation roadmap
- `src/ggrs_integration/stream_generator.rs` - Core integration
- `src/tercen/tson_convert.rs` - Columnar data conversion

## High-Level Architecture

### Three-Layer Design

1. **gRPC Client Layer** (`src/tercen/`)
   - TercenClient: Connection and authentication (Bearer token)
   - TableStreamer: Chunked data streaming via ReqStreamTable
   - Services: TaskService, TableSchemaService, FileService, EventService (disabled)
   - Uses: tonic (~0.14), prost (~0.14), tokio (~1.49)

2. **Data Transformation Layer** (Columnar Architecture - CRITICAL!)
   - **Pure Polars operations** - NO row-by-row Record construction
   - TSON → Polars DataFrame (columnar → columnar)
   - Polars lazy API with predicate pushdown: `col(".ci").eq().and(col(".ri").eq())`
   - Zero-copy operations, `vstack_mut()` for chunk concatenation
   - Quantized coordinates: `.xs`/`.ys` (uint16) → `.x`/`.y` (f64) dequantization

3. **GGRS Integration Layer** (`src/ggrs_integration/`)
   - TercenStreamGenerator: Implements GGRS `StreamGenerator` trait
   - Lazy data loading per facet cell
   - Progressive rendering with chunk-by-chunk dequantization
   - GPU backend (OpenGL): 10x faster than CPU (0.5s vs 3.1s for 475K points)

### Data Flow (Current Implementation)

```
1. TercenStreamGenerator::new()
   └─ Connect to Tercen via gRPC
   └─ Load facet metadata (column.csv, row.csv - small tables)
   └─ Load/compute axis ranges for dequantization
   └─ Create Aes mapping to .x and .y

2. GGRS calls query_data_chunk(col_idx, row_idx) per facet cell
   └─ Stream TSON in chunks via ReqStreamTable (offset + limit)
   └─ Parse TSON → Polars DataFrame (COLUMNAR!)
   └─ Filter: col(".ci").eq(col_idx).and(col(".ri").eq(row_idx))
   └─ Concatenate chunks with vstack_mut()
   └─ Returns quantized coordinates: .xs/.ys (uint16 as i64)

3. GGRS dequantizes in render pipeline (render.rs)
   └─ Calls dequantize_chunk(df, x_range, y_range)
   └─ Formula: value = (quantized / 65535) * (max - min) + min
   └─ Creates .x and .y columns with actual values

4. GGRS renders plot
   └─ Auto-converts i64 → f64 for coordinates
   └─ GPU (OpenGL): 0.5s for 475K points, 162 MB peak
   └─ CPU (Cairo): 3.1s for 475K points, 49 MB peak

5. Phase 8: Upload to Tercen ✅
   └─ Encode PNG to base64
   └─ Create result DataFrame with .content, filename, mimetype, plot_width, plot_height
   └─ Upload via TableSchemaService with full Tercen model format
   └─ Update task with fileResultId
```

### Data Structure

**Main data** (TSON format):
```csv
.ci,.ri,.xs,.ys,sp,...
0,0,12845,15632,"B",...
```
- `.ci`: Column facet index (i64)
- `.ri`: Row facet index (i64)
- `.xs`, `.ys`: Quantized coordinates (uint16 as i64, range 0-65535)

**Column facets** (`column.csv`):
```csv
sp
B
O
```

**Row facets** (`row.csv`):
```csv
variable,sex
BD,F
BD,M
```

## Key Technical Decisions

### Columnar Architecture (CRITICAL!)

**Never build row-by-row structures. Always stay columnar.**

- ✅ **DO**: Use Polars lazy API with predicate pushdown
- ✅ **DO**: Use `vstack_mut()` for chunk concatenation
- ✅ **DO**: Zero-copy operations where possible
- ❌ **DON'T**: Build `Vec<Record>` or `HashMap<String, Value>` row-by-row
- ❌ **DON'T**: Iterate rows to construct data structures

**Why**: 10x+ performance improvement, lower memory usage, aligns with Polars/GGRS architecture.

### Memory Efficiency

- **Streaming**: Don't load entire table, process in chunks (default: 15K rows)
- **Lazy Faceting**: Only load data for facet cells being rendered
- **Schema-Based Limiting**: Use table schema row count to prevent infinite loops
- **Quantized Coordinates**: Transmit 2 bytes/coordinate, dequantize on demand
- **Progressive Dequantization**: Process and discard chunks immediately

**Results**: 475K rows in 3.1s (CPU) or 0.5s (GPU), memory stable at 49MB (CPU) or 162MB (GPU)

### GPU Backend

- **Configuration**: `operator_config.json` - `"backend": "cpu"` or `"gpu"`
- **OpenGL vs Vulkan**: OpenGL selected (162 MB vs 314 MB, 49% reduction)
- **Performance**: 10x speedup for same quality
- **Trade-off**: 3.3x memory overhead acceptable for 10x speed

### NO FALLBACK STRATEGIES (Critical Development Principle)

**Never implement fallback logic unless explicitly requested by the user.**

```rust
// ❌ BAD: Fallback pattern
if data.has_column(".ys") {
    use_ys()
} else {
    use_y()
}

// ✅ GOOD: Trust the specification
data.column(".ys")  // User said .ys exists
```

**Rationale**:
- Fallbacks mask bugs instead of fixing them
- Add unnecessary complexity
- Hurt performance (checking multiple code paths)
- Make behavior ambiguous

**Only use fallbacks when**:
1. User explicitly requests backward compatibility
2. Implementing error recovery at system boundaries (user input validation)

**When the user says something exists**, trust that specification completely. If it doesn't work, it's a bug to fix, not a reason to add fallbacks.

## Core Dependencies

```toml
# Async runtime
tokio = "1.49"              # Multi-threaded async
tokio-stream = "0.1"        # Stream utilities

# gRPC and Protocol Buffers
tonic = "0.14"              # gRPC client (TLS support)
prost = "0.14"              # Protobuf serialization

# Data processing (CRITICAL!)
polars = "0.51"             # Columnar DataFrame operations
rustson = { git = "..." }   # TSON parsing (Tercen format)

# GGRS plotting
ggrs-core = { git = "https://github.com/tercen/ggrs", features = ["webgpu-backend", "cairo-backend"] }

# Error handling
thiserror = "1.0"           # Error derive macros
anyhow = "1.0"              # Error context

# Utilities
serde = "1.0"               # Serialization
base64 = "0.22"             # PNG encoding
```

## Implementation Status

**Phase 7**: ✅ COMPLETE - Full plot generation working
**Phase 8**: ✅ COMPLETE - Result upload working!

### Completed Features

1. ✅ Pure Polars columnar operations
2. ✅ TSON → Polars DataFrame conversion with schema-based limiting
3. ✅ Polars lazy filtering with predicate pushdown
4. ✅ Chunked streaming with vstack_mut() concatenation
5. ✅ Quantized coordinates (.xs/.ys) with dequantization in GGRS
6. ✅ Axis range loading from Y-axis table or computation fallback
7. ✅ Full plot rendering: 475K rows → PNG in 9.5s (CPU) or 0.5s (GPU)
8. ✅ GPU acceleration with OpenGL backend
9. ✅ Configuration system (operator_config.json)
10. ✅ Test binary (test_stream_generator)
11. ✅ **Result upload with full Tercen model format**
12. ✅ **FileService integration for result uploads**
13. ✅ **Base64 PNG encoding with 1MB chunking support**
14. ✅ **Two-path upload logic (empty fileResultId vs existing)**

### Phase 8 Completion

1. ✅ Encode PNG to base64
2. ✅ Create result DataFrame with `.content`, `.ci`, `.ri`, `{ns}.filename`, `{ns}.mimetype`, `{ns}.plot_width`, `{ns}.plot_height`
3. ✅ Full Tercen model TSON serialization (matching Python toJson structure)
4. ✅ FileService.upload() implementation
5. ✅ Task update with fileResultId
6. ✅ Result appears correctly in Tercen UI
7. ✅ Tested full operator lifecycle end-to-end

**Note**: Result structure uses full Tercen model format with `kind` fields, `properties`, `metaData`, and `cValues` to match Python/Dart serialization.

### Phase 9: CI/CD Release Workflow ✅ COMPLETE

1. ✅ Tag-based Docker build (semantic versioning)
2. ✅ Automatic operator.json updates with version tag (in-place, not committed)
3. ✅ Docker image tagging and publishing to ghcr.io
4. ✅ GitHub release creation with changelog
5. ✅ Build attestation and provenance

**Release Workflow Fix (2025-01-12)**:
- **Issue**: Original workflow tried to commit and push to immutable tags, causing Git conflicts
- **Fix**: Removed commit/push steps - operator.json is updated in-place for Docker build only
- **Pattern**: Matches tercen/plot_operator release workflow (no tag modification)

**All core phases complete!** The operator is production-ready.

### Future Enhancements (See README.md Roadmap)

**Version 0.0.2** (Current):
- ✅ Scatter plot with multiple facets (row/column/grid faceting with FreeY scales)
- 🎯 Optimize bulk streaming for multi-facet (currently uses per-facet chunking)
- 🎯 Add plot legend
- 🎯 Add support for colors
- 🎯 Review and optimize dependencies

**Version 0.0.3** (Future):
- Operator properties for plot width/height
- Switching between GPU/CPU via operator config
- Support for minimal and white themes

**Version 0.0.4** (Future):
- Textual elements (axis labels, legend, title)
- Manual axis ranges
- Additional output formats: SVG, PDF

**Other Enhancements**:
- Re-enable EventService logging when available
- Profile and optimize hot paths

## Development Workflow

### Pre-Commit Checklist (MANDATORY!)

**Before considering ANY code change complete, run these checks:**

```bash
# 1. Format check (must pass)
cargo fmt --check

# 2. Apply formatting if needed
cargo fmt

# 3. Clippy with zero warnings (must pass)
cargo clippy -- -D warnings

# 4. Build check (must compile)
cargo build --profile dev-release

# 5. Test check (when tests exist)
cargo test
```

**NEVER consider a code update complete until all checks pass.** CI will fail otherwise.

### Testing Workflow

**⚠️ CRITICAL: ALWAYS use credentials from test_local.sh**

When testing or investigating issues, you **MUST** use the exact TERCEN_URI, TERCEN_TOKEN, WORKFLOW_ID, and STEP_ID from `test_local.sh`. **NEVER** use different credentials or make up test values, as this wastes time and tokens testing the wrong workflow.

**Recommended Method** (workflow/step-based, like Python's OperatorContextDev):

```bash
# 1. Edit test_local.sh with your WORKFLOW_ID and STEP_ID (if needed)
vim test_local.sh

# 2. Run test
./test_local.sh

# 3. Check output and memory usage
# Script prints memory stats and saves plot
```

**Manual Method**:

```bash
# Set environment
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAi..."  # Get from Tercen
export WORKFLOW_ID="workflow_id"
export STEP_ID="step_id"

# Run test binary (not main!)
cargo run --profile dev-release --bin test_stream_generator
```

### Git Policy for Claude Code

Claude Code should NOT create commits or push unless explicitly requested:
- ❌ Never use `git commit` without explicit user request
- ❌ Never use `git push` without explicit user request
- ✅ Run quality checks: `cargo fmt`, `cargo clippy`, `cargo build`, `cargo test`
- ✅ Use `git status` and `git diff` to show changes
- ✅ Stage changes with `git add` if requested
- ✅ Create commits only when user explicitly asks

**Default behavior: The user handles commits and pushes manually.**

## Code Quality Standards

- Follow Rust API guidelines
- Use `rustfmt` for formatting: `cargo fmt`
- Pass `clippy` lints with zero warnings: `cargo clippy -- -D warnings`
- Write rustdoc comments for all public APIs
- Use semantic commit messages (when user commits)

**Before ANY code is considered complete**:
1. Run `cargo fmt --check` (must pass)
2. Run `cargo clippy -- -D warnings` (zero warnings required)
3. Run `cargo build --profile dev-release` (must compile)
4. Run `cargo test` (when tests exist, must pass)

CI will fail if these checks don't pass.

## Proto Files (Submodule)

**Important**: Proto files are managed via git submodule, NOT copied locally.

The `tercen_grpc_api` submodule references the canonical proto definitions:
- Repository: https://github.com/tercen/tercen_grpc_api
- Path: `tercen_grpc_api/protos/`
- Files:
  - `tercen.proto`: Service definitions (TaskService, TableSchemaService, FileService)
  - `tercen_model.proto`: Data model definitions (ETask, ComputationTask, CrosstabSpec)

**Why submodule?**
- Ensures sync with canonical Tercen gRPC API
- Same approach as C# client (TercenCSharpClient)
- Automatic updates when proto definitions change

**Setup** (for new clones):
```bash
git submodule update --init --recursive
```

Proto files are compiled at build time via `build.rs`:
```rust
tonic_prost_build::configure()
    .build_server(false)      // Client only
    .build_transport(false)   // Avoid naming conflicts
    .compile_protos(
        &[
            "tercen_grpc_api/protos/tercen.proto",
            "tercen_grpc_api/protos/tercen_model.proto",
        ],
        &["tercen_grpc_api/protos"]
    )
```

## Documentation References

### Primary Documentation (Read These First)

- **`DEPLOYMENT_DEBUG.md`** ⚠️ - Current deployment issues and debugging
- **`docs/09_FINAL_DESIGN.md`** ⭐ - Complete architecture and design
- **`docs/10_IMPLEMENTATION_PHASES.md`** - Implementation roadmap
- **`docs/GPU_BACKEND_MEMORY.md`** - GPU backend optimization
- **`BUILD.md`** - Comprehensive build guide
- **`TEST_LOCAL.md`** - Local testing procedures
- **`WORKFLOW_TEST_INSTRUCTIONS.md`** - Workflow/step-based testing

### Supporting Documentation

- `docs/03_GRPC_INTEGRATION.md` - gRPC API specifications
- `docs/08_SIMPLE_STREAMING_DESIGN.md` - Streaming architecture concepts
- `src/tercen/README.md` - Library extraction plan

### External Resources

- [Tercen gRPC API](https://github.com/tercen/tercen_grpc_api)
- [Tercen C# Client](https://github.com/tercen/TercenCSharpClient) - Reference implementation
- [GGRS Library](https://github.com/tercen/ggrs)

---

## Appendix: Detailed Technical Information

### Columnar Architecture Deep Dive

The codebase underwent a complete migration from row-oriented Record processing to pure columnar Polars operations.

**Before (Row-Oriented)**:
```rust
// ❌ OLD: Build records row-by-row
let mut records = Vec::new();
for row_idx in 0..df.height() {
    let mut record = Record::new();
    for col_name in df.column_names() {
        let value = df.get_value(row_idx, col_name)?;
        record.insert(col_name.to_string(), value);
    }
    records.push(record);
}
```

**After (Columnar)**:
```rust
// ✅ NEW: Pure columnar operations
let polars_df = polars_df
    .lazy()
    .filter(col(".ci").eq(lit(col_idx as i64))
        .and(col(".ri").eq(lit(row_idx as i64))))
    .collect()?;
```

**Key Changes**:

1. **TSON Parsing** (`src/tercen/tson_convert.rs`):
   - Converts TSON columnar arrays directly to Polars `Series` → `Column`
   - NO intermediate row-by-row processing
   - Stays columnar: TSON → Polars → GGRS

2. **Filtering** (`src/ggrs_integration/stream_generator.rs`):
   - Uses Polars lazy API with predicate pushdown
   - `col(".ci").eq(lit(idx)).and(col(".ri").eq(lit(idx)))`
   - Eliminates manual row iteration

3. **Concatenation**:
   - Uses `vstack_mut()` for columnar chunk appending
   - NO record-by-record merging

4. **Type Coercion** (`ggrs-core/src/data.rs`):
   - `column_as_f64()` auto-converts i64 → f64
   - Handles quantized coordinates

5. **Dequantization** (`ggrs-core/src/render.rs`):
   - `dequantize_chunk()` converts `.xs`/`.ys` → `.x`/`.y`
   - Formula: `value = (quantized / 65535.0) * (max - min) + min`
   - Called progressively per chunk

### Performance Results

**Test Dataset**: 475,688 rows

**CPU Backend (Cairo)**:
- Total time: 3.1s (data fetch + dequantization + rendering)
- Memory: Stable at 49MB peak
- Plot output: 59KB PNG
- Throughput: ~153K rows/second

**GPU Backend (OpenGL)**:
- Total time: 0.5s (data fetch + dequantization + rendering)
- Memory: Stable at 162MB peak (3.3x overhead)
- Plot output: 59KB PNG (identical quality)
- Throughput: ~951K rows/second
- **Speedup**: 10x faster than CPU

**OpenGL vs Vulkan**:
- OpenGL: 162 MB peak (selected)
- Vulkan: 314 MB peak (rejected)
- **Memory Savings**: 49% reduction with OpenGL

### Tercen Concepts

**Crosstab Projection**:
- **Row factors**: Faceting rows (`.ri` column)
- **Column factors**: Faceting columns (`.ci` column)
- **X/Y axes**: Plot coordinates (`.x`, `.y` after dequantization)
- **Color/Label factors**: Aesthetics (e.g., `sp` column)

**Task Lifecycle**:
1. Operator polls `TaskService.waitDone()` or receives task ID
2. Update task state to `RunningState` (if using task-based approach)
3. Execute computation (fetch data, generate plot, upload)
4. Send progress updates via `TaskProgressEvent` (currently disabled)
5. Update task state to `DoneState` or `FailedState`

**Data Streaming**:
- Use `TableSchemaService.streamTable()` with TSON format
- Receives data in chunks (Vec<u8>)
- Parse with rustson library
- Process chunks incrementally with Polars

**File Upload** (Phase 8 ✅ COMPLETE):
- Encode PNG to base64 ✅
- Create result table with `.content`, `{ns}.filename`, `{ns}.mimetype`, `{ns}.plot_width`, `{ns}.plot_height` columns ✅
- Upload via `TableSchemaService.save()` with full Tercen model TSON format ✅
- Result appears correctly in Tercen UI ✅
- **Note**: Results use full Tercen model structure with `kind` fields, `properties`, `metaData`, and `cValues`

### Build System

**Build Profiles**:
- `dev` (default): Fast compilation, no optimization
- `dev-release`: Balanced (4-5 min build, good performance) - **USE THIS**
- `release`: Full optimization (12+ min build) - Only for production

**Dockerfile**:
- Multi-stage build (builder + runtime)
- Uses `--profile dev-release` for faster CI builds
- Runtime: Debian bookworm-slim (~120-150 MB)
- jemalloc enabled for better memory management

**CI/CD** (`.github/workflows/ci.yml`):
- Test job: rustfmt, clippy, unit tests
- Build job: Docker build and push to ghcr.io
- Caching: Cargo registry/index/target + Docker layers
- Container registry: `ghcr.io/tercen/ggrs_plot_operator`
- Tagging: Push to main → `main` tag; Tag `0.1.0` → `0.1.0` tag (NO 'v' prefix!)

### Current Implementation Details

**TercenClient** (`src/tercen/client.rs`):
- `from_env()`: Create client from environment variables
- `connect(uri, token)`: Connect with explicit credentials
- Service clients: `task_service()`, `table_service()`, `event_service()` (disabled), `workflow_service()`
- `AuthInterceptor`: Injects Bearer token into all gRPC requests

**TableStreamer** (`src/tercen/table.rs`):
- `stream_tson(table_id, columns, offset, limit)`: Stream TSON chunk
- `get_schema(table_id)`: Get table schema with row count
- Schema-based row limiting prevents infinite loops

**TercenStreamGenerator** (`src/ggrs_integration/stream_generator.rs`):
- Implements GGRS `StreamGenerator` trait
- `new()`: Creates generator with table IDs, loads facets and axis ranges
- `load_axis_ranges_from_table()`: Loads pre-computed Y-axis ranges
- `compute_axis_ranges()`: Fallback to scan data and compute ranges
- `stream_facet_data()`: Streams and filters chunks by facet indices
- Uses `tokio::task::block_in_place()` for async/sync compatibility
- Helper functions: `extract_row_count_from_schema()`, `extract_column_names_from_schema()`

**TercenLogger** (`src/tercen/logger.rs`):
- `log(message)`: Send log message to Tercen
- `progress(percent, message)`: Send progress update
- **⚠️ Currently disabled** - EventService returns UnimplementedError

**Error Handling** (`src/tercen/error.rs`):
```rust
pub enum TercenError {
    Grpc(Box<tonic::Status>),
    Transport(Box<tonic::transport::Error>),
    Auth(String),
    Config(String),
    Connection(String),
    Data(String),
}
```

### Files Modified in Recent Sessions

**For EventService debugging** (2025-01-08):
1. `src/main.rs` - All `logger.log()` calls commented out
2. Added support for RunComputationTask and CubeQueryTask variants
3. Changed `logger` → `_logger` in function signatures

**For build optimization** (2025-01-08):
1. `Cargo.toml` - Added `[profile.dev-release]` section
2. `Dockerfile` - Changed to use `--profile dev-release`

**See `DEPLOYMENT_DEBUG.md` for detailed change tracking and revert instructions.**
