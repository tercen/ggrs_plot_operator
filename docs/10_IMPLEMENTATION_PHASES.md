# Implementation Phases - Revised

## Phase 0: Project Setup ✓ (Already Complete)

- ✅ Repository initialized
- ✅ Documentation created
- ✅ Docker and CI/CD configured
- ✅ Architecture designed

---

## Phase 1: CI/CD and Basic Operator Structure

**Goal**: Get CI workflow running successfully with minimal Rust operator

### Tasks

1. **Create operator.json**
   ```json
   {
     "name": "GGRS Plot",
     "description": "High-performance plotting operator using GGRS",
     "tags": ["visualization", "plot", "chart"],
     "authors": ["Tercen"],
     "urls": ["https://github.com/tercen/ggrs_plot_operator"],
     "container": "ghcr.io/tercen/ggrs_plot_operator:main",
     "properties": [
       {
         "kind": "StringProperty",
         "name": "title",
         "defaultValue": "",
         "description": "Plot title"
       },
       {
         "kind": "DoubleProperty",
         "name": "width",
         "defaultValue": 800,
         "description": "Plot width in pixels"
       },
       {
         "kind": "DoubleProperty",
         "name": "height",
         "defaultValue": 600,
         "description": "Plot height in pixels"
       },
       {
         "kind": "EnumeratedProperty",
         "name": "theme",
         "defaultValue": "gray",
         "values": ["gray", "bw", "minimal"],
         "description": "Plot theme"
       }
     ]
   }
   ```

2. **Initialize Cargo project**
   ```bash
   cargo init --name ggrs_plot_operator
   ```

3. **Create minimal Cargo.toml**
   ```toml
   [package]
   name = "ggrs_plot_operator"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   # Just basic deps for now
   tokio = { version = "1.35", features = ["full"] }
   ```

4. **Create minimal main.rs**
   ```rust
   #[tokio::main]
   async fn main() {
       println!("GGRS Plot Operator v{}", env!("CARGO_PKG_VERSION"));
       println!("Phase 1: Basic structure");
       println!("Operator starting...");

       // Read environment variables to verify Docker setup
       let uri = std::env::var("TERCEN_URI")
           .unwrap_or_else(|_| "not_set".to_string());
       let task_id = std::env::var("TERCEN_TASK_ID")
           .unwrap_or_else(|_| "not_set".to_string());

       println!("Environment:");
       println!("  TERCEN_URI: {}", uri);
       println!("  TERCEN_TASK_ID: {}", task_id);

       println!("Operator completed successfully!");
       std::process::exit(0);
   }
   ```

5. **Test locally**
   ```bash
   cargo build --release
   cargo run
   ```

6. **Test Docker build**
   ```bash
   docker build -t ggrs_plot_operator:test .
   docker run --rm ggrs_plot_operator:test
   ```

7. **Push to main branch**
   - CI workflow should run
   - Tests should pass (cargo fmt, clippy, test, doc)
   - Docker image should build
   - Image should be pushed to ghcr.io

**Deliverable**:
- ✅ CI workflow passes
- ✅ Docker image builds successfully
- ✅ Operator runs and exits cleanly
- ✅ `operator.json` defines basic properties

**Success Criteria**:
```bash
$ docker run --rm ghcr.io/tercen/ggrs_plot_operator:main
GGRS Plot Operator v0.1.0
Phase 1: Basic structure
Operator starting...
Environment:
  TERCEN_URI: not_set
  TERCEN_TASK_ID: not_set
Operator completed successfully!
```

---

## Phase 2: gRPC Connection and Simple Call

**Goal**: Connect to Tercen, authenticate, make a simple API call

### Tasks

1. **Copy proto files**
   ```bash
   mkdir -p protos
   cp /home/thiago/workspaces/tercen/main/sci/tercen_grpc/tercen_grpc_api/protos/*.proto protos/
   ```

2. **Update Cargo.toml**
   ```toml
   [dependencies]
   tokio = { version = "1.35", features = ["full"] }
   tonic = { version = "0.11", features = ["tls"] }
   prost = "0.12"
   prost-types = "0.12"
   anyhow = "1.0"

   [build-dependencies]
   tonic-build = "0.11"
   ```

3. **Create build.rs**
   ```rust
   fn main() -> Result<(), Box<dyn std::error::Error>> {
       tonic_build::configure()
           .build_server(false)
           .compile(
               &["protos/tercen.proto", "protos/tercen_model.proto"],
               &["protos"],
           )?;
       Ok(())
   }
   ```

4. **Update main.rs to connect**
   ```rust
   use tonic::transport::Channel;

   mod tercen {
       tonic::include_proto!("tercen");
   }

   #[tokio::main]
   async fn main() -> Result<(), Box<dyn std::error::Error>> {
       println!("GGRS Plot Operator v{}", env!("CARGO_PKG_VERSION"));
       println!("Phase 2: gRPC Connection");

       // Get environment
       let uri = std::env::var("TERCEN_URI")?;
       let username = std::env::var("TERCEN_USERNAME")?;
       let password = std::env::var("TERCEN_PASSWORD")?;

       println!("Connecting to {}...", uri);

       // Create channel
       let channel = Channel::from_shared(uri)?
           .connect()
           .await?;

       println!("Connected!");

       // Create UserService client
       let mut user_client = tercen::user_service::UserServiceClient::new(channel);

       // Authenticate
       println!("Authenticating as {}...", username);
       let request = tercen::ReqGenerateToken {
           domain: "".to_string(),
           username_or_email: username,
           password,
           ..Default::default()
       };

       let response = user_client.connect2(request).await?;
       let session = response.into_inner();

       println!("Authenticated successfully!");
       println!("Token length: {} chars", session.token.map(|t| t.len()).unwrap_or(0));

       println!("Phase 2 completed successfully!");
       Ok(())
   }
   ```

5. **Test with real Tercen instance**
   ```bash
   export TERCEN_URI=http://127.0.0.1:5402
   export TERCEN_USERNAME=admin
   export TERCEN_PASSWORD=admin
   cargo run
   ```

6. **Test in Docker**
   ```bash
   docker build -t ggrs_plot_operator:test .
   docker run --rm \
     -e TERCEN_URI=http://host.docker.internal:5402 \
     -e TERCEN_USERNAME=admin \
     -e TERCEN_PASSWORD=admin \
     ggrs_plot_operator:test
   ```

**Deliverable**:
- ✅ Proto files compiled
- ✅ Connects to Tercen
- ✅ Authenticates successfully
- ✅ Prints token info

**Success Criteria**:
```bash
$ cargo run
GGRS Plot Operator v0.1.0
Phase 2: gRPC Connection
Connecting to http://127.0.0.1:5402...
Connected!
Authenticating as admin...
Authenticated successfully!
Token length: 36 chars
Phase 2 completed successfully!
```

---

## Phase 3: Streaming Data - Test Chunking

**Goal**: Query Tercen data in chunks, verify correct data retrieval

### Tasks

1. **Add dependencies**
   ```toml
   [dependencies]
   csv = "1.3"
   serde = { version = "1.0", features = ["derive"] }
   ```

2. **Create streaming module**
   ```rust
   // src/streaming.rs

   use anyhow::Result;
   use tonic::transport::Channel;

   pub struct TercenStreamer {
       table_service: crate::tercen::table_schema_service::TableSchemaServiceClient<Channel>,
   }

   impl TercenStreamer {
       pub fn new(channel: Channel) -> Self {
           Self {
               table_service: crate::tercen::table_schema_service::TableSchemaServiceClient::new(channel),
           }
       }

       pub async fn test_streaming(&mut self, table_id: &str) -> Result<()> {
           println!("Testing streaming from table: {}", table_id);

           let chunk_size = 1000;
           let mut offset = 0;
           let mut total_rows = 0;
           let mut chunk_count = 0;

           loop {
               println!("\nFetching chunk {} (offset: {}, limit: {})",
                   chunk_count + 1, offset, chunk_size);

               let request = crate::tercen::ReqStreamTable {
                   table_id: table_id.to_string(),
                   cnames: vec![], // All columns
                   offset,
                   limit: chunk_size,
                   binary_format: "csv".to_string(),
               };

               let response = self.table_service.stream_table(request).await?;
               let mut stream = response.into_inner();

               let mut chunk_data = Vec::new();
               while let Some(msg) = stream.message().await? {
                   chunk_data.extend_from_slice(&msg.result);
               }

               if chunk_data.is_empty() {
                   println!("No more data");
                   break;
               }

               // Parse CSV to count rows
               let csv_str = String::from_utf8_lossy(&chunk_data);
               let row_count = csv_str.lines().count().saturating_sub(1); // -1 for header

               println!("Received {} bytes, {} rows", chunk_data.len(), row_count);

               // Print first few lines of first chunk
               if chunk_count == 0 {
                   let lines: Vec<_> = csv_str.lines().take(5).collect();
                   println!("First few lines:");
                   for line in lines {
                       println!("  {}", line);
                   }
               }

               total_rows += row_count;
               chunk_count += 1;
               offset += chunk_size;

               // Safety check
               if chunk_count > 100 {
                   println!("Safety limit reached (100 chunks)");
                   break;
               }

               if row_count < chunk_size as usize {
                   println!("Last chunk (smaller than requested)");
                   break;
               }
           }

           println!("\nStreaming complete!");
           println!("Total chunks: {}", chunk_count);
           println!("Total rows: {}", total_rows);

           Ok(())
       }
   }
   ```

3. **Update main.rs**
   ```rust
   mod streaming;

   #[tokio::main]
   async fn main() -> Result<(), Box<dyn std::error::Error>> {
       println!("GGRS Plot Operator v{}", env!("CARGO_PKG_VERSION"));
       println!("Phase 3: Streaming Test");

       // Connect and authenticate (from Phase 2)
       let uri = std::env::var("TERCEN_URI")?;
       let username = std::env::var("TERCEN_USERNAME")?;
       let password = std::env::var("TERCEN_PASSWORD")?;
       let table_id = std::env::var("TERCEN_TABLE_ID")?;

       println!("Connecting to {}...", uri);
       let channel = Channel::from_shared(uri)?.connect().await?;

       println!("Authenticating...");
       let mut user_client = tercen::user_service::UserServiceClient::new(channel.clone());
       let request = tercen::ReqGenerateToken {
           domain: "".to_string(),
           username_or_email: username,
           password,
           ..Default::default()
       };
       let _response = user_client.connect2(request).await?;

       println!("Authenticated!\n");

       // Test streaming
       let mut streamer = streaming::TercenStreamer::new(channel);
       streamer.test_streaming(&table_id).await?;

       println!("\nPhase 3 completed successfully!");
       Ok(())
   }
   ```

4. **Test with real table**
   ```bash
   export TERCEN_TABLE_ID=<actual_table_id>
   cargo run
   ```

5. **Verify chunking works**
   - Check that data is retrieved in chunks
   - Verify offset/limit work correctly
   - Ensure CSV parsing succeeds
   - Confirm total row count matches expected

**Deliverable**:
- ✅ Streams data in configurable chunks
- ✅ Parses CSV correctly
- ✅ Counts rows accurately
- ✅ Handles offset/limit properly

**Success Criteria**:
```bash
$ cargo run
GGRS Plot Operator v0.1.0
Phase 3: Streaming Test
Connecting to http://127.0.0.1:5402...
Authenticating...
Authenticated!

Testing streaming from table: abc123...

Fetching chunk 1 (offset: 0, limit: 1000)
Received 45231 bytes, 1000 rows
First few lines:
  .ci,.ri,.x,.y,sp
  0,0,51.0,6.1,"B"
  0,0,52.0,7.7,"B"
  ...

Fetching chunk 2 (offset: 1000, limit: 1000)
Received 45018 bytes, 1000 rows

...

Streaming complete!
Total chunks: 5
Total rows: 4500

Phase 3 completed successfully!
```

---

## Phase 4: Parse Data and Filter by Facets

**Goal**: Parse CSV into structured data, filter by `.ci` and `.ri`

### Tasks

1. **Create data structures**
   ```rust
   // src/data.rs

   #[derive(Debug, Clone)]
   pub struct Point {
       pub x: f64,
       pub y: f64,
       pub ci: usize,
       pub ri: usize,
    // Additional fields as needed
   }

   pub fn parse_csv_to_points(csv_data: &[u8]) -> Result<Vec<Point>> {
       // Parse CSV and extract points
   }

   pub fn filter_by_facet(points: Vec<Point>, col_idx: usize, row_idx: usize) -> Vec<Point> {
       points
           .into_iter()
           .filter(|p| p.ci == col_idx && p.ri == row_idx)
           .collect()
   }
   ```

2. **Test filtering**
   - Stream data
   - Parse to Points
   - Filter by specific facet
   - Print statistics

**Deliverable**:
- ✅ CSV parser
- ✅ Point structure
- ✅ Facet filtering works

---

## Phase 5: Load Facet Metadata

**Goal**: Load and parse column.csv and row.csv facet tables

### Tasks

1. **Load facet tables**
   ```rust
   pub struct FacetInfo {
       pub index: usize,
       pub label: String,
   }

   pub async fn load_col_facets(client: &mut TableService, table_id: &str) -> Result<Vec<FacetInfo>> {
       // Load and parse column facet table
   }

   pub async fn load_row_facets(client: &mut TableService, table_id: &str) -> Result<Vec<FacetInfo>> {
       // Load and parse row facet table
   }
   ```

2. **Test**
   - Print facet counts
   - Print facet labels
   - Verify structure

**Deliverable**:
- ✅ Facet metadata loading
- ✅ Structure validation

---

## Phase 6: Integrate with GGRS - Basic Plot

**Goal**: Create first plot with GGRS using Tercen data

### Tasks

1. **Add GGRS dependency**
   ```toml
   [dependencies]
   ggrs-core = { path = "../ggrs/crates/ggrs-core" }
   ```

2. **Implement StreamGenerator trait**
   ```rust
   pub struct TercenStreamGenerator {
       // Implementation from Phase 3-5
   }

   impl ggrs_core::StreamGenerator for TercenStreamGenerator {
       // Implement all trait methods
   }
   ```

3. **Generate simple plot**
   ```rust
   let stream_gen = TercenStreamGenerator::from_env().await?;
   let plot_spec = EnginePlotSpec::new().add_layer(Geom::point());
   let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;
   let renderer = ImageRenderer::new(plot_gen, 800, 600);
   let png_bytes = renderer.render_to_buffer()?;

   // Save locally for testing
   std::fs::write("test_plot.png", &png_bytes)?;
   ```

**Deliverable**:
- ✅ First working plot generated
- ✅ PNG file created locally

---

## Phase 7: Output to Tercen Table

**Goal**: Save PNG as base64 to Tercen table

### Tasks

1. **Add base64 dependency**
   ```toml
   [dependencies]
   base64 = "0.21"
   ```

2. **Create result table**
   ```rust
   use base64::Engine;

   let base64_string = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

   // Create result DataFrame
   let result = create_result_dataframe(base64_string, "plot.png", "image/png");

   // Save to Tercen
   save_to_tercen(table_service, result).await?;
   ```

3. **Test**
   - Generate plot
   - Encode to base64
   - Save to table
   - Verify in Tercen UI

**Deliverable**:
- ✅ PNG saved to Tercen table
- ✅ Correct column names (`.content`, `filename`, `mimetype`)
- ✅ Visible in Tercen UI

---

## Phase 8: Full Faceting Support

**Goal**: Support multiple facets, all GGRS features

### Tasks

1. **Test with faceted data**
2. **Verify all facet cells render**
3. **Add operator properties (theme, dimensions)**
4. **Test different plot types**

**Deliverable**:
- ✅ Full faceting works
- ✅ All operator properties functional

---

## Phase 9: Polish and Optimization

**Goal**: Production-ready operator

### Tasks

1. **Error handling**
2. **Logging and progress**
3. **Performance optimization**
4. **Documentation**
5. **Testing suite**

**Deliverable**:
- ✅ Production-ready operator
- ✅ Complete test coverage
- ✅ User documentation

---

## Summary

| Phase | Goal | Key Verification |
|-------|------|------------------|
| 1 | CI/CD works | Docker image builds and runs |
| 2 | gRPC connection | Authenticates successfully |
| 3 | Data streaming | Chunks retrieved correctly |
| 4 | Data parsing | Points filtered by facet |
| 5 | Facet metadata | Col/row facets loaded |
| 6 | GGRS integration | First PNG generated |
| 7 | Tercen output | Base64 saved to table |
| 8 | Full features | Faceting works |
| 9 | Production | Polished operator |

Each phase builds on the previous, with clear success criteria and deliverables.
