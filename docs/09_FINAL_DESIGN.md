# GGRS Plot Operator - Final Design

## Data Structure Understanding

Based on `/home/thiago/workspaces/tercen/main/ggrs/data/`:

### Main Data Table (`qt.csv`-like)
```csv
.colorLevels,.ci,.x,.y,.sids,.axisIndex,sp,.ri,.colorHash,.xs,.ys,.y0s
0,0,51.0,6.1,850,0,"B",0,8713,33422,422,65535
0,0,52.0,7.7,851,0,"B",0,8713,34077,7187,65535
...
```

**Key columns**:
- `.ci`: Column index (column facet identifier)
- `.ri`: Row index (row facet identifier)
- `.x`: X-axis value
- `.y`: Y-axis value
- `sp`: Color grouping variable
- Other columns: Additional aesthetics/metadata

### Column Facet Table (`column.csv`)
```csv
sp
B
O
```

Defines unique values for column facets. Each unique value = one column in the faceted plot.

### Row Facet Table (`row.csv`)
```csv
variable,sex
BD,F
BD,M
CL,F
CL,M
...
```

Defines unique combinations for row facets. Each unique combination = one row in the faceted plot.

## Complete Architecture

### 1. Streaming from Tercen (gRPC API)

**ReqStreamTable** supports:
```protobuf
message ReqStreamTable {
  string tableId = 1;        // Table to query
  repeated string cnames = 2; // Column names to retrieve
  int64 offset = 3;          // Row offset (for chunking!)
  int64 limit = 4;           // Number of rows (chunk size!)
  string binaryFormat = 5;   // "csv" or "arrow"
}
```

✅ **Perfect!** We can chunk with `offset` and `limit`.

### 2. TercenStreamGenerator Implementation

```rust
// src/tercen_stream_generator.rs

use ggrs_core::{StreamGenerator, DataFrame, Aes, FacetSpec, AxisData, Range};
use tonic::transport::Channel;

pub struct TercenStreamGenerator {
    // gRPC client
    table_service: TableSchemaServiceClient<Channel>,

    // Table IDs
    main_table_id: String,
    col_facet_table_id: String,
    row_facet_table_id: String,

    // Metadata (loaded once at start)
    col_facets: Vec<FacetGroup>,   // From column.csv
    row_facets: Vec<FacetGroup>,   // From row.csv
    axis_ranges: HashMap<(usize, usize), (AxisData, AxisData)>, // Cached ranges

    // GGRS configuration
    aes: Aes,
    facet_spec: FacetSpec,

    // Chunking config
    chunk_size: usize,  // e.g., 10_000 rows
}

#[derive(Clone)]
struct FacetGroup {
    index: usize,
    label: String,
    // For multi-variable facets, this could be a combination
}

impl TercenStreamGenerator {
    pub async fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        // 1. Connect to Tercen
        let uri = std::env::var("TERCEN_URI")?;
        let username = std::env::var("TERCEN_USERNAME")?;
        let password = std::env::var("TERCEN_PASSWORD")?;
        let task_id = std::env::var("TERCEN_TASK_ID")?;

        let channel = connect_and_auth(&uri, &username, &password).await?;
        let table_service = TableSchemaServiceClient::new(channel.clone());

        // 2. Get task and table IDs
        let task_service = TaskServiceClient::new(channel);
        let task = get_computation_task(&task_service, &task_id).await?;

        // 3. Load facet metadata (small tables, load completely)
        let col_facets = load_col_facets(&table_service, &task.col_facet_table_id).await?;
        let row_facets = load_row_facets(&table_service, &task.row_facet_table_id).await?;

        // 4. Pre-compute axis ranges for all facet cells
        let axis_ranges = compute_all_axis_ranges(
            &table_service,
            &task.main_table_id,
            &col_facets,
            &row_facets
        ).await?;

        // 5. Build GGRS specs from crosstab
        let aes = aes_from_crosstab(&task.crosstab_spec)?;
        let facet_spec = facet_spec_from_crosstab(&task.crosstab_spec, &col_facets, &row_facets)?;

        Ok(Self {
            table_service,
            main_table_id: task.main_table_id,
            col_facet_table_id: task.col_facet_table_id,
            row_facet_table_id: task.row_facet_table_id,
            col_facets,
            row_facets,
            axis_ranges,
            aes,
            facet_spec,
            chunk_size: 10_000,
        })
    }
}

impl StreamGenerator for TercenStreamGenerator {
    fn n_col_facets(&self) -> usize {
        self.col_facets.len()
    }

    fn n_row_facets(&self) -> usize {
        self.row_facets.len()
    }

    fn query_col_facet_stream(&self, range: Range) -> Result<DataFrame> {
        // Return facet labels for column facets in range
        let facets = &self.col_facets[range.start..range.end];
        DataFrame::from_facet_groups(facets)
    }

    fn query_row_facet_stream(&self, range: Range) -> Resul
    t<DataFrame> {
        // Return facet labels for row facets in range
        let facets = &self.row_facets[range.start..range.end];
        DataFrame::from_facet_groups(facets)
    }

    fn query_x_axis(&self, col_idx: usize, row_idx: usize) -> Result<AxisData> {
        // Return cached axis range
        Ok(self.axis_ranges[&(col_idx, row_idx)].0.clone())
    }

    fn query_y_axis(&self, col_idx: usize, row_idx: usize) -> Result<AxisData> {
        // Return cached axis range
        Ok(self.axis_ranges[&(col_idx, row_idx)].1.clone())
    }

    fn query_data(&self, col_idx: usize, row_idx: usize) -> Result<DataFrame> {
        // THIS IS THE KEY METHOD!
        // GGRS calls this for each facet cell
        // We need to stream ALL data for this cell in chunks

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.stream_all_facet_data(col_idx, row_idx).await
            })
        })
    }

    fn facet_spec(&self) -> &FacetSpec {
        &self.facet_spec
    }

    fn aes(&self) -> &Aes {
        &self.aes
    }
}

impl TercenStreamGenerator {
    /// Stream all data for a specific facet cell in chunks
    async fn stream_all_facet_data(&self, col_idx: usize, row_idx: usize) -> Result<DataFrame> {
        let mut all_chunks = Vec::new();
        let mut offset = 0;

        loop {
            // Request chunk from Tercen
            let request = ReqStreamTable {
                table_id: self.main_table_id.clone(),
                cnames: vec![".x".to_string(), ".y".to_string(), ".ci".to_string(), ".ri".to_string()],
                offset,
                limit: self.chunk_size as i64,
                binary_format: "csv".to_string(),
            };

            let response = self.table_service.clone().stream_table(request).await?;
            let mut stream = response.into_inner();

            // Collect this chunk's data
            let mut chunk_data = Vec::new();
            while let Some(response) = stream.message().await? {
                chunk_data.extend_from_slice(&response.result);
            }

            if chunk_data.is_empty() {
                break; // No more data
            }

            // Parse CSV and filter by facet indices
            let chunk_df = parse_csv_to_dataframe(&chunk_data)?;
            let filtered = chunk_df.filter_by_facet(col_idx, row_idx)?;

            if !filtered.is_empty() {
                all_chunks.push(filtered);
            }

            offset += self.chunk_size as i64;

            // If chunk was smaller than requested, we're done
            if chunk_data.len() < self.chunk_size {
                break;
            }
        }

        // Concatenate all chunks
        DataFrame::concatenate(all_chunks)
    }
}

/// Helper: Load column facets (small table)
async fn load_col_facets(
    client: &TableSchemaServiceClient<Channel>,
    table_id: &str,
) -> Result<Vec<FacetGroup>> {
    let request = ReqStreamTable {
        table_id: table_id.to_string(),
        cnames: vec![], // All columns
        offset: 0,
        limit: 0, // All rows (it's small)
        binary_format: "csv".to_string(),
    };

    let response = client.clone().stream_table(request).await?;
    let mut stream = response.into_inner();

    let mut data = Vec::new();
    while let Some(chunk) = stream.message().await? {
        data.extend_from_slice(&chunk.result);
    }

    let df = parse_csv_to_dataframe(&data)?;
    Ok(df_to_facet_groups(df))
}
```

### 3. Incremental Rendering (GGRS Modification Needed?)

**Current GGRS behavior** (based on tests):
```rust
// PlotGenerator calls query_data() once per facet cell
let data = stream_gen.query_data(col_idx, row_idx)?;
// Then renders all points at once
```

**Needed behavior for incremental rendering**:

**Option A**: Multiple render passes (if GGRS supports)
```rust
// Pass 1: First chunk
let chunk1 = stream_gen.query_data(col_idx, row_idx)?; // Returns 10k points
plot_gen.render_chunk(col_idx, row_idx, chunk1)?;

// Pass 2: Next chunk
let chunk2 = stream_gen.query_data(col_idx, row_idx)?; // Returns next 10k
plot_gen.add_to_render(col_idx, row_idx, chunk2)?;
```

**Option B**: Stream within query_data()
```rust
impl StreamGenerator for TercenStreamGenerator {
    fn query_data(&self, col_idx, row_idx) -> Result<DataFrame> {
        // Return generator/iterator that yields chunks
        // But current trait returns DataFrame, not Iterator...
    }
}
```

**Recommendation**: For now, load all data for a facet cell (it's filtered, so smaller). Later, we can modify GGRS to support:
```rust
fn query_data_chunked(&self, col_idx, row_idx) -> impl Iterator<Item=Result<DataFrame>>
```

### 4. Output: PNG as Base64 to Tercen Table

```rust
// After rendering
let png_bytes = renderer.render_to_buffer()?;

// Encode to base64
let base64_string = base64::encode(&png_bytes);

// Create result DataFrame
let result_df = DataFrame::new(vec![
    (".content", vec![Value::String(base64_string)]),
    ("filename", vec![Value::String("plot.png".to_string())]),
    ("mimetype", vec![Value::String("image/png".to_string())]),
]);

// Save to Tercen
save_result_table(table_service, task_id, result_df).await?;
```

**Tercen table schema**:
```csv
.content,filename,mimetype
iVBORw0KGgoAAAANS...,plot.png,image/png
```

### 5. Complete Main Flow

```rust
// src/main.rs

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create stream generator (connects, loads metadata)
    println!("Connecting to Tercen...");
    let stream_gen = TercenStreamGenerator::from_env().await?;

    println!("Loaded {} column facets, {} row facets",
        stream_gen.n_col_facets(),
        stream_gen.n_row_facets()
    );

    // 2. Get operator properties
    let width = get_property("width", 800)?;
    let height = get_property("height", 600)?;
    let theme_name = get_property("theme", "gray")?;

    // 3. Create plot spec
    let plot_spec = EnginePlotSpec::new()
        .title(get_property("title", "Plot")?)
        .add_layer(Geom::point())
        .theme(Theme::from_name(&theme_name));

    // 4. Create plot generator
    println!("Generating plot...");
    let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;

    // 5. Render (GGRS will call query_data for each facet cell)
    let renderer = ImageRenderer::new(plot_gen, width, height);

    // Progress callback (if GGRS supports)
    renderer.set_progress_callback(|progress| {
        println!("Rendering: {:.1}%", progress * 100.0);
    });

    let png_bytes = renderer.render_to_buffer()?;
    println!("Rendered {} bytes", png_bytes.len());

    // 6. Encode to base64
    let base64_string = base64::encode(&png_bytes);

    // 7. Create result table
    let result_df = DataFrame::new(vec![
        (".content", vec![Value::String(base64_string)]),
        ("filename", vec![Value::String("plot.png".to_string())]),
        ("mimetype", vec![Value::String("image/png".to_string())]),
    ]);

    // 8. Save to Tercen
    println!("Uploading result...");
    save_result_to_tercen(result_df).await?;

    println!("Done!");
    Ok(())
}
```

## Implementation Phases

### Phase 1: Basic Connection and Metadata
1. Connect to Tercen via gRPC
2. Authenticate
3. Get task and table IDs
4. Load col/row facet tables
5. Print metadata to verify

### Phase 2: Simple Data Streaming
1. Stream main data table with offset/limit
2. Parse CSV
3. Filter by `.ci`, `.ri`
4. Test with one facet cell

### Phase 3: Axis Range Computation
1. Query min/max for each facet cell
2. Cache axis ranges
3. Implement `query_x_axis()`, `query_y_axis()`

### Phase 4: GGRS Integration
1. Implement full StreamGenerator trait
2. Map crosstab spec to Aes and FacetSpec
3. Create PlotGenerator and render
4. Test locally with GGRS

### Phase 5: Result Upload
1. Encode PNG to base64
2. Create result DataFrame
3. Upload to Tercen as table
4. Verify in Tercen UI

### Phase 6: Optimization
1. Parallel facet rendering (if GGRS supports)
2. Progress reporting
3. Error handling
4. Memory optimization

## Key Questions for GGRS

1. **Incremental rendering**: Can PlotGenerator render chunks progressively?
2. **Progress callbacks**: Can we get progress during rendering?
3. **Memory**: Does ImageRenderer hold all data or can it work with streams?

## Dependencies

```toml
[dependencies]
# gRPC
tonic = { version = "0.11", features = ["tls"] }
prost = "0.12"

# Async
tokio = { version = "1.35", features = ["full"] }

# CSV parsing
csv = "1.3"

# Base64 encoding
base64 = "0.21"

# GGRS
ggrs-core = { path = "../ggrs/crates/ggrs-core" }

# Error handling
thiserror = "1.0"
anyhow = "1.0"
```

## Summary

✅ **Data structure**: Clear from example files
✅ **gRPC streaming**: `ReqStreamTable` supports offset/limit chunking
✅ **Output format**: `.content` (base64), `filename`, `mimetype` columns
✅ **Architecture**: Direct StreamGenerator implementation, no complex Context

**Next**: Implement Phase 1 and verify connection to Tercen!
