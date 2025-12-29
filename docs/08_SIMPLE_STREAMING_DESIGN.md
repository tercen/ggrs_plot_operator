# Simple Streaming Design - Direct Tercen to GGRS

## The GGRS StreamGenerator Pattern

GGRS already has a `StreamGenerator` trait that queries data on-demand:

```rust
pub trait StreamGenerator: Send + Sync {
    // Query facet metadata
    fn n_col_facets(&self) -> usize;
    fn n_row_facets(&self) -> usize;
    fn query_col_facet_stream(&self, range: Range) -> Result<DataFrame>;
    fn query_row_facet_stream(&self, range: Range) -> Result<DataFrame>;

    // Query axis metadata (min/max for each facet cell)
    fn query_x_axis(&self, col_idx: usize, row_idx: usize) -> Result<AxisData>;
    fn query_y_axis(&self, col_idx: usize, row_idx: usize) -> Result<AxisData>;

    // Query actual plot data per facet cell
    fn query_data(&self, col_idx: usize, row_idx: usize) -> Result<DataFrame>;

    // Get specs
    fn facet_spec(&self) -> &FacetSpec;
    fn aes(&self) -> &crate::aes::Aes;
}
```

**Key insight**: GGRS calls `query_data()` for each facet cell as it renders. We just need to implement this trait to stream directly from Tercen!

## Simpler Approach - No Apache Arrow, No Context

### 1. Direct gRPC Streaming

```rust
// src/tercen_stream_generator.rs

use ggrs_core::{StreamGenerator, DataFrame, FacetSpec, Aes, AxisData};
use tonic::transport::Channel;

pub struct TercenStreamGenerator {
    // gRPC clients
    table_service: TableSchemaServiceClient<Channel>,

    // Task info
    table_id: String,
    crosstab_spec: CrosstabSpec,

    // GGRS specs
    aes: Aes,
    facet_spec: FacetSpec,
}

impl TercenStreamGenerator {
    pub async fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        // 1. Read environment
        let uri = std::env::var("TERCEN_URI")?;
        let username = std::env::var("TERCEN_USERNAME")?;
        let password = std::env::var("TERCEN_PASSWORD")?;
        let task_id = std::env::var("TERCEN_TASK_ID")?;

        // 2. Connect and authenticate
        let channel = create_channel(&uri).await?;
        let token = authenticate(&channel, &username, &password).await?;

        // 3. Get task and table info
        let task_service = TaskServiceClient::new(channel.clone());
        let task = task_service.get(GetRequest { id: task_id, .. }).await?.into_inner();
        let computation_task = extract_computation_task(task)?;

        // 4. Create table service with auth
        let table_service = TableSchemaServiceClient::with_interceptor(
            channel,
            AuthInterceptor { token }
        );

        // 5. Map crosstab to GGRS specs
        let aes = aes_from_crosstab(&computation_task.crosstab_spec)?;
        let facet_spec = facet_spec_from_crosstab(&computation_task.crosstab_spec)?;

        Ok(Self {
            table_service,
            table_id: computation_task.input_table_id,
            crosstab_spec: computation_task.crosstab_spec,
            aes,
            facet_spec,
        })
    }
}

impl StreamGenerator for TercenStreamGenerator {
    fn query_data(&self, col_idx: usize, row_idx: usize) -> Result<DataFrame> {
        // This is where the magic happens!
        // GGRS calls this for each facet cell as it needs to render

        // Stream data from Tercen filtered by facet cell
        let data = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.stream_facet_data(col_idx, row_idx).await
            })
        })?;

        // Convert to GGRS DataFrame (simple format)
        convert_to_ggrs_dataframe(data)
    }

    fn n_col_facets(&self) -> usize {
        // Read from crosstab_spec or query Tercen
        count_col_facets(&self.crosstab_spec)
    }

    fn n_row_facets(&self) -> usize {
        count_row_facets(&self.crosstab_spec)
    }

    fn query_x_axis(&self, col_idx: usize, row_idx: usize) -> Result<AxisData> {
        // Query min/max for this facet cell's x data
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.query_axis_range("x", col_idx, row_idx).await
            })
        })
    }

    fn query_y_axis(&self, col_idx: usize, row_idx: usize) -> Result<AxisData> {
        // Query min/max for this facet cell's y data
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.query_axis_range("y", col_idx, row_idx).await
            })
        })
    }

    fn facet_spec(&self) -> &FacetSpec {
        &self.facet_spec
    }

    fn aes(&self) -> &Aes {
        &self.aes
    }

    // ... other methods
}

impl TercenStreamGenerator {
    async fn stream_facet_data(&self, col_idx: usize, row_idx: usize) -> Result<Vec<u8>> {
        // Build filter for this specific facet cell
        // e.g., WHERE .ci = col_idx AND .ri = row_idx

        let request = ReqStreamTable {
            table_id: self.table_id.clone(),
            cnames: vec![".x".to_string(), ".y".to_string()],
            offset: 0,
            limit: 0,
            binary_format: "csv".to_string(), // Simple CSV, no Arrow needed!
        };

        // Stream chunks from Tercen
        let mut stream = self.table_service
            .stream_table(request)
            .await?
            .into_inner();

        // Collect chunks (they're already filtered by Tercen)
        let mut data = Vec::new();
        while let Some(chunk) = stream.message().await? {
            data.extend_from_slice(&chunk.result);
        }

        Ok(data)
    }
}
```

### 2. Simple CSV Parsing (No Arrow!)

```rust
// Just parse CSV directly - it's simple and works!

fn convert_to_ggrs_dataframe(csv_bytes: Vec<u8>) -> Result<DataFrame> {
    let csv_str = String::from_utf8(csv_bytes)?;
    let mut reader = csv::Reader::from_reader(csv_str.as_bytes());

    let headers = reader.headers()?.clone();
    let mut columns = HashMap::new();

    // Initialize columns
    for header in headers.iter() {
        columns.insert(header.to_string(), Vec::new());
    }

    // Read rows
    for result in reader.records() {
        let record = result?;
        for (i, field) in record.iter().enumerate() {
            let header = &headers[i];
            let value = parse_value(field);
            columns.get_mut(header).unwrap().push(value);
        }
    }

    DataFrame::new(columns)
}

fn parse_value(s: &str) -> Value {
    // Try to parse as number first
    if let Ok(n) = s.parse::<f64>() {
        Value::Numeric(n)
    } else {
        Value::String(s.to_string())
    }
}
```

### 3. Minimal Main Application

```rust
// src/main.rs

use ggrs_core::{PlotGenerator, EnginePlotSpec, ImageRenderer, Geom, Theme};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create stream generator (reads env, connects to Tercen)
    let stream_gen = TercenStreamGenerator::from_env().await?;

    // 2. Create plot spec
    let plot_spec = EnginePlotSpec::new()
        .title("Tercen Plot")
        .add_layer(Geom::point())
        .theme(Theme::gray());

    // 3. Create plot generator
    let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;

    // 4. Create renderer and render
    let renderer = ImageRenderer::new(plot_gen, 800, 600);
    let png_bytes = renderer.render_to_buffer()?;

    // 5. Upload PNG to Tercen
    upload_to_tercen(&png_bytes).await?;

    println!("Plot generated and uploaded!");
    Ok(())
}
```

## How It Works

1. **GGRS asks for data**: `query_data(col_idx=0, row_idx=0)`
2. **We stream from Tercen**: Filter by `.ci = 0 AND .ri = 0`
3. **Parse CSV to DataFrame**: Simple, no Arrow complexity
4. **GGRS renders**: Immediately renders this chunk
5. **Repeat for next facet**: `query_data(col_idx=1, row_idx=0)`, etc.

## Benefits

✅ **No Context abstraction**: Direct gRPC calls
✅ **No Apache Arrow**: Simple CSV parsing
✅ **No buffering**: Stream directly from Tercen to GGRS
✅ **Lazy loading**: Only fetch data GGRS actually needs
✅ **Progressive rendering**: Each facet cell renders as data arrives
✅ **Minimal dependencies**: Just tonic, csv, ggrs-core

## Progressive Rendering

GGRS can update the plot as new data arrives:

```rust
// GGRS calls query_data() for each facet cell
// We can provide partial data and GGRS will render what it has

impl StreamGenerator for TercenStreamGenerator {
    fn query_data(&self, col_idx: usize, row_idx: usize) -> Result<DataFrame> {
        // Option 1: Stream all data for this facet cell
        self.stream_all_data(col_idx, row_idx).await

        // Option 2: Stream first N points, render, then update
        // (depends on GGRS progressive rendering support)
        self.stream_partial_data(col_idx, row_idx, limit=1000).await
    }
}
```

## Data Flow

```
┌─────────────────────────────────────────────────┐
│ Tercen Platform                                 │
│  - Stores data in tables                        │
│  - Provides TableSchemaService.streamTable()    │
└─────────────────┬───────────────────────────────┘
                  │
                  │ gRPC stream (CSV chunks)
                  │ Filtered by .ci, .ri
                  │
┌─────────────────▼───────────────────────────────┐
│ TercenStreamGenerator                           │
│  - Implements StreamGenerator trait             │
│  - query_data(col_idx, row_idx)                 │
│  - Streams from Tercen on-demand                │
└─────────────────┬───────────────────────────────┘
                  │
                  │ DataFrame (GGRS format)
                  │ Per facet cell
                  │
┌─────────────────▼───────────────────────────────┐
│ GGRS PlotGenerator                              │
│  - Calls query_data() for each facet           │
│  - Renders as data arrives                     │
└─────────────────┬───────────────────────────────┘
                  │
                  │ Rendered pixels
                  │
┌─────────────────▼───────────────────────────────┐
│ GGRS ImageRenderer                              │
│  - Produces PNG                                 │
└─────────────────┬───────────────────────────────┘
                  │
                  │ PNG bytes
                  │
┌─────────────────▼───────────────────────────────┐
│ Upload to Tercen FileService                    │
└─────────────────────────────────────────────────┘
```

## Comparison: Complex vs Simple

### Complex Approach (What We Designed Before)
```rust
// Too complex!
TercenFactory → TercenContext → DataFrame (Arrow)
  → Convert to GGRS DataFrame → TercenStreamGenerator
```

### Simple Approach (What We Should Do)
```rust
// Much simpler!
gRPC → TercenStreamGenerator (implements StreamGenerator)
  → GGRS renders directly
```

## Dependencies

```toml
[dependencies]
# gRPC
tonic = { version = "0.11", features = ["tls"] }
prost = "0.12"

# Async
tokio = { version = "1.35", features = ["full"] }

# CSV parsing (simple!)
csv = "1.3"

# GGRS
ggrs-core = { path = "../ggrs/crates/ggrs-core" }

# Error handling
thiserror = "1.0"
anyhow = "1.0"
```

**No Arrow dependency needed!**

## Implementation Plan

1. **Phase 1**: Basic TercenStreamGenerator
   - Connect to Tercen via gRPC
   - Implement `query_data()` with CSV streaming
   - Parse CSV to GGRS DataFrame

2. **Phase 2**: Faceting support
   - Count facets from crosstab spec
   - Filter data by `.ci`, `.ri`
   - Implement `query_x_axis()`, `query_y_axis()`

3. **Phase 3**: Upload results
   - Render PNG with GGRS
   - Upload via FileService

4. **Phase 4**: Optimization
   - Cache axis ranges
   - Batch facet queries if beneficial

## Questions

1. **Does GGRS support progressive rendering?** Can we call `query_data()` multiple times with more data?
2. **Facet filtering**: How does Tercen filter by `.ci`, `.ri`? Do we need to filter client-side?
3. **Axis ranges**: Should we pre-query all axis ranges or compute per facet?

## Summary

**Don't overcomplicate it!**

- No Context abstraction needed
- No Apache Arrow complexity
- Just implement `StreamGenerator` trait
- Stream CSV directly from Tercen to GGRS
- Let GGRS handle the rendering on-demand

This is the **simplest possible approach** that leverages GGRS's existing streaming architecture.
