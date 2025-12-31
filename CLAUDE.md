# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

The **ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with the Tercen platform. It receives tabular data through the Tercen gRPC API, generates high-performance plots using GGRS, and returns PNG images back to Tercen for visualization.

**Current Status**: Phase 4 complete - gRPC connection, data streaming, CSV parsing, and facet filtering implemented. Ready for Phase 5 (load facet metadata).

## Quick Reference

### Common Commands

```bash
# Build and test
cargo build                    # Debug build
cargo build --release          # Release build with optimizations
cargo test                     # Run all tests
cargo clippy                   # Lint code
cargo fmt                      # Format code

# Docker
docker build -t ggrs_plot_operator:local .
docker run --rm ggrs_plot_operator:local

# CI/CD
git push origin main           # Triggers CI workflow
git tag 0.1.0 && git push origin 0.1.0  # Create release (NO 'v' prefix)
```

See `BUILD.md` for comprehensive build and deployment instructions.

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

2. **Data Transformation Layer**
   - Converts Tercen data formats (CSV/Arrow) to GGRS DataFrame
   - Maps Tercen crosstab specifications (row factors, column factors, X/Y axes) to GGRS Aes (aesthetics)
   - Translates Tercen faceting to GGRS FacetSpec (none/col/row/grid)
   - Handles operator properties (theme, dimensions, scales)

3. **GGRS Integration Layer**
   - Custom `TercenStreamGenerator` implementing GGRS `StreamGenerator` trait
   - Lazy loads data from Tercen on demand for memory efficiency
   - Integrates with GGRS core components: PlotGenerator (engine) and ImageRenderer (rendering)
   - Uses Plotters backend for PNG generation

### Data Flow (Final)

```
User clicks "Run" in Tercen
  ↓
1. TercenStreamGenerator::from_env()
   - Connect & authenticate via gRPC
   - Load task (get table IDs from ComputationTask)
   - Load facet metadata (column.csv, row.csv - small tables)
   - Pre-compute axis ranges for all facet cells
   ↓
2. GGRS calls query_data(col_idx, row_idx) per facet cell
   - Stream data in chunks via ReqStreamTable with offset/limit
   - Filter by .ci == col_idx AND .ri == row_idx
   - Parse CSV, collect all chunks for this facet
   - Return DataFrame to GGRS
   ↓
3. GGRS renders incrementally
   - Each facet cell rendered as data arrives
   - "Paint canvas incrementally"
   ↓
4. Generate final PNG
   - ImageRenderer produces complete PNG buffer
   ↓
5. Save result to Tercen table
   - Encode PNG to base64
   - Create DataFrame: .content (base64), filename, mimetype
   - Save as Tercen table
```

### Data Structure (from example files)

**Main data** (`qt.csv`-like): Large table with plot points
```csv
.ci,.ri,.x,.y,sp,...
0,0,51.0,6.1,"B",...
0,0,52.0,7.7,"B",...
```
- `.ci`: Column facet index
- `.ri`: Row facet index
- `.x`, `.y`: Plot coordinates
- Other columns: Color, aesthetics

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

### Streaming Implementation

**Key insight**: GGRS `StreamGenerator` trait queries data on-demand per facet cell. Tercen's `ReqStreamTable` supports chunking with `offset` and `limit`!

```rust
pub struct TercenStreamGenerator {
    table_service: TableSchemaServiceClient,
    table_id: String,
    aes: Aes,
    facet_spec: FacetSpec,
}

impl StreamGenerator for TercenStreamGenerator {
    // GGRS calls this for each facet cell as it renders
    fn query_data(&self, col_idx: usize, row_idx: usize) -> Result<DataFrame> {
        // Stream from Tercen filtered by facet indices
        let csv_data = self.stream_facet_data(col_idx, row_idx).await?;

        // Parse CSV directly (no Arrow complexity needed!)
        parse_csv_to_dataframe(csv_data)
    }

    fn n_col_facets(&self) -> usize { /* from crosstab spec */ }
    fn n_row_facets(&self) -> usize { /* from crosstab spec */ }
    fn query_x_axis(&self, col_idx, row_idx) -> Result<AxisData> { /* query range */ }
    fn query_y_axis(&self, col_idx, row_idx) -> Result<AxisData> { /* query range */ }
}
```

**Benefits**:
- ✅ No Context abstraction - direct gRPC
- ✅ No Apache Arrow - simple CSV parsing
- ✅ No buffering - stream directly from Tercen to GGRS
- ✅ Lazy loading - only fetch what GGRS needs
- ✅ Progressive rendering - each facet renders as data arrives

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

## Key Technical Decisions

### Memory Efficiency
- **Streaming Architecture**: Don't load entire table into memory; process in chunks
- **Lazy Faceting**: Only load data for facet cells being rendered
- **Chunked Processing**: Default 10K rows per chunk

### Proto Files Location
Proto files should be copied/linked from: `/home/thiago/workspaces/tercen/main/sci/tercen_grpc/tercen_grpc_api/protos/`
- `tercen.proto`: Service definitions (TaskService, TableSchemaService, FileService, UserService)
- `tercen_model.proto`: Data model definitions (ETask, ComputationTask, CrosstabSpec, etc.)

### Core Dependencies
- **tonic** (~0.11): gRPC client with TLS support
- **prost** (~0.12): Protocol Buffer serialization
- **tokio** (~1.35): Async runtime with multi-threading
- **tokio-stream**: Stream utilities for gRPC
- **csv** (1.3): Simple CSV parsing (NO Apache Arrow needed!)
- **serde**: Serialization/deserialization
- **thiserror**: Error handling
- **anyhow**: Error context
- **tikv-jemallocator** (optional): Memory allocator for performance
- **ggrs-core** (Phase 6+): Plot generation library (local path dependency to sibling GGRS project)

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

**Current Phase**: Phase 4 ✅ COMPLETED → Starting Phase 5

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
- ✅ `DataRow` struct with `.ci`, `.ri`, `.x`, `.y` fields
- ✅ `ParsedData::from_csv()` for CSV parsing
- ✅ `filter_by_facet()` for filtering by column/row indices
- ✅ `DataSummary` with statistics (min/max for x/y axes)
- ✅ `TercenLogger` for sending logs to Tercen

### Next Steps (Phase 5)
**Goal**: Load and parse facet metadata tables (column.csv, row.csv)

**Key tasks**:
1. Extract table IDs from ComputationTask query relation
2. Identify column and row facet table IDs from CrosstabSpec
3. Load small facet tables (`column.csv`, `row.csv`)
4. Parse facet metadata to determine facet structure
5. Calculate total number of facet cells (n_col_facets × n_row_facets)

### Complete Roadmap

See `docs/10_IMPLEMENTATION_PHASES.md` for details:
- Phase 1: CI/CD and Basic Operator Structure ✅
- Phase 2: gRPC Connection and Simple Call ✅
- Phase 3: Streaming Data - Test Chunking ✅
- Phase 4: Data Parsing and Filtering ✅
- Phase 5: Load Facet Metadata ⏭️ NEXT
- Phase 6: First GGRS Plot
- Phase 7: Output to Tercen Table
- Phase 8: Full Faceting Support
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
├── data.rs              # DataRow, ParsedData, CSV parsing (COMPLETE)
└── README.md            # Extraction plan

src/ggrs_integration/    # GGRS-specific (stays in this project)
├── mod.rs               # Module stub (TODO)
├── stream_generator.rs  # TercenStreamGenerator impl (Phase 6)
├── plot_builder.rs      # EnginePlotSpec builder (Phase 6)
└── renderer.rs          # ImageRenderer wrapper (Phase 6)
```

**Design benefits**: Clear separation, no GGRS deps in `tercen/`, easy extraction. See `src/tercen/README.md` for details.

## Development Workflow

### Testing with Environment Variables

The operator requires these environment variables to connect to Tercen:

```bash
# Required for connection
export TERCEN_URI=https://tercen.com:5400
export TERCEN_TOKEN=your_token_here

# Required for task processing
export TERCEN_TASK_ID=your_task_id_here

# Run the operator
cargo run
```

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

### Implemented Architecture (Phase 4)

The current implementation includes:

**TercenClient** (`src/tercen/client.rs`):
- `from_env()`: Create client from environment variables
- `connect(uri, token)`: Connect with explicit credentials
- `task_service()`, `table_service()`, `event_service()`: Get authenticated service clients
- `AuthInterceptor`: Injects Bearer token into all gRPC requests

**TableStreamer** (`src/tercen/table.rs`):
- `stream_csv(table_id, columns, offset, limit)`: Stream a chunk of data as CSV
- `stream_table_chunked(table_id, columns, chunk_size, callback)`: Stream entire table in chunks

**Data Types** (`src/tercen/data.rs`):
- `DataRow`: Represents a row with `.ci`, `.ri`, `.x`, `.y`, and extra fields
- `ParsedData`: Collection of rows with column names
- `from_csv()`: Parse CSV bytes into structured data
- `filter_by_facet()`: Filter rows by facet indices
- `summary()`: Get statistics (min/max for axes)

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

### Current Implementation (Phase 4)

1. **Authentication**: `AuthInterceptor` injects Bearer token from `TERCEN_TOKEN` env var into all gRPC requests
2. **TLS Configuration**: `ClientTlsConfig` is configured for secure connections in `TercenClient::connect()`
3. **Streaming Architecture**: `TableStreamer` uses chunked streaming with offset/limit - never loads entire datasets into memory
4. **CSV Parsing**: Simple `csv` crate parsing into `DataRow` structs - NO Apache Arrow complexity
5. **Facet Filtering**: `ParsedData::filter_by_facet()` filters data by `.ci` and `.ri` indices
6. **Logging**: `TercenLogger` sends log messages and progress to Tercen via EventService
7. **Error Handling**: `TercenError` enum with proper error context using `thiserror`

### Main Entry Point (`src/main.rs`)

The main function follows this flow:
1. Print version and phase info
2. Check for environment variables (`TERCEN_URI`, `TERCEN_TOKEN`, `TERCEN_TASK_ID`)
3. Connect to Tercen using `TercenClient::from_env()`
4. If `TERCEN_TASK_ID` is set:
   - Create `TercenLogger` for the task
   - Call `process_task()` to handle the task
   - Send logs to Tercen showing progress
5. Exit with status code

The `process_task()` function (Phase 4 implementation):
- Fetches task using `TaskService.get()`
- Extracts `ComputationTask` from task object
- Logs task structure (query, relation, settings)
- **Phase 5 will add**: Extract table IDs and load facet metadata
- **Phase 6 will add**: Generate plots using GGRS
- **Phase 7 will add**: Upload results to Tercen

### Future Implementation (Phase 6+)

8. **GGRS Integration**: Implement `StreamGenerator` trait for lazy data loading
9. **Progress Reporting**: Send incremental progress updates during long operations
10. **Resource Cleanup**: Ensure proper cleanup on errors (no partial uploads or leaked connections)

## Documentation References

### Primary Documentation (Read These First)
- **`docs/09_FINAL_DESIGN.md`** ⭐⭐⭐ - Complete architecture and final design
- **`docs/10_IMPLEMENTATION_PHASES.md`** - Current implementation roadmap
- **`docs/03_GRPC_INTEGRATION.md`** - gRPC API specs and examples
- **`BUILD.md`** - Comprehensive build and deployment guide

### Supporting Documentation
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
