# Tercen Module - Future Library Extraction

This module contains all Tercen gRPC client code and is designed to be extracted into a separate `tercen-rust` crate in the future.

## Planned Structure

```
src/tercen/
├── mod.rs              # Module root with re-exports
├── client.rs           # TercenClient with connection and auth
├── error.rs            # TercenError type
├── types.rs            # Common types and conversions
└── services/
    ├── mod.rs
    ├── task.rs         # TaskService wrapper
    ├── table.rs        # TableSchemaService wrapper
    └── file.rs         # FileService wrapper
```

## Extraction Plan

When ready to create the `tercen-rust` library:

### Step 1: Create new crate
```bash
cd ..
cargo new --lib tercen-rust
```

### Step 2: Move module contents
```bash
cp -r ggrs_plot_operator/src/tercen/* tercen-rust/src/
```

### Step 3: Move proto files
```bash
mkdir tercen-rust/protos
cp ggrs_plot_operator/protos/*.proto tercen-rust/protos/
```

### Step 4: Update dependencies
In `tercen-rust/Cargo.toml`:
```toml
[dependencies]
tonic = "0.11"
prost = "0.12"
tokio = { version = "1.35", features = ["rt-multi-thread"] }
thiserror = "1.0"

[build-dependencies]
tonic-build = "0.11"
```

### Step 5: Update this project
In `ggrs_plot_operator/Cargo.toml`:
```toml
[dependencies]
tercen-rust = { path = "../tercen-rust" }
# Or from crates.io when published:
# tercen-rust = "0.1"
```

In `ggrs_plot_operator/src/main.rs`:
```rust
// Remove: mod tercen;
// Add:
use tercen_rust::{TercenClient, TercenError};
```

## Design Principles

To make extraction easy, this module follows these principles:

1. **Self-contained**: All Tercen-specific code lives here
2. **No GGRS dependencies**: Keep plotting logic separate
3. **Clear public API**: Re-export only what's needed via mod.rs
4. **Standard error handling**: Use thiserror for error types
5. **Async-first**: All I/O operations are async

## Public API (when complete)

```rust
// Main client
pub struct TercenClient { /* ... */ }

impl TercenClient {
    pub async fn connect(uri: &str, username: &str, password: &str) -> Result<Self>;
    pub async fn from_env() -> Result<Self>;

    pub fn task_service(&self) -> TaskService;
    pub fn table_service(&self) -> TableService;
    pub fn file_service(&self) -> FileService;
}

// Services
pub struct TaskService { /* ... */ }
pub struct TableService { /* ... */ }
pub struct FileService { /* ... */ }

// Error handling
pub enum TercenError { /* ... */ }
```

## Usage Example (after extraction)

```rust
use tercen_rust::{TercenClient, TercenError};

#[tokio::main]
async fn main() -> Result<(), TercenError> {
    // Connect to Tercen
    let client = TercenClient::from_env().await?;

    // Use services
    let task = client.task_service().get_task("task-id").await?;
    let data = client.table_service()
        .stream_table("table-id", &[".x", ".y"])
        .await?;

    Ok(())
}
```
