# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

The **ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with the Tercen platform. It receives tabular data through the Tercen gRPC API, generates high-performance plots using GGRS, and returns PNG images back to Tercen for visualization.

**Current Status**: Phase 7 COMPLETE ✅ - Full end-to-end plot generation with GPU acceleration! Successfully tested with 475K rows. GPU backend optimized to use OpenGL (162 MB peak) instead of Vulkan (314 MB), achieving 49% memory reduction while maintaining 10x speedup over CPU.

**📋 Continue from**: Ready for Phase 8 (result upload to Tercen).

**⚠️ Note**: render_v2.rs is the active implementation. render.rs will be replaced once v2 is fully validated (see TODO).

## Quick Reference

### Common Commands

```bash
# Build and test
cargo build                    # Debug build
cargo build --release          # Release build with optimizations
cargo test                     # Run all tests
cargo clippy                   # Lint code
cargo fmt                      # Format code

# Local testing with workflow/step IDs (like Python OperatorContextDev)
./test_local.sh                # Uses hardcoded test values in script
# Or manually:
TERCEN_URI="http://127.0.0.1:50051" \
TERCEN_TOKEN="your_token" \
WORKFLOW_ID="workflow_id" \
STEP_ID="step_id" \
cargo run --bin test_stream_generator

# Docker
docker build -t ggrs_plot_operator:local .
docker run --rm ggrs_plot_operator:local

# CI/CD
git push origin main           # Triggers CI workflow
git tag 0.1.0 && git push origin 0.1.0  # Create release (NO 'v' prefix)
```

See `BUILD.md` for comprehensive build and deployment instructions.
See `TEST_LOCAL.md` and `WORKFLOW_TEST_INSTRUCTIONS.md` for testing instructions.

## Project Structure

```
ggrs_plot_operator/
├── src/
│   ├── main.rs                  # Entry point with task processing logic
│   ├── tercen/                  # Tercen gRPC client library (future tercen-rust crate)
│   │   ├── mod.rs               # Module exports
│   │   ├── client.rs            # TercenClient with gRPC connection and auth
│   │   ├── error.rs             # TercenError types
│   │   ├── logger.rs            # TercenLogger for sending logs/progress to Tercen
│   │   ├── table.rs             # TableStreamer for streaming data in chunks
│   │   ├── data.rs              # CSV parsing, DataRow, and facet filtering
│   │   └── README.md            # Extraction plan
│   └── ggrs_integration/        # GGRS integration code (Phase 6+)
│       └── mod.rs               # Module stubs (empty)
├── docs/
│   ├── 09_FINAL_DESIGN.md       # ⭐ PRIMARY: Complete architecture
│   ├── 10_IMPLEMENTATION_PHASES.md # Current implementation roadmap
│   ├── 03_GRPC_INTEGRATION.md   # gRPC API specifications
│   └── [other docs]             # Historical design iterations
├── protos/                      # gRPC protocol buffer definitions
│   ├── tercen.proto             # Tercen service definitions
│   └── tercen_model.proto       # Tercen data model definitions
├── Cargo.toml                   # Current dependencies (tokio, tonic, prost, csv, etc.)
├── build.rs                     # tonic-build configuration for proto compilation
├── Dockerfile                   # Multi-stage Docker build
├── .github/workflows/ci.yml     # CI/CD pipeline
├── operator.json                # Tercen operator specification
├── BUILD.md                     # Comprehensive build guide
└── CLAUDE.md                    # This file
```

**Key files to read first**:
- `docs/09_FINAL_DESIGN.md` - Complete architecture and design decisions
- `docs/10_IMPLEMENTATION_PHASES.md` - Phased implementation roadmap
- `src/main.rs` - Entry point and task processing logic
- `src/tercen/client.rs` - gRPC client implementation
- `src/tercen/data.rs` - Data structures and CSV parsing

## High-Level Architecture

### Three-Layer Design

1. **gRPC Client Layer**
   - Interfaces with Tercen's gRPC API services
   - Handles authentication via token-based auth
   - Services: TaskService (task lifecycle), TableSchemaService (data streaming), FileService (file upload)
   - Uses tonic (~0.11) for gRPC, prost (~0.12) for protobuf serialization

2. **Data Transformation Layer (COLUMNAR ARCHITECTURE)**
   - **Pure Polars columnar operations** - NO row-by-row Record construction
   - Converts Tercen TSON format directly to Polars DataFrame (columnar → columnar)
   - Uses Polars lazy API for filtering: `col().eq().and()` with predicate pushdown
   - Zero-copy operations where possible, avoiding data duplication
   - Maps Tercen crosstab specifications (row factors, column factors, X/Y axes) to GGRS Aes (aesthetics)
   - Translates Tercen faceting to GGRS FacetSpec (none/col/row/grid)
   - Handles operator properties (theme, dimensions, scales)

3. **GGRS Integration Layer**
   - Custom `TercenStreamGenerator` implementing GGRS `StreamGenerator` trait
   - Lazy loads data from Tercen on demand for memory efficiency
   - Uses quantized coordinates: `.xs` and `.ys` (uint16 stored as i64)
   - GGRS automatically dequantizes using axis ranges
   - Integrates with GGRS core components: PlotGenerator (engine) and ImageRenderer (rendering)
   - Uses Plotters backend for PNG generation

### Data Flow (Final - With Dequantization)

```
User clicks "Run" in Tercen
  ↓
1. TercenStreamGenerator::new()
   - Connect & authenticate via gRPC
   - Load facet metadata (column.csv, row.csv - small tables)
   - Pre-load axis ranges from Y-axis table (or compute from data)
   - Create Aes mapping to .x and .y (dequantized columns)
   ↓
2. GGRS calls query_data_chunk() per facet cell during rendering
   - Stream TSON data in 1000-row chunks via ReqStreamTable
   - Parse TSON → Polars DataFrame (COLUMNAR, no Records!)
   - Filter using Polars lazy: col(".ci").eq(col_idx).and(col(".ri").eq(row_idx))
   - Concatenate chunks with vstack_mut() (columnar append)
   - Data contains quantized coordinates: .xs/.ys (uint16 as i64)
   ↓
3. GGRS dequantizes in render.rs after each query
   - Calls dequantize_chunk(df, x_range, y_ranges)
   - Formula: value = (quantized / 65535) * (max - min) + min
   - Creates new columns .x and .y with actual data values
   - Removes .xs and .ys columns
   ↓
4. GGRS renders with dequantized data
   - Auto-converts i64 → f64 for coordinates
   - Plots points using actual data ranges (e.g., 0-10)
   - Each facet cell rendered progressively as chunks arrive
   ↓
5. Generate final PNG
   - ImageRenderer produces complete PNG buffer
   ↓
6. Save result to Tercen table (TODO: Phase 8)
   - Encode PNG to base64
   - Create DataFrame: .content (base64), filename, mimetype
   - Save as Tercen table
```

### Data Structure (from example files)

**Main data** (TSON format): Large table with plot points
```csv
.ci,.ri,.xs,.ys,sp,...
0,0,12845,15632,"B",...
0,0,13124,19687,"B",...
```
- `.ci`: Column facet index (i64)
- `.ri`: Row facet index (i64)
- `.xs`, `.ys`: Quantized coordinates (uint16 stored as i64, range 0-65535)
- Other columns: Color/aesthetics (string or numeric)

**Column facets** (`column.csv`): Small table defining column groups
```csv
sp
B
O
```

**Row facets** (`row.csv`): Small table defining row groups
```csv
variable,sex
BD,F
BD,M
```

### Streaming Implementation (COLUMNAR)

**Key insight**: GGRS `StreamGenerator` trait queries data on-demand per facet cell. Tercen's `ReqStreamTable` supports chunking with `offset` and `limit`!

```rust
pub struct TercenStreamGenerator {
    client: Arc<TercenClient>,
    main_table_id: String,
    facet_info: FacetInfo,
    axis_ranges: HashMap<(usize, usize), (AxisData, AxisData)>,
    total_rows: usize,
    aes: Aes,  // Maps to .x and .y (dequantized coordinates)
    facet_spec: FacetSpec,
    chunk_size: usize,
}

impl TercenStreamGenerator {
    /// Create generator with explicit table IDs
    pub async fn new(
        client: Arc<TercenClient>,
        main_table_id: String,
        col_facet_table_id: String,
        row_facet_table_id: String,
        y_axis_table_id: Option<String>,
        chunk_size: usize,
    ) -> Result<Self> {
        // Load facet metadata from tables
        let facet_info = FacetInfo::load(&client, &col_facet_table_id, &row_facet_table_id).await?;

        // Load or compute axis ranges
        let (axis_ranges, total_rows) = if let Some(ref y_table_id) = y_axis_table_id {
            Self::load_axis_ranges_from_table(&client, y_table_id, &main_table_id, &facet_info).await?
        } else {
            Self::compute_axis_ranges(&client, &main_table_id, &facet_info, chunk_size).await?
        };

        // Create Aes mapping to dequantized columns
        let aes = Aes::new().x(".x").y(".y");

        Ok(Self { client, main_table_id, facet_info, axis_ranges, total_rows, aes, facet_spec, chunk_size })
    }
}

impl StreamGenerator for TercenStreamGenerator {
    // GGRS calls this for each facet cell during rendering
    fn query_data_chunk(&self, col_idx: usize, row_idx: usize, range: Range) -> DataFrame {
        // Stream TSON chunks from Tercen (returns quantized .xs/.ys)
        // GGRS will dequantize after this returns
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.stream_facet_data(col_idx, row_idx, range).await
            })
        }).unwrap_or_else(|_| DataFrame::new())
    }

    fn query_x_axis(&self, col_idx, row_idx) -> AxisData {
        self.axis_ranges.get(&(col_idx, row_idx)).map(|(x, _)| x.clone()).unwrap_or_default()
    }

    fn query_y_axis(&self, col_idx, row_idx) -> AxisData {
        self.axis_ranges.get(&(col_idx, row_idx)).map(|(_, y)| y.clone()).unwrap_or_default()
    }
}
```

**Benefits**:
- ✅ Pure columnar operations - NO row-by-row Record construction
- ✅ Polars lazy API with predicate pushdown for efficient filtering
- ✅ Zero-copy operations where possible
- ✅ **Dequantization in GGRS** - operator stays simple, returns quantized data
- ✅ Memory efficient (CPU): stable ~49MB for 475K rows
- ✅ Memory efficient (GPU): stable ~162MB for 475K rows (OpenGL backend, 10x faster)
- ✅ Fast: CPU 3.1s, GPU 0.5s for 475K points
- ✅ Lazy loading - only fetch what GGRS needs
- ✅ Progressive rendering - each chunk is dequantized and rendered immediately

**Minimal main**:
```rust
#[tokio::main]
async fn main() -> Result<()> {
    // 1. Create stream generator (reads env, connects)
    let stream_gen = TercenStreamGenerator::from_env().await?;

    // 2. Create plot and render (GGRS handles everything)
    let plot_spec = EnginePlotSpec::new().add_layer(Geom::point());
    let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;
    let renderer = ImageRenderer::new(plot_gen, 800, 600);
    let png = renderer.render_to_buffer()?;

    // 3. Upload
    upload_to_tercen(&png).await?;
    Ok(())
}
```

### Columnar Architecture Deep Dive

#### Records → Polars DataFrame Migration (COMPLETED ✅)

The codebase underwent a complete migration from row-oriented Record processing to pure columnar Polars operations for maximum performance.

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
   - Eliminates manual row iteration and HashMap construction

3. **Concatenation**:
   - Uses `vstack_mut()` for columnar chunk appending
   - NO record-by-record merging

4. **Type Coercion** (`ggrs-core/src/data.rs`):
   - `column_as_f64()` auto-converts i64 → f64
   - Handles quantized coordinates (uint16 stored as i64)

5. **Dequantization** (`ggrs-core/src/stream.rs` and `ggrs-core/src/render.rs`):
   - `dequantize_chunk()` converts quantized `.xs`/`.ys` to actual `.x`/`.y` values
   - Called in render pipeline after each `query_data_chunk()`
   - Formula: `value = (quantized / 65535.0) * (max - min) + min`
   - Handles both single-facet and multi-facet data

**Performance Results (with Full Rendering + Dequantization)**:
- Test dataset: 475,688 rows
- Total time: 9.5 seconds (data fetch + dequantization + rendering)
- Memory usage: Stable at ~46MB (peak during rendering)
- Plot output: 59KB PNG file
- Data throughput: ~50K rows/second with full processing

**Files Modified**:
- `src/ggrs_integration/stream_generator.rs` - Added `new()` constructor with axis loading
- `src/tercen/tson_convert.rs` - Pure columnar conversion
- `ggrs/crates/ggrs-core/src/data.rs` - Added i64→f64 auto-conversion
- `ggrs/crates/ggrs-core/src/stream.rs` - Added `dequantize_chunk()` function
- `ggrs/crates/ggrs-core/src/render.rs` - Integrated dequantization into render loop
- `ggrs/crates/ggrs-core/src/engine.rs` - Added dequantization for aesthetic training
- `src/bin/test_stream_generator.rs` - Updated to use `new()` constructor

## Key Technical Decisions

### Columnar Architecture (CRITICAL!)
- **Pure Polars Operations**: ALL data processing uses columnar operations
- **NO Record Construction**: Never build row-by-row `Vec<Record>` or `HashMap<String, Value>`
- **Stay Columnar**: TSON (columnar) → Polars DataFrame (columnar) → GGRS (columnar)
- **Lazy API**: Use Polars lazy evaluation with predicate pushdown for filtering
- **Zero-Copy**: Minimize data duplication, use references and move semantics

### Memory Efficiency & Dequantization
- **Streaming Architecture**: Don't load entire table into memory; process in chunks
- **Lazy Faceting**: Only load data for facet cells being rendered
- **Chunked Processing**: Default 15000 rows per operator chunk
- **Schema-Based Limiting**: Use table schema row count to prevent infinite loops
- **Dequantization in GGRS**: Operator returns compact quantized data (2 bytes/coordinate)
- **Progressive Dequantization**: Each chunk dequantized immediately before rendering, then discarded
- **Test Results (CPU)**: 475K rows in 3.1s, memory stable at ~49MB peak
- **Test Results (GPU)**: 475K rows in 0.5s, memory stable at ~162MB peak (OpenGL)

### GPU Backend (WebGPU)
- **Backend Selection**: Configurable via `operator_config.json` (`"backend": "cpu"` or `"gpu"`)
- **OpenGL vs Vulkan**: OpenGL selected for 49% memory reduction (162 MB vs 314 MB)
- **Performance**: 10x faster than CPU (0.5s vs 3.1s for 475K points)
- **Memory Overhead**: GPU uses 3.3x more memory than CPU (acceptable for performance gain)
- **Driver Overhead**: ~97 MB for OpenGL initialization, ~12 MB for staging buffers
- **Implementation**: `ggrs/crates/ggrs-core/src/renderer/webgpu.rs`
- **Configuration**: `wgpu::Backends::GL` with `Limits::downlevel_defaults()`
- **Documentation**: See `docs/GPU_BACKEND_MEMORY.md` for detailed investigation

**Memory Comparison**:
| Backend | Peak Memory | Speedup | Render Time |
|---------|------------|---------|-------------|
| CPU (Cairo) | 49 MB | 1.0x | 3.1s |
| GPU (OpenGL) | 162 MB | 10x | 0.5s |
| GPU (Vulkan) | 314 MB | 10x | 0.5s (rejected) |

### Proto Files Location
Proto files should be copied/linked from: `/home/thiago/workspaces/tercen/main/sci/tercen_grpc/tercen_grpc_api/protos/`
- `tercen.proto`: Service definitions (TaskService, TableSchemaService, FileService, UserService)
- `tercen_model.proto`: Data model definitions (ETask, ComputationTask, CrosstabSpec, etc.)

### Core Dependencies
- **tonic** (~0.11): gRPC client with TLS support
- **prost** (~0.12): Protocol Buffer serialization
- **tokio** (~1.35): Async runtime with multi-threading
- **tokio-stream**: Stream utilities for gRPC
- **polars** (~0.44): Columnar DataFrame operations (CRITICAL for performance!)
- **rustson** (0.1.14): TSON parsing (Tercen's binary format)
- **serde** / **serde_json**: Serialization/deserialization
- **thiserror**: Error handling
- **anyhow**: Error context
- **base64** (0.22): PNG encoding for result upload
- **tikv-jemallocator** (optional): Memory allocator for performance
- **ggrs-core**: Plot generation library (local path dependency to sibling GGRS project at `../ggrs/crates/ggrs-core`)

### Build System
Proto files are automatically compiled at build time using `tonic-build` in `build.rs`:
```rust
tonic_build::configure()
    .build_server(false)      // Client only, no server code
    .build_transport(false)   // Avoid naming conflicts
    .compile(&["protos/tercen.proto", "protos/tercen_model.proto"], &["protos"])
```

Generated proto code is included in the binary using:
```rust
// In src/tercen/client.rs
pub mod proto {
    tonic::include_proto!("tercen");
}
```

## Implementation Status

**Current Phase**: Phase 7 COMPLETE ✅ - Full end-to-end plot generation working! Ready for Phase 8 (result upload)

**✅ Working (Full Pipeline)**:
1. Pure Polars columnar operations - NO Record construction
2. TSON → Polars DataFrame conversion (schema-based limiting)
3. Polars lazy filtering with predicate pushdown: `col().eq().and()`
4. Chunked streaming with `vstack_mut()` concatenation
5. Quantized coordinates: `.xs`/`.ys` transmitted from operator
6. **Dequantization in GGRS**: `.xs`/`.ys` → `.x`/`.y` conversion with axis ranges
7. Axis range loading from Y-axis table or computation fallback
8. `TercenStreamGenerator::new()` constructor with table IDs
9. Full plot rendering: 475K rows → 59KB PNG in 9.5s
10. Configuration system (operator_config.json)
11. Test binary with memory tracking

**📋 See**: Latest performance metrics in memory_usage_chunk_15000.csv/.png

### Completed Phases

#### Phase 1: CI/CD and Basic Operator Structure ✅
- ✅ `operator.json` with properties (width, height, theme, title)
- ✅ `Cargo.toml` with full dependencies
- ✅ `src/main.rs` - entry point with task processing
- ✅ Dockerfile with multi-stage build
- ✅ CI/CD workflow (`.github/workflows/ci.yml`) - test + build jobs
- ✅ Successfully builds and runs locally

#### Phase 2: gRPC Connection ✅
- ✅ Proto files copied and compiling via `build.rs`
- ✅ `TercenClient::from_env()` implemented with TLS authentication
- ✅ `AuthInterceptor` for Bearer token injection
- ✅ Service clients (TaskService, EventService, TableSchemaService)
- ✅ Connection tested successfully

#### Phase 3: Streaming Data ✅
- ✅ `TableStreamer` implemented in `src/tercen/table.rs`
- ✅ Chunked streaming via `ReqStreamTable` with offset/limit
- ✅ `stream_csv()` and `stream_table_chunked()` methods
- ✅ Verified chunking works correctly

#### Phase 4: Data Parsing and Filtering ✅
- ✅ Switched from CSV to TSON (Tercen's binary format)
- ✅ `tson_to_dataframe()` for TSON parsing using rustson library
- ✅ Fixed infinite loop in data streaming (schema-based limiting)
- ✅ `filter_by_facet()` for filtering by column/row indices
- ✅ Synthetic x-value generation with global offsets
- ✅ `TercenLogger` for sending logs to Tercen

#### Phase 5: Configuration & Facet Metadata ✅
- ✅ Created `operator_config.json` for centralized configuration
- ✅ Unified chunk_size across all modules
- ✅ Loaded facet metadata (FacetInfo)
- ✅ Pre-computed axis ranges for all facet cells
- ✅ Schema querying (handles TableSchema, ComputedTableSchema, CubeQueryTableSchema)

#### Phase 6: GGRS Integration + Columnar Migration ✅
- ✅ Implemented `StreamGenerator` trait for `TercenStreamGenerator`
- ✅ **COLUMNAR MIGRATION**: Eliminated all Record construction, pure Polars operations
- ✅ Fixed TSON parsing infinite loop (schema-based row limiting)
- ✅ Implemented workflow/step-based context loading
- ✅ Facet metadata loading (column.csv, row.csv)
- ✅ Axis range pre-computation with 5% padding
- ✅ Polars lazy filtering with predicate pushdown: `col(".ci").eq().and(col(".ri").eq())`
- ✅ Quantized coordinates: `.xs`/`.ys` (uint16 as i64) instead of synthetic `.x`
- ✅ Auto i64→f64 conversion in GGRS `column_as_f64()`
- ✅ Configuration system (operator_config.json)
- ✅ Standalone test binary (test_stream_generator)

#### Phase 7: Plot Generation and Dequantization ✅
- ✅ Implemented `dequantize_chunk()` in GGRS `stream.rs`
- ✅ Integrated dequantization into GGRS `render.rs` render loop (line ~1822)
- ✅ Added dequantization to GGRS `engine.rs` aesthetic training (line ~551)
- ✅ Handles both single-facet and multi-facet data (optional `.ri` column)
- ✅ Implemented `TercenStreamGenerator::new()` constructor
- ✅ Added `load_axis_ranges_from_table()` for Y-axis table loading
- ✅ Added `compute_axis_ranges()` as fallback
- ✅ Added helper functions: `extract_row_count_from_schema()`, `extract_column_names_from_schema()`
- ✅ Updated Aes mapping to use `.x` and `.y` (dequantized columns)
- ✅ Full end-to-end test: 475K rows → 59KB PNG in 9.5s
- ✅ Memory stable: ~46MB peak during full rendering
- ✅ Plot displays correct data ranges (verified dequantization working)

### Next Steps

**Phase 8: Result Upload to Tercen** 📋 NEXT
1. Encode PNG to base64
2. Create result DataFrame with `.content`, `filename`, `mimetype` columns
3. Wrap in `OperatorResult` JSON structure
4. Upload via `FileService.uploadTable()`
5. Update task with `fileResultId`
6. Test full operator lifecycle from task pickup to result upload

**Phase 9: Production Polish**
1. Operator properties support (read from task)
2. Better legend support (color mappings)
3. Multi-facet grid layout support
4. Color aesthetics (categorical and continuous)
5. Error handling improvements
6. Progress reporting during long operations

### Complete Roadmap

See `docs/10_IMPLEMENTATION_PHASES.md` for details:
- Phase 1: CI/CD and Basic Operator Structure ✅
- Phase 2: gRPC Connection and Simple Call ✅
- Phase 3: Streaming Data - Test Chunking ✅
- Phase 4: Data Parsing and Filtering ✅
- Phase 5: Load Facet Metadata ✅
- Phase 6: GGRS Integration + Columnar Migration ✅
- Phase 7: Plot Generation and Dequantization ✅
- Phase 8: Result Upload to Tercen 📋 NEXT
- Phase 9: Production Polish

### Module Structure (Designed for Library Extraction)

The `src/tercen/` module is intentionally isolated to enable future extraction as a separate `tercen-rust` crate:

```
src/tercen/              # ⭐ Future tercen-rust crate
├── mod.rs               # Module exports and documentation
├── client.rs            # TercenClient with connection and auth (COMPLETE)
├── error.rs             # TercenError type (COMPLETE)
├── logger.rs            # TercenLogger for logs/progress (COMPLETE)
├── table.rs             # TableStreamer for data streaming (COMPLETE)
├── data.rs              # DataRow, ParsedData structures (COMPLETE)
├── tson_convert.rs      # TSON → DataFrame conversion (COMPLETE)
├── facets.rs            # Facet metadata loading (COMPLETE)
└── arrow_convert.rs     # Arrow format support (unused, CSV preferred)

src/ggrs_integration/    # GGRS-specific (stays in this project)
├── mod.rs               # Module exports
└── stream_generator.rs  # TercenStreamGenerator impl (COMPLETE)

src/bin/
└── test_stream_generator.rs  # Standalone test binary (COMPLETE)
```

**Design benefits**: Clear separation, no GGRS deps in `tercen/`, easy extraction. See `src/tercen/README.md` for details.

## Development Workflow

### Testing with Workflow/Step IDs (Recommended)

Like Python's `OperatorContextDev`, you can test with workflow and step IDs:

```bash
# Use the test script (has hardcoded test values)
./test_local.sh

# Or manually:
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="your_token_here"
export WORKFLOW_ID="workflow_id_here"
export STEP_ID="step_id_here"

# Run test binary
cargo run --bin test_stream_generator
```

### Testing with Task ID (Alternative)

For testing the full operator with an existing task:

```bash
export TERCEN_URI=https://tercen.com:5400
export TERCEN_TOKEN=your_token_here
export TERCEN_TASK_ID=your_task_id_here

# Run main operator
cargo run
```

### Important Development Principles

#### ⚠️ CRITICAL: NO FALLBACK STRATEGIES ⚠️

**NEVER implement fallback logic, alternative approaches, or "try this if that fails" patterns unless EXPLICITLY instructed by the user.**

This is a critical design principle that prevents:
- Unnecessary complexity
- Masking underlying issues
- Performance degradation from checking multiple code paths
- Ambiguous behavior that's hard to debug

**Examples**:
- ❌ **BAD**: `if data.has_column(".ys") { use_ys() } else { use_y() }`
- ✅ **GOOD**: `data.column(".ys")` (when user says `.ys` exists)
- ❌ **BAD**: `let x = try_method_a().or_else(|| try_method_b()).or_else(|| try_method_c())`
- ✅ **GOOD**: `let x = the_correct_method()`

**When to use fallbacks** (ONLY these cases):
1. User explicitly requests handling multiple scenarios
2. User explicitly requests backward compatibility
3. You are implementing error recovery at well-defined boundaries (e.g., user input validation)

**When the user says something exists** (e.g., "use `.xs` column", "TSON format is used"):
- Trust that specification completely
- Implement the direct solution
- Do NOT add checks for alternative formats or columns "just in case"
- If something doesn't work, it's a bug to fix, not a reason to add fallbacks

### Local Development

```bash
# Format code
cargo fmt

# Lint code
cargo clippy

# Build (debug)
cargo build

# Build (optimized)
cargo build --release

# Run tests
cargo test

# Run with environment variables
cargo run
```

### Implemented Architecture (Phase 6 Complete)

The current implementation includes:

**TercenClient** (`src/tercen/client.rs`):
- `from_env()`: Create client from environment variables
- `connect(uri, token)`: Connect with explicit credentials
- `task_service()`, `table_service()`, `event_service()`, `workflow_service()`: Get authenticated service clients
- `AuthInterceptor`: Injects Bearer token into all gRPC requests

**TableStreamer** (`src/tercen/table.rs`):
- `stream_tson(table_id, columns, offset, limit)`: Stream a chunk of TSON data
- `stream_csv(table_id, columns, offset, limit)`: Stream a chunk of CSV data (legacy)
- `get_schema(table_id)`: Get table schema with row count
- Schema-based row limiting prevents infinite loops with zero-padded data

**TSON Conversion** (`src/tercen/tson_convert.rs`):
- `tson_to_dataframe()`: Parse TSON bytes to Polars DataFrame (STAY COLUMNAR!)
- Converts TSON columnar arrays directly to Polars `Series` → `Column`
- Handles numeric (i64, f64) and string columns
- NO row-by-row Record construction - pure columnar operations

**Facet Support** (`src/tercen/facets.rs`):
- `FacetMetadata`: Parses multi-column facet definitions
- `FacetInfo`: Manages both column and row facets
- Loads facet metadata from small tables (column.csv, row.csv)

**TercenStreamGenerator** (`src/ggrs_integration/stream_generator.rs`):
- Implements GGRS `StreamGenerator` trait for lazy data loading
- `new()`: Creates generator with explicit table IDs, loads facets and axis ranges
- `load_axis_ranges_from_table()`: Loads pre-computed Y-axis ranges from table
- `compute_axis_ranges()`: Fallback to scan data and compute ranges
- `stream_facet_data()`: Streams chunks, concatenates with `vstack_mut()` (columnar append)
- `filter_by_facet()`: Pure Polars lazy filtering with `col().eq().and()`
- Returns quantized coordinates `.xs`/`.ys` - GGRS handles dequantization
- Uses `tokio::task::block_in_place()` for async/sync trait compatibility
- Helper functions: `extract_row_count_from_schema()`, `extract_column_names_from_schema()`

**Configuration** (`src/config.rs`):
- `OperatorConfig`: Centralized configuration from `operator_config.json`
- `chunk_size`, `max_chunks`, `default_plot_width`, `default_plot_height`
- Falls back to sensible defaults if config missing

**Logger** (`src/tercen/logger.rs`):
- `log(message)`: Send log message to Tercen
- `progress(percent, message)`: Send progress update to Tercen

**Error Handling** (`src/tercen/error.rs`):
```rust
#[derive(Debug, thiserror::Error)]
pub enum TercenError {
    #[error("gRPC error: {0}")]
    Grpc(Box<tonic::Status>),
    #[error("Transport error: {0}")]
    Transport(Box<tonic::transport::Error>),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Data error: {0}")]
    Data(String),
}
```

### Testing Strategy

- **Unit tests**: Test each module with mocks (target >80% coverage)
- **Integration tests**: Against local Tercen instance (use `#[ignore]` flag)
- **Visual regression**: Compare generated PNGs with reference images
- **Performance benchmarks**: vs R plot_operator baseline

### CI/CD Pipeline

**Workflow** (`.github/workflows/ci.yml`):
- **Test job**: rustfmt, clippy, unit tests (all branches)
- **Build job**: Docker build and push to ghcr.io (depends on test)
- **Caching**: Cargo registry/index/target + Docker layers
- **Attestation**: Supply chain security via GitHub attestations

**Container registry**: `ghcr.io/tercen/ggrs_plot_operator`

**Tagging**:
- Push to main → `main` tag
- Create tag `0.1.0` → `0.1.0` tag (NO 'v' prefix!)

**Docker image**:
- Multi-stage build (builder + runtime)
- Runtime: Debian bookworm-slim (~120-150 MB)
- jemalloc enabled, size-optimized, non-root user

## Important Tercen Concepts

### Crosstab Projection
Tercen organizes data as a crosstab with:
- **Row factors**: Faceting rows (`.ri` in CSV)
- **Column factors**: Faceting columns (`.ci` in CSV)
- **X/Y axes**: Plot axes (`.x`, `.y` in CSV)
- **Color/Label factors**: Aesthetics
- **Page factors**: Separate output files

### Task Lifecycle
1. Operator polls `TaskService.waitDone()` (blocking)
2. Update task state to `RunningState`
3. Execute computation (fetch data, generate plot, upload)
4. Send progress updates via `TaskProgressEvent`
5. Update task state to `DoneState` or `FailedState`

### Data Streaming
- Use `TableSchemaService.streamTable()` with CSV or Arrow format
- Receives data in chunks (Vec<u8>)
- Concatenate chunks and parse
- For large datasets, process chunks incrementally

### File Upload
- Create `EFileDocument` with metadata (name, mimetype, size)
- Stream upload via `FileService.upload()`:
  - First message: file metadata
  - Subsequent messages: 64KB chunks of file data
- Returns created file with ID assigned by server

## Critical Implementation Notes

### Current Implementation Status (Columnar Architecture)

1. **Authentication**: ✅ `AuthInterceptor` injects Bearer token from `TERCEN_TOKEN` env var
2. **TLS Configuration**: ✅ `ClientTlsConfig` configured for secure connections
3. **Streaming Architecture**: ✅ `TableStreamer` uses chunked streaming with schema-based limiting
4. **TSON Parsing**: ✅ `tson_to_dataframe()` converts Tercen TSON to Polars DataFrame (COLUMNAR!)
5. **Facet Filtering**: ✅ `filter_dataframe_by_facet()` uses Polars lazy API with predicate pushdown
6. **Quantized Coordinates**: ✅ Uses `.xs`/`.ys` (uint16 as i64) with auto i64→f64 conversion
7. **Logging**: ✅ `TercenLogger` sends log messages and progress to Tercen
8. **Error Handling**: ✅ `TercenError` enum with proper error context using `thiserror`
9. **GGRS Integration**: ✅ `TercenStreamGenerator` implements `StreamGenerator` trait (pure columnar)
10. **Configuration**: ✅ Centralized config system with JSON file
11. **Testing**: ✅ Standalone test binary, validated with 475K rows
12. **Performance**: ✅ 5.6s for 475K rows, memory stable at ~60MB

## Documentation References

### Primary Documentation (Read These First)
- **`docs/09_FINAL_DESIGN.md`** ⭐⭐⭐ - Complete architecture and final design
- **`docs/10_IMPLEMENTATION_PHASES.md`** - Current implementation roadmap
- **`docs/GPU_BACKEND_MEMORY.md`** - GPU backend memory investigation and optimization
- **`docs/SESSION_2025-01-05.md`** - Session notes with debugging details
- **`docs/SESSION_2025-01-07.md`** - GPU backend optimization session
- **`IMPLEMENTATION_COMPLETE.md`** - Phase 6 completion status
- **`TESTING_STATUS.md`** - Testing phase status and instructions
- **`TEST_LOCAL.md`** - Local testing guide
- **`WORKFLOW_TEST_INSTRUCTIONS.md`** - Workflow/step-based testing
- **`BUILD.md`** - Comprehensive build and deployment guide

### Supporting Documentation
- `docs/03_GRPC_INTEGRATION.md` - gRPC API specs and examples
- `docs/08_SIMPLE_STREAMING_DESIGN.md` - Streaming architecture concepts
- `docs/01_ARCHITECTURE.md` - Initial architecture design
- `docs/04_DOCKER_AND_CICD.md` - Docker and CI/CD details
- `src/tercen/README.md` - Library extraction plan
- `CI_FINAL.md`, `DOCKER_UPDATES.md` - CI/CD implementation notes

### Historical/Deprecated
- ~~`docs/06_CONTEXT_DESIGN.md`~~ - Python pattern analysis (TOO COMPLEX, not used)
- ~~`docs/07_RUST_CONTEXT_IMPL.md`~~ - C# client-based design (TOO COMPLEX, not used)

### External Resources
- [Tercen gRPC API](https://github.com/tercen/tercen_grpc_api)
- [Tercen C# Client](https://github.com/tercen/TercenCSharpClient) - Reference implementation
- [Tercen Developers Guide](https://github.com/tercen/developers_guide)

## Code Organization Conventions

### Module Structure
- `src/tercen/`: Pure Tercen gRPC client code (no GGRS dependencies)
  - Use `#![allow(dead_code)]` at module level for Phase 4 implementations not yet fully utilized
  - All public APIs documented with rustdoc comments
  - Re-export key types in `mod.rs` for convenience
- `src/ggrs_integration/`: GGRS-specific code (Phase 6+)
  - Will depend on both `tercen` module and `ggrs-core`
- `src/main.rs`: Minimal entry point orchestrating modules

### Proto Code Access
- Generated proto code is in `client::proto` module
- Use type aliases for authenticated clients (e.g., `AuthTaskServiceClient`)
- Allow clippy lints for generated code: `#[allow(clippy::large_enum_variant, clippy::enum_variant_names)]`

### Async Patterns
- All gRPC operations are async using `tokio`
- Use `tokio-stream` for streaming responses
- Main function uses `#[tokio::main]` with implicit `rt-multi-thread`

## Code Quality Standards

### Pre-Commit Checklist

**CRITICAL**: Before considering any code change complete, you MUST run these checks locally:

```bash
# 1. Format check (must pass)
cargo fmt --check

# 2. If formatting needed, apply it
cargo fmt

# 3. Clippy check (must pass with no warnings)
cargo clippy -- -D warnings

# 4. Build check (must compile)
cargo build

# 5. Test check (when tests exist)
cargo test
```

**NEVER** consider a code update complete until all these checks pass locally. The CI will fail if any of these checks fail, wasting time and resources.

### General Standards

- Follow Rust API guidelines
- Use `rustfmt` for formatting: `cargo fmt`
- Pass `clippy` lints with zero warnings: `cargo clippy -- -D warnings`
- Write rustdoc comments for all public APIs
- Maintain test coverage >80% (future)
- Use semantic commit messages
- Feature branches with pull requests for all changes (if working in team)

## Git Policy

**IMPORTANT**: Do NOT create git commits or push to remote repositories.

- ❌ Never use `git commit`
- ❌ Never use `git push`
- ✅ Run `cargo fmt`, `cargo clippy`, `cargo build`, `cargo test`
- ✅ Stage changes with `git add` if needed
- ✅ Show `git status` and `git diff` to help user understand changes

The user will handle all commits and pushes manually.
