# Tercen Context Design for Rust

## Overview

This document outlines the design for a minimal Tercen context API in Rust that can query data via gRPC and be later extracted into a separate library.

## Python Pattern Analysis

From the scyan_operator example:

```python
from tercen.client import context

ctx = context.TercenContext()

# Select data with indices
yDf = ctx.select([".y", ".ci", ".ri"])  # values + column/row indices

# Select column metadata
colDf = ctx.cselect([""])  # column annotations

# Select row metadata
rowDf = ctx.rselect([""])  # row annotations

# Get operator properties
learning_rate = ctx.operator_property('learning_rate', typeFn=float, default=0.001)

# Logging
ctx.log("Processing data...")

# Save results
ctx.save_relation(result_df)
```

**Key Observations**:
1. Context is initialized from task (likely from environment variable)
2. Three selection methods: `select()`, `cselect()`, `rselect()`
3. Data returned as DataFrames (Polars/Pandas)
4. Special columns: `.y`, `.ci` (column index), `.ri` (row index)
5. Operator properties retrieved with type conversion
6. Results saved back to Tercen

## Rust Context API Design

### Goals

1. **Minimal**: Only what's needed for GGRS plot operator
2. **Portable**: Easy to extract to separate crate later
3. **Streaming**: Efficient for large datasets
4. **Ergonomic**: Rust-idiomatic API

### Core Types

```rust
// src/tercen/mod.rs (can be moved to separate crate later)

pub mod context;
pub mod data;
pub mod error;

// Re-exports
pub use context::TercenContext;
pub use data::{DataFrame, Value};
pub use error::TercenError;
```

### TercenContext API

```rust
// src/tercen/context.rs

use crate::grpc_client::TercenGrpcClient;
use std::sync::Arc;

pub struct TercenContext {
    client: Arc<TercenGrpcClient>,
    task_id: String,
    computation_task: ComputationTask,
}

impl TercenContext {
    /// Create context from environment (like Python version)
    pub async fn from_env() -> Result<Self, TercenError> {
        let task_id = std::env::var("TERCEN_TASK_ID")?;
        let endpoint = std::env::var("TERCEN_ENDPOINT")?;
        let username = std::env::var("TERCEN_USERNAME")?;
        let password = std::env::var("TERCEN_PASSWORD")?;

        let client = TercenGrpcClient::new(&endpoint, &username, &password).await?;
        Self::new(Arc::new(client), task_id).await
    }

    /// Create context from task ID
    pub async fn new(client: Arc<TercenGrpcClient>, task_id: String) -> Result<Self, TercenError> {
        // Get computation task from task service
        let computation_task = client.get_computation_task(&task_id).await?;

        Ok(Self {
            client,
            task_id,
            computation_task,
        })
    }

    /// Select data columns (equivalent to ctx.select([".y", ".ci", ".ri"]))
    /// Returns values with column/row indices
    pub async fn select(&self, columns: &[&str]) -> Result<DataFrame, TercenError> {
        self.select_impl(columns, SelectMode::Data).await
    }

    /// Select column metadata (equivalent to ctx.cselect([""]))
    pub async fn cselect(&self, columns: &[&str]) -> Result<DataFrame, TercenError> {
        self.select_impl(columns, SelectMode::ColumnMeta).await
    }

    /// Select row metadata (equivalent to ctx.rselect([""]))
    pub async fn rselect(&self, columns: &[&str]) -> Result<DataFrame, TercenError> {
        self.select_impl(columns, SelectMode::RowMeta).await
    }

    /// Stream data in chunks for large datasets
    pub fn select_stream(&self, columns: &[&str]) -> impl Stream<Item = Result<DataFrame, TercenError>> {
        // Returns async stream for processing data in chunks
        todo!()
    }

    /// Get operator property with type conversion
    pub fn operator_property<T>(&self, name: &str, default: T) -> Result<T, TercenError>
    where
        T: FromStr + Default,
    {
        // Parse from computation_task.operator_settings
        todo!()
    }

    /// Log message to Tercen
    pub async fn log(&self, message: &str) -> Result<(), TercenError> {
        // Send TaskLogEvent via task service
        todo!()
    }

    /// Update progress (0.0 to 1.0)
    pub async fn progress(&self, progress: f64, message: &str) -> Result<(), TercenError> {
        // Send TaskProgressEvent
        todo!()
    }

    /// Get column names
    pub fn cnames(&self) -> Vec<String> {
        // Parse from crosstab spec
        todo!()
    }

    /// Get row names
    pub fn rnames(&self) -> Vec<String> {
        // Parse from crosstab spec
        todo!()
    }

    // Internal implementation
    async fn select_impl(
        &self,
        columns: &[&str],
        mode: SelectMode,
    ) -> Result<DataFrame, TercenError> {
        let table_id = match mode {
            SelectMode::Data => &self.computation_task.input_table_id,
            SelectMode::ColumnMeta => &self.computation_task.column_table_id,
            SelectMode::RowMeta => &self.computation_task.row_table_id,
        };

        // Use TableSchemaService.streamTable()
        let stream = self.client.stream_table(table_id, columns).await?;

        // Collect and parse into DataFrame
        let data = collect_stream(stream).await?;
        DataFrame::from_csv(&data)
    }
}

enum SelectMode {
    Data,        // Main data with .y, .ci, .ri
    ColumnMeta,  // Column annotations
    RowMeta,     // Row annotations
}
```

### DataFrame API

```rust
// src/tercen/data.rs

use std::collections::HashMap;

/// Simple DataFrame for Tercen data
/// Can be replaced with Polars later if needed
pub struct DataFrame {
    columns: HashMap<String, Column>,
    nrows: usize,
}

pub enum Column {
    Float(Vec<f64>),
    Int(Vec<i64>),
    String(Vec<String>),
    Bool(Vec<bool>),
}

impl DataFrame {
    /// Create from CSV data (from Tercen stream)
    pub fn from_csv(csv_data: &str) -> Result<Self, TercenError> {
        // Parse CSV into columns
        todo!()
    }

    /// Create from Arrow binary (from Tercen stream)
    pub fn from_arrow(arrow_data: &[u8]) -> Result<Self, TercenError> {
        // Parse Arrow IPC format
        todo!()
    }

    /// Get column by name
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.get(name)
    }

    /// Get column names
    pub fn column_names(&self) -> Vec<&str> {
        self.columns.keys().map(|s| s.as_str()).collect()
    }

    /// Number of rows
    pub fn nrows(&self) -> usize {
        self.nrows
    }

    /// Number of columns
    pub fn ncols(&self) -> usize {
        self.columns.len()
    }

    /// Get data for GGRS (convert to GGRS DataFrame format)
    pub fn to_ggrs_dataframe(&self) -> Result<ggrs_core::DataFrame, TercenError> {
        // Convert to GGRS format
        todo!()
    }

    /// Select specific rows
    pub fn filter_rows(&self, predicate: impl Fn(usize) -> bool) -> Self {
        // Filter rows based on predicate
        todo!()
    }

    /// Select specific columns
    pub fn select_columns(&self, columns: &[&str]) -> Self {
        // Select subset of columns
        todo!()
    }
}

impl Column {
    /// Get value at index
    pub fn get(&self, index: usize) -> Option<Value> {
        match self {
            Column::Float(v) => v.get(index).map(|&x| Value::Float(x)),
            Column::Int(v) => v.get(index).map(|&x| Value::Int(x)),
            Column::String(v) => v.get(index).map(|x| Value::String(x.clone())),
            Column::Bool(v) => v.get(index).map(|&x| Value::Bool(x)),
        }
    }

    /// Get as float slice (if column is float)
    pub fn as_float(&self) -> Option<&[f64]> {
        match self {
            Column::Float(v) => Some(v),
            _ => None,
        }
    }

    /// Length of column
    pub fn len(&self) -> usize {
        match self {
            Column::Float(v) => v.len(),
            Column::Int(v) => v.len(),
            Column::String(v) => v.len(),
            Column::Bool(v) => v.len(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    Float(f64),
    Int(i64),
    String(String),
    Bool(bool),
    Null,
}
```

### Error Handling

```rust
// src/tercen/error.rs

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TercenError {
    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),

    #[error("Environment variable not found: {0}")]
    EnvVar(#[from] std::env::VarError),

    #[error("CSV parse error: {0}")]
    CsvParse(String),

    #[error("Arrow parse error: {0}")]
    ArrowParse(String),

    #[error("Column not found: {0}")]
    ColumnNotFound(String),

    #[error("Type conversion error: {0}")]
    TypeConversion(String),

    #[error("Invalid operator property: {0}")]
    InvalidProperty(String),
}
```

## Usage Example

### In GGRS Plot Operator

```rust
// src/main.rs

use tercen::TercenContext;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create context from environment
    let ctx = TercenContext::from_env().await?;

    ctx.log("Starting GGRS plot generation").await?;

    // Get operator properties
    let width = ctx.operator_property("width", 800)?;
    let height = ctx.operator_property("height", 600)?;
    let theme = ctx.operator_property("theme", "gray".to_string())?;

    // Query data
    ctx.progress(0.1, "Loading data...").await?;
    let data = ctx.select(&[".x", ".y", ".ci", ".ri"]).await?;

    // Get column/row metadata for faceting
    let col_meta = ctx.cselect(&["factor1", "factor2"]).await?;
    let row_meta = ctx.rselect(&["sample", "group"]).await?;

    ctx.progress(0.3, "Preparing plot data...").await?;

    // Convert to GGRS format
    let ggrs_df = data.to_ggrs_dataframe()?;

    // Generate plot with GGRS
    ctx.progress(0.5, "Generating plot...").await?;
    let png_bytes = generate_plot(&ggrs_df, width, height, &theme)?;

    // Upload result
    ctx.progress(0.9, "Uploading result...").await?;
    upload_result(&ctx, png_bytes).await?;

    ctx.progress(1.0, "Done").await?;

    Ok(())
}
```

### Streaming for Large Datasets

```rust
use futures::StreamExt;

// For very large datasets, stream in chunks
let mut stream = ctx.select_stream(&[".x", ".y", ".ci", ".ri"]);

while let Some(chunk) = stream.next().await {
    let chunk_df = chunk?;
    // Process chunk incrementally
    process_chunk(chunk_df)?;
}
```

## Integration with GGRS StreamGenerator

The TercenContext can be used to implement the GGRS `StreamGenerator` trait:

```rust
pub struct TercenStreamGenerator {
    context: Arc<TercenContext>,
    aes: Aes,
    facet_spec: FacetSpec,
}

impl StreamGenerator for TercenStreamGenerator {
    fn query_cell_data(&mut self, cell_spec: &CellSpec) -> Result<DataFrame> {
        // Use context to query specific facet cell data
        let filter = build_filter_from_cell_spec(cell_spec);

        // Query with filter
        let data = self.context
            .select(&[".x", ".y"])
            .await?
            .filter_rows(filter);

        // Convert to GGRS DataFrame
        data.to_ggrs_dataframe()
    }

    fn get_facet_spec(&self) -> &FacetSpec {
        &self.facet_spec
    }

    fn get_aes(&self) -> &Aes {
        &self.aes
    }
}
```

## Directory Structure

```
src/
├── main.rs                      # Application entry point
├── tercen/                      # Tercen context (extractable to separate crate)
│   ├── mod.rs
│   ├── context.rs               # TercenContext implementation
│   ├── data.rs                  # DataFrame and Column types
│   ├── error.rs                 # TercenError type
│   └── stream.rs                # Streaming utilities
├── grpc_client.rs               # Low-level gRPC client
├── ggrs_integration/            # GGRS-specific code
│   ├── stream_generator.rs      # TercenStreamGenerator
│   └── ...
└── ...
```

## Migration Path

When ready to extract to separate crate:

1. **Create new crate**: `tercen-rust` or `rtercen`
2. **Move directory**: `src/tercen/` → `tercen-rust/src/`
3. **Add dependency**: `tercen-rust = { path = "../tercen-rust" }`
4. **Update imports**: `use crate::tercen::` → `use tercen_rust::`

## C# Implementation Patterns (from TercenCSharpClient)

Based on [TercenCSharpClient](https://github.com/tercen/TercenCSharpClient):

### 1. Streaming Pattern

**C# approach**:
```csharp
// Stream table data using Apache Arrow format
var call = tableSchemaService.streamTable(reqStreamTable);

// Async enumeration over chunks
await foreach (var dataFrame in tableSchemaService.Stream(reqStreamTable)) {
    // Process DataFrame chunk
}

// Or collect all chunks
var list = new List<DataFrame>();
await foreach (var dataFrame in tableSchemaService.Stream(reqStreamTable)) {
    list.Add(dataFrame);
}
var fullDataFrame = ConcatenateVertically(list);
```

**Key observations**:
- Uses `streamTable()` gRPC method from TableSchemaService
- Returns Apache Arrow binary format
- Supports async enumeration for chunk-by-chunk processing
- Can collect and concatenate chunks into single DataFrame

### 2. Data Format

- **Arrow Binary**: Data streamed as Apache Arrow IPC format (efficient columnar)
- **Deserialization**: `DataFrameFromBytes()` converts Arrow bytes to typed DataFrames
- **Type extraction**: Uses Arrow arrays (Int32Array, StringArray, DoubleArray, etc.)

### 3. Factory Pattern

```csharp
// Create authenticated factory
var factory = await TercenFactory.Create(uri, tenant, username, password);

// Get service clients
var tableSchemaService = factory.TableSchemaService();
var taskService = factory.TaskService();
var fileService = factory.FileService();
```

**Authentication**:
- Bearer token added via `AuthenticatedHttpClientHandler`
- Token injected into Authorization header
- Persistent authenticated channel

### 4. Request Structure

```csharp
var reqStreamTable = new ReqStreamTable {
    TableId = tableId,
    Cnames = { /* column names */ },
    Offset = 0,
    Limit = 10000,
    BinaryFormat = "arrow"
};
```

### 5. Service Clients Available

- `UserService()` - Authentication
- `TableSchemaService()` - Data streaming
- `TaskService()` - Task lifecycle
- `FileService()` - File upload/download
- `WorkflowService()` - Workflow execution
- `EventService()` - Event management
- `DocumentService()`, `ProjectService()`, etc.

### 6. Error Handling

```csharp
// Custom exception wrapping
try {
    await service.operation();
} catch (RpcException ex) {
    var tercenEx = new TercenException(ex);
    if (tercenEx.IsNotFound()) {
        // Handle 404
    }
}
```

## Next Steps

1. Review C# implementation (please provide link)
2. Implement minimal `TercenContext` with `select()` method
3. Add DataFrame CSV/Arrow parsing
4. Implement streaming version
5. Integrate with GGRS StreamGenerator
6. Add operator property parsing
7. Extract to separate crate when stable
