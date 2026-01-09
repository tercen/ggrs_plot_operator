# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

The **ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with the Tercen platform. It receives tabular data through the Tercen gRPC API, generates high-performance plots using GGRS, and returns PNG images back to Tercen for visualization.

## ⚠️ IMPORTANT: Current Status & Known Issues

**Phase**: Phase 7 COMPLETE ✅ | **Next**: Phase 8 (Result Upload)

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
- ✅ TableSchemaService (streamTable)
- ✅ Full plot generation pipeline (475K rows → PNG in 9.5s)
- ✅ GPU acceleration (OpenGL backend: 0.5s vs CPU: 3.1s)
- ✅ Columnar architecture with Polars
- ✅ TSON streaming and dequantization

**What's Blocked**:
- ❌ EventService.create() - All logging disabled
- ⏸️ Phase 8: Result upload - Ready to implement

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
- Check proto files: `ls protos/`
- Update deps: `cargo update`

**Tests failing?**
- Use test script: `./test_local.sh`
- Check test_stream_generator binary exists
- Verify WORKFLOW_ID and STEP_ID are valid

**See `DEPLOYMENT_DEBUG.md` for detailed troubleshooting.**

## Module Structure

```
src/
├── main.rs                      # Entry point (⚠️ logging disabled!)
├── tercen/                      # Pure Tercen gRPC client (future crate)
│   ├── client.rs               # TercenClient with auth
│   ├── table.rs                # TableStreamer (chunked streaming)
│   ├── tson_convert.rs         # TSON → Polars DataFrame (columnar)
│   ├── facets.rs               # Facet metadata loading
│   ├── logger.rs               # TercenLogger (currently disabled)
│   └── error.rs                # TercenError types
├── ggrs_integration/           # GGRS-specific code
│   └── stream_generator.rs     # TercenStreamGenerator impl
└── bin/
    └── test_stream_generator.rs # Test binary (USE THIS for testing!)
```

**Key Design Principle**: `src/tercen/` has NO GGRS dependencies for future extraction as separate crate.

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

1. Encode PNG to base64
2. Create result DataFrame with `.content`, `filename`, `mimetype` columns
3. Wrap in `OperatorResult` JSON structure (if needed)
4. Upload via `FileService.uploadTable()` or similar
5. Update task with result reference
6. Test full operator lifecycle

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

Claude Code should NOT create commits or push:
- ❌ Never use `git commit`
- ❌ Never use `git push`
- ✅ Run quality checks: `cargo fmt`, `cargo clippy`, `cargo build`, `cargo test`
- ✅ Use `git status` and `git diff` to show changes
- ✅ Stage changes with `git add` if requested

**The user handles all commits and pushes manually.**

## Code Quality Standards

- Follow Rust API guidelines
- Use `rustfmt` for formatting: `cargo fmt`
- Pass `clippy` lints with zero warnings: `cargo clippy -- -D warnings`
- Write rustdoc comments for all public APIs
- Use semantic commit messages (when user commits)

## Proto Files

Proto files are copied from:
`/home/thiago/workspaces/tercen/main/sci/tercen_grpc/tercen_grpc_api/protos/`

- `tercen.proto`: Service definitions (TaskService, TableSchemaService, FileService)
- `tercen_model.proto`: Data model definitions (ETask, ComputationTask, CrosstabSpec)

Proto files are compiled at build time via `build.rs`:
```rust
tonic_build::configure()
    .build_server(false)      // Client only
    .build_transport(false)   // Avoid naming conflicts
    .compile(&["protos/tercen.proto", "protos/tercen_model.proto"], &["protos"])
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

**File Upload** (Phase 8 TODO):
- Encode PNG to base64
- Create result table with `.content`, `filename`, `mimetype` columns
- Upload via `FileService.upload()` or save as table
- Reference result in task

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
