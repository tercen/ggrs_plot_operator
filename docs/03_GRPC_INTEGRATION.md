# gRPC Integration Specification

## Overview

This document provides detailed specifications for integrating with Tercen's gRPC API. It covers authentication, service interaction patterns, data formats, and error handling.

---

## Proto Files and Generation

### Source Files

Located in: `/home/thiago/workspaces/tercen/main/sci/tercen_grpc/tercen_grpc_api/protos/`

- **tercen.proto**: Service definitions and RPC methods
- **tercen_model.proto**: Data model definitions (messages)

### Code Generation

Add to `Cargo.toml`:

```toml
[dependencies]
tonic = "0.11"
prost = "0.12"
tokio = { version = "1.35", features = ["full"] }

[build-dependencies]
tonic-build = "0.11"
```

Create `build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(false)  // We only need the client
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile(
            &[
                "protos/tercen.proto",
                "protos/tercen_model.proto",
            ],
            &["protos"],
        )?;
    Ok(())
}
```

Generated types will be available as:
```rust
pub mod tercen {
    tonic::include_proto!("tercen");
}
```

---

## Service Descriptions

### 1. UserService

**Purpose**: Authentication and user management

#### Key Methods

##### `generateToken`

Generate authentication token for subsequent requests.

```protobuf
rpc generateToken(ReqGenerateToken) returns (RespGenerateToken);
```

**Request**:
```rust
ReqGenerateToken {
    username: String,
    password: String,
    // Or use existing refresh token:
    refresh_token: Option<String>,
}
```

**Response**:
```rust
RespGenerateToken {
    access_token: String,
    refresh_token: String,
    expires_in: i64,  // seconds
}
```

**Usage Pattern**:
```rust
let token_response = user_client
    .generate_token(ReqGenerateToken {
        username: "operator_user".to_string(),
        password: env::var("TERCEN_PASSWORD")?,
        refresh_token: None,
    })
    .await?;

let access_token = token_response.into_inner().access_token;
```

**Token Refresh**:
- Access tokens expire (typically 1 hour)
- Use refresh token to get new access token
- Implement auto-refresh on 401 errors

---

### 2. TaskService

**Purpose**: Manage computation task lifecycle

#### Key Methods

##### `create`

Create a new task.

```protobuf
rpc create(ETask) returns (ETask);
```

**Request**: `ETask` with appropriate task type (e.g., `ComputationTask`)

##### `get`

Retrieve task details.

```protobuf
rpc get(GetRequest) returns (ETask);
```

**Request**:
```rust
GetRequest {
    id: String,           // Task ID
    use_factory: bool,    // false for direct retrieval
}
```

##### `runTask`

Execute a task.

```protobuf
rpc runTask(ReqRunTask) returns (RespRunTask);
```

**Request**:
```rust
ReqRunTask {
    task_id: String,
}
```

**Response**:
```rust
RespRunTask {
    result: Vec<u8>,  // Serialized task result
}
```

##### `waitDone`

Block until task completes (used by operators to wait for work).

```protobuf
rpc waitDone(ReqWaitDone) returns (RespWaitDone);
```

**Request**:
```rust
ReqWaitDone {
    task_id: String,
}
```

**Response**:
```rust
RespWaitDone {
    result: Option<ETask>,  // Completed task
}
```

##### `update`

Update task state and metadata.

```protobuf
rpc update(ETask) returns (ResponseUpdate);
```

**Common Task States**:
```rust
enum State {
    InitState,              // Initial state
    PendingState,           // Waiting to run
    RunningState,           // Currently executing
    RunningDependentState,  // Running dependency
    DoneState,              // Successfully completed
    FailedState,            // Failed with error
    CanceledState,          // Canceled by user
}
```

#### Task Lifecycle Pattern

```rust
// 1. Operator polls for tasks
let wait_request = ReqWaitDone {
    task_id: operator_task_id.clone(),
};

let task_response = task_client
    .wait_done(wait_request)
    .await?;

let computation_task = task_response.into_inner().result;

// 2. Update to Running state
let mut task = computation_task.clone();
task.state = Some(EState {
    object: Some(e_state::Object::RunningState(RunningState {
        start_date: SystemTime::now(),
        ..Default::default()
    })),
});

task_client.update(task.clone()).await?;

// 3. Execute computation (details below)
// ...

// 4. Update to Done state
task.state = Some(EState {
    object: Some(e_state::Object::DoneState(DoneState {
        end_date: SystemTime::now(),
        ..Default::default()
    })),
});

task_client.update(task).await?;
```

#### Progress and Log Events

Send progress updates:
```rust
let progress_event = EEvent {
    object: Some(e_event::Object::TaskProgressEvent(TaskProgressEvent {
        task_id: task.id.clone(),
        progress: 0.5,  // 50% complete
        message: "Generating plot...".to_string(),
        ..Default::default()
    })),
};

// Publish via event service or update task
```

Send log messages:
```rust
let log_event = EEvent {
    object: Some(e_event::Object::TaskLogEvent(TaskLogEvent {
        task_id: task.id.clone(),
        level: "INFO".to_string(),
        message: "Data loaded successfully".to_string(),
        ..Default::default()
    })),
};
```

---

### 3. TableSchemaService

**Purpose**: Access and stream table data

#### Key Methods

##### `get`

Retrieve table schema metadata.

```protobuf
rpc get(GetRequest) returns (ESchema);
```

**Response**: `ESchema` with table metadata (column names, types, row count, etc.)

##### `streamTable`

Stream table data in chunks.

```protobuf
rpc streamTable(ReqStreamTable) returns (stream RespStreamTable);
```

**Request**:
```rust
ReqStreamTable {
    table_id: String,              // Table ID from computation task
    cnames: Vec<String>,           // Column names to fetch (empty = all)
    offset: i64,                   // Row offset (0-based)
    limit: i64,                    // Number of rows (0 = all remaining)
    binary_format: String,         // "csv" or "arrow"
}
```

**Response** (streamed):
```rust
RespStreamTable {
    result: Vec<u8>,  // Chunk of data in specified format
}
```

#### Data Streaming Pattern

```rust
// Request data stream
let stream_request = ReqStreamTable {
    table_id: computation_task.input_table_id.clone(),
    cnames: vec![],  // Empty = all columns
    offset: 0,
    limit: 0,        // 0 = all data
    binary_format: "csv".to_string(),
};

let mut stream = table_client
    .stream_table(stream_request)
    .await?
    .into_inner();

// Collect chunks
let mut data_chunks = Vec::new();
while let Some(chunk) = stream.message().await? {
    data_chunks.push(chunk.result);
}

// Concatenate chunks
let full_data = data_chunks.concat();

// Parse CSV
let csv_string = String::from_utf8(full_data)?;
let df = parse_csv_to_dataframe(&csv_string)?;
```

#### CSV Format

Example CSV from Tercen:
```csv
.ri,.ci,.x,.y,species,color_factor
0,0,5.1,3.5,setosa,group_a
0,1,4.9,3.0,setosa,group_a
1,0,7.0,3.2,versicolor,group_b
...
```

Special columns:
- `.ri`: Row index (facet row)
- `.ci`: Column index (facet column)
- `.x`: X axis value
- `.y`: Y axis value
- Other columns: Factors and variables

#### Binary Format (Arrow)

For large datasets, use Arrow format:
```rust
ReqStreamTable {
    binary_format: "arrow".to_string(),
    // ...
}
```

Parse Arrow bytes:
```rust
use arrow::ipc::reader::StreamReader;

let cursor = std::io::Cursor::new(full_data);
let reader = StreamReader::try_new(cursor)?;

for batch in reader {
    let record_batch = batch?;
    // Process record batch
}
```

---

### 4. FileService

**Purpose**: Upload and download files

#### Key Methods

##### `create`

Create file document metadata.

```protobuf
rpc create(EFileDocument) returns (EFileDocument);
```

##### `upload`

Upload file data (streaming).

```protobuf
rpc upload(stream ReqUpload) returns (RespUpload);
```

**Request** (streamed):
```rust
ReqUpload {
    file: Option<EFileDocument>,  // First message only
    bytes: Vec<u8>,               // Subsequent messages
}
```

**Response**:
```rust
RespUpload {
    result: Option<EFileDocument>,  // Created file with ID
}
```

#### File Upload Pattern

```rust
// 1. Create file document
let file_doc = EFileDocument {
    object: Some(e_file_document::Object::FileDocument(FileDocument {
        id: "".to_string(),  // Will be assigned by server
        name: format!("{}.png", output_name),
        project_id: task.project_id.clone(),
        folder_id: task.folder_id.clone(),
        acl: task.acl.clone(),
        mimetype: "image/png".to_string(),
        size: png_bytes.len() as i64,
        ..Default::default()
    })),
};

// 2. Create upload stream
let (tx, rx) = mpsc::channel(4);

// 3. Send first message with metadata
tx.send(ReqUpload {
    file: Some(file_doc),
    bytes: vec![],
}).await?;

// 4. Send data in chunks
const CHUNK_SIZE: usize = 64 * 1024;  // 64 KB chunks
for chunk in png_bytes.chunks(CHUNK_SIZE) {
    tx.send(ReqUpload {
        file: None,
        bytes: chunk.to_vec(),
    }).await?;
}

drop(tx);  // Close stream

// 5. Perform upload
let response = file_client
    .upload(ReceiverStream::new(rx))
    .await?;

let uploaded_file = response.into_inner().result;
```

##### `download`

Download file data (streaming).

```protobuf
rpc download(ReqDownload) returns (stream RespDownload);
```

**Request**:
```rust
ReqDownload {
    file_document_id: String,
}
```

**Response** (streamed):
```rust
RespDownload {
    result: Vec<u8>,  // Chunk of file data
}
```

---

## Authentication & Metadata

### Adding Auth Token to Requests

Use interceptors to add auth token to all requests:

```rust
use tonic::{Request, Status, service::Interceptor};

#[derive(Clone)]
pub struct AuthInterceptor {
    pub token: Arc<RwLock<String>>,
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let token = self.token.read().unwrap();
        let metadata_value = format!("Bearer {}", token)
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid token format"))?;

        request.metadata_mut().insert(
            "authorization",
            metadata_value,
        );

        Ok(request)
    }
}

// Usage
let auth = AuthInterceptor {
    token: Arc::new(RwLock::new(access_token)),
};

let task_client = TaskServiceClient::with_interceptor(
    channel.clone(),
    auth.clone(),
);
```

### Connection Setup

```rust
use tonic::transport::{Channel, ClientTlsConfig};

async fn create_channel(endpoint: &str) -> Result<Channel, Box<dyn std::error::Error>> {
    let tls = ClientTlsConfig::new()
        .domain_name("tercen.com");  // Or your domain

    let channel = Channel::from_shared(endpoint.to_string())?
        .tls_config(tls)?
        .connect()
        .await?;

    Ok(channel)
}

// Usage
let channel = create_channel("https://tercen.com:5400").await?;
```

---

## Data Model Details

### ComputationTask

This is the main task type for operator execution.

```rust
ComputationTask {
    // From Task base
    id: String,
    project_id: String,
    acl: Option<Acl>,
    state: Option<EState>,
    owner: String,

    // Computation-specific
    operator_id: String,              // Operator to execute
    operator_settings: Option<OperatorSettings>,
    input_table_id: String,           // Input data table
    output_table_id: Option<String>,  // Output table (if any)
    crosstab_spec: Option<CrosstabSpec>,

    // Properties from operator.json
    properties: Vec<Property>,

    // Context
    workflow_id: Option<String>,
    step_id: Option<String>,
}
```

### OperatorSettings

Operator configuration from `operator.json`:

```rust
OperatorSettings {
    properties: Vec<Property>,
}

// Property types:
Property {
    name: String,
    value: PropertyValue,
}

enum PropertyValue {
    String(String),
    Double(f64),
    Boolean(bool),
    Enumerated(String),  // One of predefined values
}
```

### CrosstabSpec

Defines the data projection (axes, facets, aesthetics):

```rust
CrosstabSpec {
    row_factors: Vec<Factor>,     // Facet rows
    col_factors: Vec<Factor>,     // Facet columns
    x_factors: Vec<Factor>,       // X axis
    y_factors: Vec<Factor>,       // Y axis
    color_factors: Vec<Factor>,   // Color aesthetic
    label_factors: Vec<Factor>,   // Labels
}

Factor {
    name: String,
    type: String,  // "numeric", "string", "date", etc.
}
```

### TableSchema

Metadata about data tables:

```rust
TableSchema {
    id: String,
    name: String,
    n_rows: i64,
    n_cols: i32,
    columns: Vec<ColumnSchema>,
}

ColumnSchema {
    name: String,
    type: String,  // "double", "int32", "string"
}
```

---

## Error Handling

### gRPC Status Codes

Common status codes from Tercen:

| Code | Description | Typical Cause | Recovery Action |
|------|-------------|---------------|-----------------|
| `OK` | Success | - | Continue |
| `CANCELLED` | Request cancelled | User cancellation | Stop processing |
| `UNKNOWN` | Unknown error | Server error | Log and report |
| `INVALID_ARGUMENT` | Bad request | Invalid parameters | Validate inputs |
| `DEADLINE_EXCEEDED` | Timeout | Slow operation | Retry with backoff |
| `NOT_FOUND` | Resource not found | Invalid ID | Report to user |
| `ALREADY_EXISTS` | Duplicate | Concurrent creation | Use existing |
| `PERMISSION_DENIED` | Auth failed | Invalid token | Re-authenticate |
| `RESOURCE_EXHAUSTED` | Rate limit | Too many requests | Backoff and retry |
| `FAILED_PRECONDITION` | Invalid state | Wrong task state | Check state |
| `ABORTED` | Concurrent conflict | Race condition | Retry |
| `OUT_OF_RANGE` | Invalid range | Bad offset/limit | Adjust range |
| `UNIMPLEMENTED` | Not supported | Using wrong API | Check documentation |
| `INTERNAL` | Server error | Tercen bug | Report to Tercen |
| `UNAVAILABLE` | Service down | Network/server issue | Retry with backoff |
| `UNAUTHENTICATED` | No auth | Missing token | Authenticate |

### Error Handling Pattern

```rust
use tonic::Status;

async fn execute_task(task_id: &str) -> Result<(), OperatorError> {
    match task_client.run_task(ReqRunTask {
        task_id: task_id.to_string(),
    }).await {
        Ok(response) => {
            // Success
            Ok(())
        },
        Err(status) => {
            match status.code() {
                tonic::Code::Unauthenticated => {
                    // Re-authenticate
                    refresh_token().await?;
                    // Retry once
                    execute_task(task_id).await
                },
                tonic::Code::Unavailable => {
                    // Retry with backoff
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    execute_task(task_id).await
                },
                tonic::Code::PermissionDenied => {
                    // Fatal error
                    Err(OperatorError::Auth(status.message().to_string()))
                },
                _ => {
                    // Log and report
                    Err(OperatorError::Grpc(status))
                }
            }
        }
    }
}
```

### Retry Logic

Implement exponential backoff for retryable errors:

```rust
use tokio::time::{sleep, Duration};

async fn retry_with_backoff<F, T>(
    mut f: F,
    max_retries: u32,
) -> Result<T, OperatorError>
where
    F: FnMut() -> BoxFuture<'static, Result<T, tonic::Status>>,
{
    let mut retries = 0;
    let mut delay = Duration::from_millis(100);

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(status) if is_retryable(&status) && retries < max_retries => {
                retries += 1;
                tracing::warn!(
                    "Retrying after error: {} (attempt {}/{})",
                    status.message(),
                    retries,
                    max_retries
                );
                sleep(delay).await;
                delay *= 2;  // Exponential backoff
            },
            Err(status) => {
                return Err(OperatorError::Grpc(status));
            }
        }
    }
}

fn is_retryable(status: &tonic::Status) -> bool {
    matches!(
        status.code(),
        tonic::Code::Unavailable
            | tonic::Code::DeadlineExceeded
            | tonic::Code::Aborted
            | tonic::Code::ResourceExhausted
    )
}
```

---

## Complete Integration Example

Here's a complete example of a minimal operator:

```rust
use tonic::transport::Channel;
use tercen::*;

pub struct TercenOperator {
    channel: Channel,
    auth: AuthInterceptor,
}

impl TercenOperator {
    pub async fn new(
        endpoint: &str,
        username: &str,
        password: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Create channel
        let channel = create_channel(endpoint).await?;

        // Authenticate
        let mut user_client = UserServiceClient::new(channel.clone());
        let token_response = user_client
            .generate_token(ReqGenerateToken {
                username: username.to_string(),
                password: password.to_string(),
                refresh_token: None,
            })
            .await?;

        let token = token_response.into_inner().access_token;
        let auth = AuthInterceptor {
            token: Arc::new(RwLock::new(token)),
        };

        Ok(Self { channel, auth })
    }

    pub async fn run(&self, task_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Create clients
        let mut task_client = TaskServiceClient::with_interceptor(
            self.channel.clone(),
            self.auth.clone(),
        );
        let mut table_client = TableSchemaServiceClient::with_interceptor(
            self.channel.clone(),
            self.auth.clone(),
        );
        let mut file_client = FileServiceClient::with_interceptor(
            self.channel.clone(),
            self.auth.clone(),
        );

        // 1. Wait for task
        let task = task_client
            .wait_done(ReqWaitDone {
                task_id: task_id.to_string(),
            })
            .await?
            .into_inner()
            .result
            .ok_or("No task returned")?;

        // 2. Update to running
        // ... (see Task Lifecycle Pattern above)

        // 3. Get data
        let df = self.fetch_data(&mut table_client, &task).await?;

        // 4. Generate plot
        let png = self.generate_plot(df).await?;

        // 5. Upload result
        let file = self.upload_png(&mut file_client, png, &task).await?;

        // 6. Update task to done
        // ... (see Task Lifecycle Pattern above)

        Ok(())
    }

    async fn fetch_data(
        &self,
        table_client: &mut TableSchemaServiceClient<InterceptedService<Channel, AuthInterceptor>>,
        task: &ETask,
    ) -> Result<DataFrame, Box<dyn std::error::Error>> {
        // Extract table ID from task
        let computation_task = extract_computation_task(task)?;

        // Stream table data
        let mut stream = table_client
            .stream_table(ReqStreamTable {
                table_id: computation_task.input_table_id.clone(),
                cnames: vec![],
                offset: 0,
                limit: 0,
                binary_format: "csv".to_string(),
            })
            .await?
            .into_inner();

        // Collect and parse
        let mut chunks = Vec::new();
        while let Some(chunk) = stream.message().await? {
            chunks.push(chunk.result);
        }

        let csv_data = String::from_utf8(chunks.concat())?;
        let df = parse_csv_to_dataframe(&csv_data)?;

        Ok(df)
    }

    async fn generate_plot(&self, df: DataFrame) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Use GGRS to generate plot
        // ... (implementation in Phase 3)
        Ok(vec![])  // Placeholder
    }

    async fn upload_png(
        &self,
        file_client: &mut FileServiceClient<InterceptedService<Channel, AuthInterceptor>>,
        png_bytes: Vec<u8>,
        task: &ETask,
    ) -> Result<EFileDocument, Box<dyn std::error::Error>> {
        // ... (see File Upload Pattern above)
        Ok(EFileDocument::default())  // Placeholder
    }
}
```

---

## Testing gRPC Integration

### Unit Tests with Mock Server

Use `tonic::transport::server::Server` for testing:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tonic::transport::Server;

    #[tokio::test]
    async fn test_fetch_data() {
        // Create mock server
        let (client, server) = tokio::io::duplex(1024);

        tokio::spawn(async move {
            Server::builder()
                .add_service(TableSchemaServiceServer::new(MockTableService))
                .serve_with_incoming(/* ... */)
                .await
        });

        // Test client
        let mut client = TableSchemaServiceClient::new(/* ... */);
        let result = client.stream_table(/* ... */).await;
        assert!(result.is_ok());
    }
}
```

### Integration Tests

Test against real Tercen instance:

```rust
#[tokio::test]
#[ignore]  // Only run with --ignored flag
async fn test_real_tercen_connection() {
    let endpoint = env::var("TERCEN_ENDPOINT").unwrap();
    let username = env::var("TERCEN_USERNAME").unwrap();
    let password = env::var("TERCEN_PASSWORD").unwrap();

    let operator = TercenOperator::new(&endpoint, &username, &password)
        .await
        .unwrap();

    // Test actual operations
    // ...
}
```

---

## Performance Considerations

### Connection Pooling

Reuse gRPC channels:
```rust
// Create once
let channel = create_channel(endpoint).await?;

// Share across services
let task_client = TaskServiceClient::new(channel.clone());
let table_client = TableSchemaServiceClient::new(channel.clone());
```

### Streaming Optimization

Process chunks as they arrive:
```rust
let mut stream = table_client.stream_table(request).await?.into_inner();

while let Some(chunk) = stream.message().await? {
    // Process chunk immediately
    process_chunk(chunk.result)?;
    // Don't accumulate all chunks in memory
}
```

### Request Timeouts

Set timeouts for long-running operations:
```rust
use tonic::Request;

let mut request = Request::new(ReqStreamTable { /* ... */ });
request.set_timeout(Duration::from_secs(300));  // 5 minutes

let response = table_client.stream_table(request).await?;
```

---

## Security Best Practices

1. **Token Storage**: Never hardcode tokens, use environment variables or secure vaults
2. **TLS**: Always use TLS for production connections
3. **Token Rotation**: Implement automatic token refresh
4. **Error Messages**: Don't expose sensitive info in error messages
5. **Logging**: Redact tokens and passwords from logs
6. **Rate Limiting**: Respect Tercen's rate limits

---

## Troubleshooting

### Common Issues

**Issue**: `UNAUTHENTICATED` errors
**Solution**: Check token is being sent, verify not expired, re-authenticate

**Issue**: `DEADLINE_EXCEEDED` on large data
**Solution**: Increase timeout, process in chunks, optimize queries

**Issue**: `UNAVAILABLE` errors
**Solution**: Check network, verify Tercen is running, implement retry logic

**Issue**: Streaming connection drops
**Solution**: Implement reconnection logic, handle partial data

**Issue**: Memory usage spikes
**Solution**: Process chunks without accumulating, use streaming throughout

---

## Next Steps

With this gRPC integration specification, you can:

1. Implement the gRPC client layer (Phase 1)
2. Test authentication and basic connectivity
3. Implement data streaming (Phase 2)
4. Build out the full operator pipeline (Phases 3-5)

Refer to `02_IMPLEMENTATION_PLAN.md` for detailed implementation steps.
