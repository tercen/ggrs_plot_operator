# Rust Context Implementation (Based on C# Client)

## Overview

This document provides the Rust implementation of TercenContext based on patterns from [TercenCSharpClient](https://github.com/tercen/TercenCSharpClient).

## Key Patterns from C#

1. **Factory pattern** for authenticated client creation
2. **Apache Arrow format** for efficient data streaming
3. **Async enumeration** over data chunks
4. **Service-based architecture** (TableSchemaService, TaskService, FileService)
5. **Bearer token authentication** via interceptor

## Rust Implementation

### Module Structure

```
src/tercen/
├── mod.rs              # Public exports
├── factory.rs          # TercenFactory (like C# factory pattern)
├── context.rs          # TercenContext (high-level API)
├── client.rs           # Low-level gRPC service clients
├── data/
│   ├── mod.rs
│   ├── dataframe.rs    # DataFrame type (Arrow-based)
│   ├── arrow.rs        # Arrow deserialization
│   └── stream.rs       # Async streaming utilities
├── auth.rs             # Authentication interceptor
└── error.rs            # Error types
```

### 1. Factory Pattern

```rust
// src/tercen/factory.rs

use tonic::transport::{Channel, ClientTlsConfig};
use tonic::service::Interceptor;
use std::sync::Arc;

pub struct TercenFactory {
    channel: Channel,
    token: Arc<RwLock<String>>,
}

impl TercenFactory {
    /// Create authenticated factory (like C# TercenFactory.Create)
    pub async fn create(
        uri: &str,
        tenant: &str,
        username: &str,
        password: &str,
    ) -> Result<Self, TercenError> {
        // Create channel
        let channel = Self::create_channel(uri).await?;

        // Authenticate via UserService
        let mut user_service = UserServiceClient::new(channel.clone());
        let auth_request = ReqGenerateToken {
            domain: tenant.to_string(),
            username_or_email: username.to_string(),
            password: password.to_string(),
            ..Default::default()
        };

        let response = user_service.connect2(auth_request).await?;
        let session = response.into_inner();
        let token = session.token.unwrap_or_default();

        Ok(Self {
            channel,
            token: Arc::new(RwLock::new(token)),
        })
    }

    /// Create TLS channel
    async fn create_channel(uri: &str) -> Result<Channel, TercenError> {
        let tls = ClientTlsConfig::new();

        Channel::from_shared(uri.to_string())?
            .tls_config(tls)?
            .connect()
            .await
            .map_err(Into::into)
    }

    /// Get TableSchemaService client
    pub fn table_schema_service(&self) -> TableSchemaServiceClient<AuthenticatedChannel> {
        let interceptor = AuthInterceptor {
            token: self.token.clone(),
        };
        TableSchemaServiceClient::with_interceptor(self.channel.clone(), interceptor)
    }

    /// Get TaskService client
    pub fn task_service(&self) -> TaskServiceClient<AuthenticatedChannel> {
        let interceptor = AuthInterceptor {
            token: self.token.clone(),
        };
        TaskServiceClient::with_interceptor(self.channel.clone(), interceptor)
    }

    /// Get FileService client
    pub fn file_service(&self) -> FileServiceClient<AuthenticatedChannel> {
        let interceptor = AuthInterceptor {
            token: self.token.clone(),
        };
        FileServiceClient::with_interceptor(self.channel.clone(), interceptor)
    }
}

type AuthenticatedChannel = InterceptedService<Channel, AuthInterceptor>;
```

### 2. Authentication Interceptor

```rust
// src/tercen/auth.rs

use tonic::{Request, Status, service::Interceptor};
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct AuthInterceptor {
    pub token: Arc<RwLock<String>>,
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let token = self.token.read().unwrap();
        let auth_header = format!("Bearer {}", token)
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid token format"))?;

        request.metadata_mut().insert("authorization", auth_header);
        Ok(request)
    }
}
```

### 3. DataFrame with Arrow Support

```rust
// src/tercen/data/dataframe.rs

use arrow::array::*;
use arrow::ipc::reader::StreamReader;
use arrow::record_batch::RecordBatch;
use std::collections::HashMap;
use std::io::Cursor;

pub struct DataFrame {
    columns: HashMap<String, Column>,
    nrows: usize,
}

pub enum Column {
    Float64(Vec<f64>),
    Int32(Vec<i32>),
    Int64(Vec<i64>),
    String(Vec<String>),
    Bool(Vec<bool>),
}

impl DataFrame {
    /// Create DataFrame from Apache Arrow bytes (like C# DataFrameFromBytes)
    pub fn from_arrow_bytes(bytes: &[u8]) -> Result<Self, TercenError> {
        let cursor = Cursor::new(bytes);
        let reader = StreamReader::try_new(cursor, None)?;

        let mut batches = Vec::new();
        for batch_result in reader {
            batches.push(batch_result?);
        }

        Self::from_record_batches(&batches)
    }

    /// Convert Arrow RecordBatches to DataFrame
    fn from_record_batches(batches: &[RecordBatch]) -> Result<Self, TercenError> {
        if batches.is_empty() {
            return Ok(Self {
                columns: HashMap::new(),
                nrows: 0,
            });
        }

        let schema = batches[0].schema();
        let mut columns = HashMap::new();
        let nrows = batches.iter().map(|b| b.num_rows()).sum();

        // Convert each field
        for field in schema.fields() {
            let name = field.name().clone();
            let mut column_data = Vec::new();

            for batch in batches {
                let array = batch.column_by_name(&name).unwrap();
                Self::append_array_to_column(&mut column_data, array)?;
            }

            columns.insert(name, column_data);
        }

        Ok(Self { columns, nrows })
    }

    /// Append Arrow array to column vector
    fn append_array_to_column(
        column_data: &mut Vec<Column>,
        array: &dyn arrow::array::Array,
    ) -> Result<(), TercenError> {
        use arrow::datatypes::DataType;

        match array.data_type() {
            DataType::Float64 => {
                let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
                let values: Vec<f64> = (0..arr.len())
                    .map(|i| arr.value(i))
                    .collect();
                column_data.push(Column::Float64(values));
            }
            DataType::Int32 => {
                let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
                let values: Vec<i32> = (0..arr.len())
                    .map(|i| arr.value(i))
                    .collect();
                column_data.push(Column::Int32(values));
            }
            DataType::Int64 => {
                let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
                let values: Vec<i64> = (0..arr.len())
                    .map(|i| arr.value(i))
                    .collect();
                column_data.push(Column::Int64(values));
            }
            DataType::Utf8 => {
                let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
                let values: Vec<String> = (0..arr.len())
                    .map(|i| arr.value(i).to_string())
                    .collect();
                column_data.push(Column::String(values));
            }
            DataType::Boolean => {
                let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
                let values: Vec<bool> = (0..arr.len())
                    .map(|i| arr.value(i))
                    .collect();
                column_data.push(Column::Bool(values));
            }
            _ => {
                return Err(TercenError::UnsupportedDataType(
                    format!("{:?}", array.data_type())
                ));
            }
        }

        Ok(())
    }

    /// Concatenate DataFrames vertically (like C# ConcatenateVertically)
    pub fn concatenate_vertical(frames: Vec<DataFrame>) -> Result<Self, TercenError> {
        if frames.is_empty() {
            return Ok(DataFrame {
                columns: HashMap::new(),
                nrows: 0,
            });
        }

        // Verify schema compatibility
        let first_cols: Vec<_> = frames[0].columns.keys().collect();
        for frame in &frames[1..] {
            let cols: Vec<_> = frame.columns.keys().collect();
            if first_cols != cols {
                return Err(TercenError::SchemaMismatch(
                    "Cannot concatenate DataFrames with different schemas".into()
                ));
            }
        }

        // Concatenate columns
        let mut result_columns = HashMap::new();
        let total_rows = frames.iter().map(|f| f.nrows).sum();

        for col_name in first_cols {
            let mut concatenated = Vec::new();
            for frame in &frames {
                concatenated.extend(frame.columns.get(col_name).unwrap().clone());
            }
            result_columns.insert(col_name.clone(), concatenated);
        }

        Ok(DataFrame {
            columns: result_columns,
            nrows: total_rows,
        })
    }

    /// Get column by name
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.get(name)
    }

    /// Number of rows
    pub fn nrows(&self) -> usize {
        self.nrows
    }
}
```

### 4. Streaming Table Data

```rust
// src/tercen/data/stream.rs

use futures::Stream;
use tonic::Streaming;

/// Stream table data with async enumeration (like C# Stream extension)
pub async fn stream_table(
    mut client: TableSchemaServiceClient<AuthenticatedChannel>,
    table_id: String,
    columns: Vec<String>,
) -> impl Stream<Item = Result<DataFrame, TercenError>> {
    async_stream::stream! {
        let request = ReqStreamTable {
            table_id,
            cnames: columns,
            offset: 0,
            limit: 0, // 0 = all data
            binary_format: "arrow".to_string(),
        };

        let response = client.stream_table(request).await?;
        let mut stream = response.into_inner();

        // Stream chunks as Arrow bytes
        while let Some(chunk) = stream.message().await? {
            // Deserialize Arrow bytes to DataFrame
            let df = DataFrame::from_arrow_bytes(&chunk.result)?;
            yield Ok(df);
        }
    }
}

/// Select all data (collects all chunks like C# Select extension)
pub async fn select_table(
    client: TableSchemaServiceClient<AuthenticatedChannel>,
    table_id: String,
    columns: Vec<String>,
) -> Result<DataFrame, TercenError> {
    let mut frames = Vec::new();

    let mut stream = stream_table(client, table_id, columns).await;

    while let Some(result) = stream.next().await {
        frames.push(result?);
    }

    DataFrame::concatenate_vertical(frames)
}
```

### 5. TercenContext API

```rust
// src/tercen/context.rs

pub struct TercenContext {
    factory: Arc<TercenFactory>,
    task_id: String,
    computation_task: ComputationTask,
}

impl TercenContext {
    /// Create context from environment
    pub async fn from_env() -> Result<Self, TercenError> {
        let uri = std::env::var("TERCEN_URI")?;
        let tenant = std::env::var("TERCEN_TENANT").unwrap_or_default();
        let username = std::env::var("TERCEN_USERNAME")?;
        let password = std::env::var("TERCEN_PASSWORD")?;
        let task_id = std::env::var("TERCEN_TASK_ID")?;

        let factory = TercenFactory::create(&uri, &tenant, &username, &password).await?;
        Self::new(Arc::new(factory), task_id).await
    }

    /// Create context from factory and task ID
    pub async fn new(factory: Arc<TercenFactory>, task_id: String) -> Result<Self, TercenError> {
        let mut task_service = factory.task_service();

        // Get computation task
        let task_request = GetRequest {
            id: task_id.clone(),
            use_factory: false,
        };

        let task = task_service.get(task_request).await?.into_inner();
        let computation_task = extract_computation_task(task)?;

        Ok(Self {
            factory,
            task_id,
            computation_task,
        })
    }

    /// Select data (like Python ctx.select([".y", ".ci", ".ri"]))
    pub async fn select(&self, columns: &[&str]) -> Result<DataFrame, TercenError> {
        let table_id = &self.computation_task.input_table_id;
        let client = self.factory.table_schema_service();
        let cols = columns.iter().map(|s| s.to_string()).collect();

        select_table(client, table_id.clone(), cols).await
    }

    /// Select column metadata (like Python ctx.cselect([""]))
    pub async fn cselect(&self, columns: &[&str]) -> Result<DataFrame, TercenError> {
        let table_id = &self.computation_task.column_schema_id;
        let client = self.factory.table_schema_service();
        let cols = columns.iter().map(|s| s.to_string()).collect();

        select_table(client, table_id.clone(), cols).await
    }

    /// Select row metadata (like Python ctx.rselect([""]))
    pub async fn rselect(&self, columns: &[&str]) -> Result<DataFrame, TercenError> {
        let table_id = &self.computation_task.row_schema_id;
        let client = self.factory.table_schema_service();
        let cols = columns.iter().map(|s| s.to_string()).collect();

        select_table(client, table_id.clone(), cols).await
    }

    /// Stream data for large datasets
    pub fn select_stream(&self, columns: &[&str]) -> impl Stream<Item = Result<DataFrame, TercenError>> {
        let table_id = self.computation_task.input_table_id.clone();
        let client = self.factory.table_schema_service();
        let cols = columns.iter().map(|s| s.to_string()).collect();

        stream_table(client, table_id, cols)
    }

    /// Get operator property
    pub fn operator_property<T>(&self, name: &str, default: T) -> T
    where
        T: FromStr,
    {
        self.computation_task
            .operator_settings
            .as_ref()
            .and_then(|settings| {
                settings.properties.iter()
                    .find(|p| p.name == name)
                    .and_then(|p| p.value.parse().ok())
            })
            .unwrap_or(default)
    }

    /// Log message
    pub async fn log(&self, message: &str) -> Result<(), TercenError> {
        let mut event_service = self.factory.event_service();

        let log_event = EEvent {
            object: Some(e_event::Object::TaskLogEvent(TaskLogEvent {
                task_id: self.task_id.clone(),
                level: "INFO".to_string(),
                message: message.to_string(),
                ..Default::default()
            })),
        };

        event_service.create(log_event).await?;
        Ok(())
    }

    /// Update progress (0.0 to 1.0)
    pub async fn progress(&self, progress: f64, message: &str) -> Result<(), TercenError> {
        let mut task_service = self.factory.task_service();

        let mut task = self.computation_task.clone();
        // Update task with progress event
        // Implementation depends on TaskService.update pattern

        Ok(())
    }
}
```

## Usage Example

```rust
use tercen::TercenContext;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create context from environment (reads TERCEN_* env vars)
    let ctx = TercenContext::from_env().await?;

    ctx.log("Starting plot generation").await?;

    // Query main data with indices
    let data = ctx.select(&[".x", ".y", ".ci", ".ri"]).await?;
    println!("Loaded {} rows", data.nrows());

    // Query column metadata
    let col_meta = ctx.cselect(&["sample", "condition"]).await?;

    // Query row metadata
    let row_meta = ctx.rselect(&["gene", "protein"]).await?;

    // Get operator properties
    let width: i32 = ctx.operator_property("width", 800);
    let height: i32 = ctx.operator_property("height", 600);
    let theme: String = ctx.operator_property("theme", "gray".to_string());

    // For large datasets, stream
    use futures::StreamExt;
    let mut stream = ctx.select_stream(&[".x", ".y"]);
    while let Some(chunk) = stream.next().await {
        let df = chunk?;
        println!("Processing chunk of {} rows", df.nrows());
        // Process incrementally
    }

    Ok(())
}
```

## Cargo Dependencies

```toml
[dependencies]
# gRPC
tonic = { version = "0.11", features = ["tls"] }
prost = "0.12"

# Async
tokio = { version = "1.35", features = ["full"] }
futures = "0.3"
async-stream = "0.3"

# Arrow
arrow = { version = "50.0", features = ["ipc"] }

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Utilities
tokio-stream = "0.1"
```

## Key Differences from C#

1. **No LINQ**: Use iterators and functional methods instead
2. **Ownership**: Need careful Arc/Clone management for concurrent access
3. **Async**: Rust async is more explicit (`.await`, `async_stream`)
4. **Arrow**: Use Rust arrow crate directly (similar to C# Arrow.NET)
5. **Type safety**: Rust enum for Column types vs C# generic arrays

## Benefits of Rust Implementation

- ✅ **Zero-copy Arrow deserialization** where possible
- ✅ **Memory safety** without GC overhead
- ✅ **Concurrent streaming** with tokio
- ✅ **Type-safe gRPC** clients
- ✅ **Portable** - can extract to separate crate

## Next Steps

1. Implement TercenFactory with authentication
2. Add Arrow DataFrame parsing
3. Implement streaming with async-stream
4. Add operator property parsing
5. Integrate with GGRS StreamGenerator
6. Test with real Tercen instance
7. Extract to tercen-rust crate
