# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

The **ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with the Tercen platform. It receives tabular data through the Tercen gRPC API, generates high-performance plots using GGRS, and returns PNG images back to Tercen for visualization.

This is a greenfield project currently in the planning phase with comprehensive architecture and implementation documentation but no code yet.

## Project Structure

```
ggrs_plot_operator/
├── docs/
│   ├── 01_ARCHITECTURE.md       # Complete system architecture and design
│   ├── 02_IMPLEMENTATION_PLAN.md # Phased implementation roadmap
│   └── 03_GRPC_INTEGRATION.md   # Detailed gRPC API specifications
├── README.md                    # Basic project description
└── .gitignore                   # Rust project ignores
```

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
- **tonic** (~0.11): gRPC client
- **prost** (~0.12): Protocol Buffer serialization
- **tokio** (~1.35): Async runtime
- **csv** (1.3): Simple CSV parsing (NO Apache Arrow needed!)
- **ggrs-core**: Plot generation library (local path dependency to sibling GGRS project)
- **serde/serde_json**: Configuration parsing
- **thiserror**: Error handling

### Build System
Use `tonic-build` in `build.rs` to compile proto files at build time:
```rust
tonic_build::configure()
    .build_server(false)  // Client only
    .compile(&["protos/tercen.proto", "protos/tercen_model.proto"], &["protos"])
```

## Implementation Status

**Current Phase**: Phase 1 (CI/CD and Basic Operator Structure) - ✅ COMPLETED

### Completed Phases

**Phase 1 (CI/CD and Basic Operator Structure)**:
- ✅ Created `operator.json` with basic properties (width, height, theme, title)
- ✅ Initialized `Cargo.toml` with minimal dependencies (tokio, jemalloc support)
- ✅ Created minimal `src/main.rs` that prints version, checks environment, and exits cleanly
- ✅ Successfully compiles and runs locally
- Next: Set up CI workflow and Dockerfile

### Implementation Roadmap

The project follows a revised implementation plan. See `docs/10_IMPLEMENTATION_PHASES.md` for the complete 9-phase roadmap:
- Phase 1: CI/CD and Basic Operator Structure ✅
- Phase 2: gRPC Connection and Simple Call
- Phase 3: Streaming Data - Test Chunking
- Phase 4: Data Parsing and Filtering
- Phase 5: Load Facet Metadata
- Phase 6: First GGRS Plot
- Phase 7: Output to Tercen Table
- Phase 8: Full Faceting Support
- Phase 9: Production Polish

### Current Files
```
ggrs_plot_operator/
├── operator.json         # Tercen operator specification
├── Cargo.toml            # Rust dependencies and build config
├── src/
│   └── main.rs           # Minimal entry point (Phase 1)
└── docs/
    ├── 10_IMPLEMENTATION_PHASES.md  # Revised implementation roadmap
    └── 09_FINAL_DESIGN.md           # Complete architecture
```

### Planned Module Structure

```rust
src/
├── main.rs                     // Entry point, CLI args, task polling loop
├── config.rs                   // OperatorConfig parsing
├── executor.rs                 // Orchestrates full execution pipeline
├── error.rs                    // OperatorError enum with From conversions
├── auth.rs                     // Token-based authentication
├── grpc_client.rs              // TercenGrpcClient with retry logic
├── services/
│   ├── task_service.rs         // TaskService wrapper
│   ├── table_service.rs        // TableSchemaService wrapper
│   └── file_service.rs         // FileService wrapper
├── data/
│   ├── stream.rs               // Chunked data retrieval
│   ├── dataframe.rs            // DataFrame builder
│   ├── aes_mapper.rs           // Crosstab → GGRS Aes mapping
│   ├── facet_mapper.rs         // Faceting logic
│   └── cache.rs                // In-memory cache
├── ggrs_integration/
│   ├── stream_generator.rs     // TercenStreamGenerator impl
│   ├── plot_builder.rs         // EnginePlotSpec builder
│   ├── generator.rs            // PlotGenerator wrapper
│   └── renderer.rs             // ImageRenderer wrapper
├── upload/
│   └── file_uploader.rs        // Streaming file upload
├── task/
│   ├── manager.rs              // Task lifecycle management
│   ├── result_linker.rs        // Link files to results
│   └── progress.rs             // Progress reporting
└── observability.rs            // Logging with tracing crate
```

## Development Workflow

### When Starting Implementation

1. **Phase 0**: Initialize Cargo project, set up proto compilation, create Dockerfile
2. **Phase 1**: Implement gRPC client foundation (auth, service wrappers, connection pooling)
3. **Phase 2**: Build data retrieval and transformation pipeline
4. **Phase 3**: Implement TercenStreamGenerator and GGRS integration
5. **Phase 4-5**: File upload and main application loop
6. **Phase 6-8**: Operator properties, optimization, testing
7. **Phase 9**: Deployment and release

### Error Handling Strategy

Define comprehensive error types:
```rust
#[derive(Debug, thiserror::Error)]
pub enum OperatorError {
    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),
    #[error("GGRS error: {0}")]
    Ggrs(#[from] ggrs_core::GgrsError),
    #[error("Data transformation error: {0}")]
    DataTransform(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Authentication error: {0}")]
    Auth(String),
}
```

Implement retry logic with exponential backoff for:
- `UNAVAILABLE`, `DEADLINE_EXCEEDED`, `ABORTED`, `RESOURCE_EXHAUSTED` status codes
- Max 3 retries, starting at 100ms delay, doubling each time

### Testing Strategy

- **Unit tests**: Test each module independently with mocks (target >80% coverage)
- **Integration tests**: Test against local Tercen instance (use `#[ignore]` flag)
- **Visual regression tests**: Compare generated PNGs with reference images
- **Performance benchmarks**: Measure against R plot_operator baseline

### Deployment

**Docker multi-stage build** (see `docs/04_DOCKER_AND_CICD.md`):
- Builder stage: Rust 1.75-slim-bookworm with jemalloc
- Runtime stage: Debian bookworm-slim (~120-150 MB final image)
- jemalloc enabled for better memory management
- Size-optimized build (LTO, opt-level="z", stripped symbols)
- Run as non-root user (UID 1000)

**Container registry**: GitHub Container Registry (ghcr.io/tercen/ggrs_plot_operator)

**CI/CD Pipeline** (`.github/workflows/ci.yml`):
- Based on Tercen reference: [model_estimator CI](https://github.com/tercen/model_estimator/blob/main/.github/workflows/ci.yml)
- Test job: rustfmt, clippy, unit tests, doc tests (runs on all branches)
- Build job: Docker build and push (only on main branch)
- Automatic tagging: Uses `docker/metadata-action` for branch/version tags
- Build attestation: Supply chain security via GitHub attestations
- Cache strategy: Cargo (registry, index, target) + Docker layer caching (GHA)

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

1. **Authentication**: Use `AuthInterceptor` with tonic to inject Bearer token in all requests
2. **TLS Required**: Always use TLS (ClientTlsConfig) for production connections
3. **Token Refresh**: Implement automatic token refresh on `UNAUTHENTICATED` errors
4. **Streaming Throughout**: Never accumulate entire dataset in memory; process chunks as they arrive
5. **GGRS Integration**: Implement `StreamGenerator` trait for lazy data loading
6. **Progress Reporting**: Send updates to Tercen to show user feedback during long operations
7. **Resource Cleanup**: Ensure proper cleanup on errors (no partial uploads or leaked connections)

## Documentation References

- Architecture details: `docs/01_ARCHITECTURE.md`
- Implementation phases and tasks: `docs/02_IMPLEMENTATION_PLAN.md`
- gRPC API specifications and examples: `docs/03_GRPC_INTEGRATION.md`
- Docker and CI/CD setup: `docs/04_DOCKER_AND_CICD.md`
- CI/CD final configuration: `CI_FINAL.md` (based on Tercen model_estimator)
- **FINAL DESIGN**: `docs/09_FINAL_DESIGN.md` ⭐⭐⭐ **USE THIS**
- Simple streaming design: `docs/08_SIMPLE_STREAMING_DESIGN.md` (good concepts)
- ~Tercen Context design: `docs/06_CONTEXT_DESIGN.md` (Python pattern analysis - TOO COMPLEX)~
- ~Rust Context implementation: `docs/07_RUST_CONTEXT_IMPL.md` (based on C# client - TOO COMPLEX)~
- Quick build reference: `BUILD.md`
- Docker updates summary: `DOCKER_UPDATES.md`
- Tercen gRPC API: https://github.com/tercen/tercen_grpc_api
- Tercen C# Client: https://github.com/tercen/TercenCSharpClient
- Tercen Developers Guide: https://github.com/tercen/developers_guide

## Code Quality Standards

- Follow Rust API guidelines
- Use `rustfmt` for formatting (configure in `rustfmt.toml`)
- Pass `clippy` lints (configure in `clippy.toml`)
- Write rustdoc comments for all public APIs
- Maintain test coverage >80%
- Use semantic commit messages
- Feature branches with pull requests for all changes
