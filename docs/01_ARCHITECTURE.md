# GGRS Plot Operator Architecture

## Overview

The **ggrs_plot_operator** is a Tercen operator that integrates the GGRS plotting library with the Tercen platform. It will receive tabular data through the Tercen gRPC API, generate high-performance plots using GGRS, and return PNG images (and potentially other formats) back to Tercen for visualization and storage.

## System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Tercen Platform                          │
│  ┌────────────┐      ┌──────────────┐      ┌────────────────┐  │
│  │  Workflow  │─────▶│ Computation  │─────▶│  Result/Image  │  │
│  │   Engine   │      │     Task     │      │    Storage     │  │
│  └────────────┘      └──────┬───────┘      └────────────────┘  │
│                             │ gRPC                               │
└─────────────────────────────┼──────────────────────────────────┘
                              │
                              │ (1) Task request
                              │ (2) Data query via TableSchemaService
                              │ (3) PNG upload via FileService
                              │
┌─────────────────────────────▼──────────────────────────────────┐
│                    GGRS Plot Operator                           │
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐ │
│  │                  gRPC Client Layer                         │ │
│  │  - TaskService: Receive & execute computation tasks       │ │
│  │  - TableSchemaService: Query input data streams           │ │
│  │  - FileService: Upload generated PNG images               │ │
│  │  - Authentication: Token-based auth                       │ │
│  └─────────────────────────┬─────────────────────────────────┘ │
│                            │                                     │
│  ┌─────────────────────────▼─────────────────────────────────┐ │
│  │              Data Transformation Layer                     │ │
│  │  - Parse Tercen data streams (CSV/binary format)          │ │
│  │  - Convert to GGRS DataFrame                              │ │
│  │  - Extract aesthetic mappings from operator config        │ │
│  │  - Handle faceting specifications                         │ │
│  └─────────────────────────┬─────────────────────────────────┘ │
│                            │                                     │
│  ┌─────────────────────────▼─────────────────────────────────┐ │
│  │                    GGRS Integration                        │ │
│  │  ┌──────────────────────────────────────────────────────┐ │ │
│  │  │  StreamGenerator (Custom TercenStreamGenerator)      │ │ │
│  │  │  - Implements GGRS StreamGenerator trait             │ │ │
│  │  │  - Lazy loads data from Tercen on demand             │ │ │
│  │  │  - Handles large datasets efficiently                │ │ │
│  │  └─────────────────────┬────────────────────────────────┘ │ │
│  │                        │                                    │ │
│  │  ┌─────────────────────▼────────────────────────────────┐ │ │
│  │  │  PlotGenerator (GGRS Core)                           │ │ │
│  │  │  - Coordinates data and rendering                    │ │ │
│  │  │  - Manages faceting and scales                       │ │ │
│  │  │  - Handles color/shape/size aesthetics               │ │ │
│  │  └─────────────────────┬────────────────────────────────┘ │ │
│  │                        │                                    │ │
│  │  ┌─────────────────────▼────────────────────────────────┐ │ │
│  │  │  ImageRenderer (GGRS Core)                           │ │ │
│  │  │  - Renders to PNG using Plotters backend            │ │ │
│  │  │  - Applies themes (gray, bw, minimal, custom)       │ │ │
│  │  │  - Produces final image buffer                      │ │ │
│  │  └──────────────────────────────────────────────────────┘ │ │
│  └────────────────────────┬───────────────────────────────────┘ │
│                           │                                      │
│  ┌────────────────────────▼────────────────────────────────┐   │
│  │          Result Handling & Upload                       │   │
│  │  - Compress PNG if needed                               │   │
│  │  - Upload via FileService streaming                     │   │
│  │  - Update task status                                   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

## Key Components

### 1. gRPC Client Layer

**Purpose**: Interface with Tercen's gRPC API services

**Responsibilities**:
- Authenticate using token-based authentication
- Receive computation task requests via `TaskService`
- Query input data via `TableSchemaService.streamTable()`
- Upload result images via `FileService.upload()`
- Report task progress and completion via `TaskService.update()`

**Key Services Used**:
- `TaskService`: Task lifecycle management (create, run, waitDone, update)
- `TableSchemaService`: Data streaming with chunked binary/CSV formats
- `FileService`: File upload with streaming support
- `UserService`: Authentication token generation

**Proto Files**:
- `tercen.proto`: Service definitions
- `tercen_model.proto`: Message types (ETask, EDocument, ESchema, etc.)

### 2. Data Transformation Layer

**Purpose**: Convert Tercen data formats to GGRS-compatible structures

**Input Data Structure** (from Tercen):
```
Crosstab Projection:
- Row factors (faceting rows)
- Column factors (faceting columns)
- X axis variable
- Y axis variable
- Color factors
- Label factors
- Page factors (separate files)
```

**Output** (to GGRS):
```rust
DataFrame {
    columns: HashMap<String, Vec<Value>>,
    // Value can be Numeric(f64), String(String), Int(i32), Bool(bool)
}

Aes {
    x: Option<String>,
    y: Option<String>,
    color: Option<String>,
    shape: Option<String>,
    size: Option<String>,
}

FacetSpec {
    // FacetSpec::none()
    // FacetSpec::col("variable")
    // FacetSpec::row("variable")
    // FacetSpec::grid("row_var", "col_var")
}
```

**Responsibilities**:
- Parse binary or CSV data streams from `TableSchemaService`
- Build GGRS DataFrame from columnar data
- Map Tercen crosstab axes to GGRS aesthetics
- Translate Tercen faceting to GGRS FacetSpec
- Handle operator properties (theme, scales, dimensions, etc.)

### 3. GGRS Integration

**Purpose**: Generate plots using GGRS streaming architecture

#### Custom StreamGenerator

We will implement `TercenStreamGenerator` that implements the GGRS `StreamGenerator` trait:

```rust
pub struct TercenStreamGenerator {
    table_id: String,
    grpc_client: Arc<TercenGrpcClient>,
    aes: Aes,
    facet_spec: FacetSpec,
    data_cache: Option<DataFrame>,
}

impl StreamGenerator for TercenStreamGenerator {
    fn query_cell_data(&mut self, cell_spec: &CellSpec) -> Result<DataFrame>;
    fn get_facet_spec(&self) -> &FacetSpec;
    fn get_aes(&self) -> &Aes;
}
```

**Features**:
- Lazy loading: Only fetch data when GGRS requests it
- Streaming support: Query data in chunks via gRPC
- Faceting aware: Query specific facet cells independently
- Memory efficient: Don't load entire dataset upfront

#### GGRS Core Usage

Use GGRS's existing three-layer architecture:

1. **Data Layer**: `TercenStreamGenerator` (custom)
2. **Engine Layer**: `PlotGenerator` (GGRS core, no changes)
3. **Rendering Layer**: `ImageRenderer` (GGRS core, no changes)

```rust
// Pseudo-code for plot generation
let stream_gen = TercenStreamGenerator::new(
    table_id,
    grpc_client,
    aes,
    facet_spec,
);

let plot_spec = EnginePlotSpec::new()
    .title(&operator_config.title)
    .add_layer(Geom::point())
    .theme(Theme::from_name(&operator_config.theme));

let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;
let renderer = ImageRenderer::new(plot_gen, width, height);
let png_bytes = renderer.render_to_buffer()?;
```

### 4. Result Handling & Upload

**Purpose**: Return generated images to Tercen

**Process**:
1. Receive PNG buffer from GGRS ImageRenderer
2. Create `EFileDocument` with metadata:
   - filename: Based on operator config prefix + page factors
   - mimetype: "image/png"
   - size: Buffer length
3. Stream upload via `FileService.upload()`:
   - Send `ReqUpload` messages with file metadata + chunks
   - Receive `RespUpload` with created file document
4. Link file to computation result
5. Update task status to "Done"

## Data Flow

### Typical Execution Flow

1. **Task Initialization**
   ```
   Tercen → TaskService.create(ComputationTask)
   Tercen → TaskService.runTask(taskId)
   Operator → TaskService.waitDone(taskId) // Blocking wait for task
   ```

2. **Configuration Parsing**
   ```
   Operator → Parse task.operatorSettings
     - plot_type: png/pdf/svg
     - dimensions: width, height
     - theme: gray/bw/minimal
     - aesthetics mappings
     - faceting specification
   ```

3. **Data Retrieval**
   ```
   Operator → TableSchemaService.streamTable(
     tableId,
     cnames: ["col1", "col2", ...],
     offset: 0,
     limit: 10000,
     binaryFormat: "arrow" // or CSV
   )

   Stream chunks → Build DataFrame
   ```

4. **Plot Generation**
   ```
   DataFrame + Aes + FacetSpec
     → TercenStreamGenerator
     → PlotGenerator (GGRS)
     → ImageRenderer (GGRS)
     → PNG buffer
   ```

5. **Result Upload**
   ```
   Operator → FileService.upload(stream ReqUpload)
     Chunk 1: file metadata
     Chunk 2..N: PNG bytes

   Tercen → Returns EFileDocument with file ID
   ```

6. **Task Completion**
   ```
   Operator → TaskService.update(task, state=Done)
   Tercen → Displays plot in UI
   ```

## Deployment Model

### Docker Container

The operator will be packaged as a Docker container:

```dockerfile
FROM rust:1.75-slim as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/ggrs_plot_operator /usr/local/bin/

CMD ["ggrs_plot_operator"]
```

### Operator Manifest

The operator will have an `operator.json` file defining:

```json
{
  "name": "GGRS Plot",
  "description": "High-performance plot operator using GGRS",
  "container": "ghcr.io/tercen/ggrs_plot_operator:0.1.0",
  "properties": [
    {
      "kind": "EnumeratedProperty",
      "name": "plot_type",
      "defaultValue": "png",
      "values": ["png", "svg", "pdf"]
    },
    {
      "kind": "EnumeratedProperty",
      "name": "theme",
      "defaultValue": "gray",
      "values": ["gray", "bw", "minimal"]
    },
    {
      "kind": "StringProperty",
      "name": "title",
      "defaultValue": ""
    }
    // ... more properties
  ],
  "operatorSpec": {
    "kind": "OperatorSpec",
    "inputSpecs": [
      {
        "kind": "CrosstabSpec",
        "metaFactors": [
          {
            "name": "Rows",
            "crosstabMapping": "row",
            "cardinality": "0..n"
          },
          {
            "name": "Columns",
            "crosstabMapping": "column",
            "cardinality": "0..n"
          }
        ],
        "axis": [
          {
            "kind": "AxisSpec",
            "metaFactors": [
              {
                "name": "Y Axis",
                "type": "numeric",
                "crosstabMapping": "y",
                "cardinality": "1"
              },
              {
                "name": "X Axis",
                "crosstabMapping": "x",
                "cardinality": "1"
              },
              {
                "name": "Colors",
                "crosstabMapping": "color",
                "cardinality": "0..n"
              }
            ]
          }
        ]
      }
    ]
  }
}
```

## Technology Stack

### Core Dependencies

- **Rust 1.75+**: Programming language
- **tonic** (~0.11): gRPC client implementation
- **prost** (~0.12): Protocol Buffer serialization
- **tokio** (~1.35): Async runtime
- **ggrs-core**: Plot generation library (local path dependency)
- **plotters** (~0.3): Rendering backend (via GGRS)
- **serde/serde_json**: Configuration parsing
- **thiserror**: Error handling

### Proto Compilation

Use `tonic-build` in `build.rs`:

```rust
fn main() {
    tonic_build::configure()
        .build_server(false)
        .compile(
            &["protos/tercen.proto", "protos/tercen_model.proto"],
            &["protos"],
        )
        .unwrap();
}
```

## Scalability Considerations

### Memory Efficiency

1. **Streaming Data Access**: Don't load entire table into memory
2. **Chunk Processing**: Process data in configurable chunks (default: 10K rows)
3. **Lazy Faceting**: Only load data for the facet cells being rendered

### Performance

1. **Rust Performance**: Native performance, no GC overhead
2. **GGRS Optimization**: Pre-optimized rendering pipeline
3. **Parallel Facets**: Potentially render independent facet cells in parallel
4. **GPU Acceleration**: Future WebGPU backend support in GGRS

### Large Datasets

For datasets too large to fit in memory:

1. Use GGRS streaming architecture
2. Query data per facet cell on-demand
3. Implement data sampling for preview mode
4. Support progressive rendering (future GGRS feature)

## Error Handling Strategy

### Error Types

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

### Error Recovery

- Transient gRPC errors: Retry with exponential backoff (max 3 attempts)
- Data errors: Report detailed error to user via task log events
- Configuration errors: Fail fast with clear error message
- Resource errors: Clean up partial resources before exit

### Logging & Monitoring

- Use `tracing` crate for structured logging
- Log levels: ERROR, WARN, INFO, DEBUG, TRACE
- Send progress updates via `TaskProgressEvent`
- Send log messages via `TaskLogEvent`
- Final state via `TaskStateEvent` (Done/Failed)

## Security Considerations

### Authentication

- Token-based authentication via `UserService.generateToken()`
- Tokens stored securely (environment variables or secure vault)
- Token refresh on expiration

### Data Access

- Operator only accesses data for assigned task
- No persistent storage of sensitive data
- All data transmission over TLS (gRPC)

### Container Security

- Run as non-root user
- Minimal base image (distroless or slim)
- No unnecessary tools in production image
- Regular dependency updates

## Future Enhancements

### Phase 2 Features

1. **Additional Output Formats**
   - PDF export via Cairo backend
   - SVG export for web embedding
   - Interactive HTML with GGRS-WASM

2. **Advanced GGRS Features**
   - More geometry types (lines, bars, areas, histograms)
   - Custom themes matching Tercen brand
   - Animation support for time-series

3. **Performance Optimizations**
   - Parallel facet rendering
   - Data pre-aggregation for large datasets
   - GPU acceleration via WebGPU backend

4. **Integration Enhancements**
   - Direct streaming from Tercen without intermediate DataFrame
   - Cached data access for repeated queries
   - Progressive rendering with preview mode

### Potential Challenges

1. **Color Palette Compatibility**: Tercen has custom palettes defined in `palettes.json`. Need to map these to GGRS color scales.

2. **Exact ggplot2 Equivalence**: GGRS aims for visual equivalence with ggplot2, but some edge cases may differ. Document any known differences.

3. **Memory Limits in Containers**: Large faceted plots may hit memory limits. Implement safeguards and clear error messages.

4. **Complex Crosstab Mappings**: Some Tercen crosstab configurations may be complex to map to GGRS. Start with common patterns first.

## References

- [Tercen gRPC API](https://github.com/tercen/tercen_grpc_api)
- [Tercen Developers Guide](https://github.com/tercen/developers_guide)
- [GGRS Architecture](../ggrs/docs/ARCHITECTURE.md)
- [GGRS README](../ggrs/README.md)
- [Existing Plot Operator](../plot_operator/)
