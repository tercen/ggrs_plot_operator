# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

The **ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with the Tercen platform. It receives tabular data through the Tercen gRPC API, generates high-performance plots using GGRS, and returns PNG images back to Tercen for visualization.

**Current Status**: Phase 1 complete - basic operator structure with CI/CD. Ready for Phase 2 (gRPC integration).

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
│   ├── main.rs                  # Entry point (Phase 1 - minimal)
│   ├── tercen/                  # Future Tercen gRPC client library
│   │   ├── mod.rs               # Module stubs (empty)
│   │   └── README.md            # Extraction plan
│   └── ggrs_integration/        # GGRS integration code
│       └── mod.rs               # Module stubs (empty)
├── docs/
│   ├── 09_FINAL_DESIGN.md       # ⭐ PRIMARY: Complete architecture
│   ├── 10_IMPLEMENTATION_PHASES.md # Current implementation roadmap
│   ├── 03_GRPC_INTEGRATION.md   # gRPC API specifications
│   └── [other docs]             # Historical design iterations
├── Cargo.toml                   # Minimal deps (tokio, jemalloc)
├── Cargo.toml.template          # Full deps for Phase 2+
├── Dockerfile                   # Multi-stage Docker build
├── .github/workflows/ci.yml     # CI/CD pipeline
├── operator.json                # Tercen operator specification
├── BUILD.md                     # Comprehensive build guide
└── CLAUDE.md                    # This file
```

**Key files to read first**: `docs/09_FINAL_DESIGN.md`, `docs/10_IMPLEMENTATION_PHASES.md`

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

**Current Phase**: Phase 1 ✅ COMPLETED → Starting Phase 2

### Phase 1 Completed ✅
- ✅ `operator.json` with properties (width, height, theme, title)
- ✅ `Cargo.toml` with minimal deps (tokio, jemalloc)
- ✅ `src/main.rs` - prints version, checks environment vars
- ✅ Dockerfile with multi-stage build
- ✅ CI/CD workflow (`.github/workflows/ci.yml`) - test + build jobs
- ✅ Successfully builds and runs locally
- ✅ Module structure prepared (`tercen/`, `ggrs_integration/`)

### Next Steps (Phase 2)
**Goal**: Establish gRPC connection to Tercen and make first service call

**Key tasks**:
1. Copy proto files from `/home/thiago/workspaces/tercen/main/sci/tercen_grpc/tercen_grpc_api/protos/`
2. Create `build.rs` with `tonic-build` configuration
3. Update `Cargo.toml` with full dependencies (see `Cargo.toml.template`)
4. Implement `TercenClient::from_env()` for authentication
5. Make first service call (TaskService.getTask)

### Complete Roadmap

See `docs/10_IMPLEMENTATION_PHASES.md` for details:
- Phase 1: CI/CD and Basic Operator Structure ✅
- Phase 2: gRPC Connection and Simple Call ⏭️ NEXT
- Phase 3: Streaming Data - Test Chunking
- Phase 4: Data Parsing and Filtering
- Phase 5: Load Facet Metadata
- Phase 6: First GGRS Plot
- Phase 7: Output to Tercen Table
- Phase 8: Full Faceting Support
- Phase 9: Production Polish

### Module Structure (Designed for Library Extraction)

The `src/tercen/` module is intentionally isolated to enable future extraction as a separate `tercen-rust` crate:

```
src/tercen/              # ⭐ Future tercen-rust crate
├── client.rs            # TercenClient with connection and auth
├── error.rs             # TercenError type
├── types.rs             # Common types and conversions
└── services/
    ├── task.rs          # TaskService wrapper
    ├── table.rs         # TableSchemaService wrapper
    └── file.rs          # FileService wrapper

src/ggrs_integration/    # GGRS-specific (stays in this project)
├── stream_generator.rs  # TercenStreamGenerator impl
├── plot_builder.rs      # EnginePlotSpec builder
└── renderer.rs          # ImageRenderer wrapper
```

**Design benefits**: Clear separation, no GGRS deps in `tercen/`, easy extraction. See `src/tercen/README.md` for details.

## Development Workflow

### Proto Files Setup

Before Phase 2, proto files must be copied from the main Tercen gRPC repository:

```bash
# Source location (adjust path as needed)
SOURCE=/home/thiago/workspaces/tercen/main/sci/tercen_grpc/tercen_grpc_api/protos

# Copy to project
mkdir -p protos
cp $SOURCE/tercen.proto protos/
cp $SOURCE/tercen_model.proto protos/
```

### Dependency Management

- `Cargo.toml`: Current minimal dependencies (Phase 1)
- `Cargo.toml.template`: Full dependencies for Phase 2+ (tonic, prost, csv, etc.)
- When starting Phase 2, review and merge template into `Cargo.toml`

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

1. **Authentication**: Use `AuthInterceptor` with tonic to inject Bearer token in all requests
2. **TLS Required**: Always use TLS (ClientTlsConfig) for production connections
3. **Token Refresh**: Implement automatic token refresh on `UNAUTHENTICATED` errors
4. **Streaming Throughout**: Never accumulate entire dataset in memory; process chunks as they arrive
5. **GGRS Integration**: Implement `StreamGenerator` trait for lazy data loading
6. **Progress Reporting**: Send updates to Tercen to show user feedback during long operations
7. **Resource Cleanup**: Ensure proper cleanup on errors (no partial uploads or leaked connections)

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

## Code Quality Standards

- Follow Rust API guidelines
- Use `rustfmt` for formatting (configure in `rustfmt.toml`)
- Pass `clippy` lints (configure in `clippy.toml`)
- Write rustdoc comments for all public APIs
- Maintain test coverage >80%
- Use semantic commit messages
- Feature branches with pull requests for all changes
