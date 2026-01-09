# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

The **ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with the Tercen platform. It receives tabular data through the Tercen gRPC API, generates high-performance plots using GGRS, and returns PNG images back to Tercen for visualization.

## ⚠️ IMPORTANT: Current Status & Known Issues

**Phase**: Phase 8 IN PROGRESS 🚧 | **Current**: Debugging result upload "columns missing" error

**Deployment Status**: ✅ Working (with logging disabled)

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
- ✅ TableSchemaService (streamTable, uploadTable)
- ✅ Full plot generation pipeline (475K rows → PNG in 9.5s)
- ✅ GPU acceleration (OpenGL backend: 0.5s vs CPU: 3.1s)
- ✅ Columnar architecture with Polars
- ✅ TSON streaming and dequantization
- ✅ Proto files via submodule (tercen_grpc_api)

**What's Blocked**:
- ❌ EventService.create() - All logging disabled
- 🚧 Phase 8: Result upload - Debugging Sarno worker error (see logbook below)

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

**Current Hypothesis**:
The result DataFrame structure now matches R's output format. The issue may have been the combination of:
- Unnecessary `.ci`/`.ri` columns
- Missing `plot_width`/`plot_height` columns
- Incorrect TSON type codes (now fixed)

**Next Steps**:
1. Test current implementation in production
2. If still failing, investigate column ordering or TSON encoding details
3. Consider examining actual R operator output TSON bytes for comparison

**Files Modified**:
- `src/tercen/result.rs` - Result DataFrame structure
- `src/main.rs` - Added plot dimensions to save_result call
- `build.rs` - Updated to use tercen_grpc_api submodule
- `.gitmodules` - Added tercen_grpc_api submodule

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

5. TODO Phase 8: Upload to Tercen
   └─ Encode PNG to base64
   └─ Create result DataFrame with .content, filename, mimetype
   └─ Upload via FileService or TableSchemaService
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
**Phase 8**: 📋 NEXT - Result upload to Tercen

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

### Next Steps (Phase 8)

1. ✅ Encode PNG to base64
2. ✅ Create result DataFrame with `.content`, `.ci`, `.ri`, `filename`, `mimetype` columns
3. 🚧 Test result upload to Tercen
4. 🚧 Verify result appears in Tercen UI
5. ⏸️ Update task state (if needed)
6. ⏸️ Test full operator lifecycle end-to-end

**Note**: Result structure uses columnar format with facet indices (`.ci`, `.ri`) to support multi-facet plots.

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

**Recommended Method** (workflow/step-based, like Python's OperatorContextDev):

```bash
# 1. Edit test_local.sh with your WORKFLOW_ID and STEP_ID
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

**File Upload** (Phase 8 IN PROGRESS):
- Encode PNG to base64 ✅
- Create result table with `.content`, `.ci`, `.ri`, `filename`, `mimetype` columns ✅
- Upload via `TableSchemaService.save()` with TSON format 🚧
- Test result visibility in Tercen UI 🚧
- **Note**: Results use columnar format with facet indices for multi-facet support

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
