//! GGRS Plot Operator - Main entry point
//!
//! This operator receives tabular data from Tercen via gRPC, generates plots using GGRS,
//! and returns PNG images back to Tercen for visualization.
//!
//! Module organization:
//! - `tercen`: Tercen gRPC client library (future tercen-rust crate)
//! - `ggrs_integration`: GGRS-specific integration code
//! - `config`: Operator configuration

#![allow(dead_code)] // Allow unused functions when building as lib

pub mod config;
pub mod ggrs_integration;
pub mod tercen;

#[cfg(feature = "jemalloc")]
use tikv_jemallocator::Jemalloc;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() {
    println!("GGRS Plot Operator v{}", env!("CARGO_PKG_VERSION"));
    println!("Ready to generate high-performance plots!\n");

    // Parse command-line arguments
    // Production: Tercen passes --taskId, --serviceUri, --token
    // Dev: Can pass --workflowId, --stepId (like Python OperatorContextDev)
    let args: Vec<String> = std::env::args().collect();
    parse_args(&args);

    // Load operator configuration
    let config = config::OperatorConfig::load();

    // Print environment info
    print_env_info();

    // Connect to Tercen
    println!("Attempting to connect to Tercen...");
    match tercen::TercenClient::from_env().await {
        Ok(client) => {
            println!("✓ Successfully connected to Tercen!\n");

            // Process task if TERCEN_TASK_ID is set
            if let Ok(task_id) = std::env::var("TERCEN_TASK_ID") {
                // Create Arc for sharing client across async operations
                let client_arc = std::sync::Arc::new(client);
                let logger = tercen::TercenLogger::new(&client_arc, task_id.clone());

                match process_task(client_arc.clone(), &task_id, &logger, &config).await {
                    Ok(()) => {
                        println!("\n✓ Task processed successfully!");
                    }
                    Err(e) => {
                        eprintln!("\n✗ Task processing failed: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                println!("No TERCEN_TASK_ID set, skipping task processing");
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to connect to Tercen: {}", e);
            eprintln!("\nNote: To run the operator, set environment variables:");
            eprintln!("  export TERCEN_URI=https://tercen.com:5400");
            eprintln!("  export TERCEN_TOKEN=your_token_here");
            eprintln!("  export TERCEN_TASK_ID=your_task_id_here");
            std::process::exit(1);
        }
    }

    println!("\nOperator completed!");
}

/// Parse command-line arguments and set environment variables
fn parse_args(args: &[String]) {
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--taskId" if i + 1 < args.len() => {
                std::env::set_var("TERCEN_TASK_ID", &args[i + 1]);
                i += 2;
            }
            "--workflowId" if i + 1 < args.len() => {
                std::env::set_var("WORKFLOW_ID", &args[i + 1]);
                i += 2;
            }
            "--stepId" if i + 1 < args.len() => {
                std::env::set_var("STEP_ID", &args[i + 1]);
                i += 2;
            }
            "--serviceUri" if i + 1 < args.len() => {
                std::env::set_var("TERCEN_URI", &args[i + 1]);
                i += 2;
            }
            "--token" if i + 1 < args.len() => {
                std::env::set_var("TERCEN_TOKEN", &args[i + 1]);
                i += 2;
            }
            _ => i += 1,
        }
    }
}

/// Print environment info for debugging
fn print_env_info() {
    if let Ok(task_id) = std::env::var("TERCEN_TASK_ID") {
        println!("TERCEN_TASK_ID: {}", task_id);
    } else {
        println!("TERCEN_TASK_ID not set");
    }

    if let Ok(uri) = std::env::var("TERCEN_URI") {
        println!("TERCEN_URI: {}", uri);
    } else {
        println!("TERCEN_URI not set");
    }

    if let Ok(token) = std::env::var("TERCEN_TOKEN") {
        println!(
            "TERCEN_TOKEN: {}...{}",
            &token[..8.min(token.len())],
            if token.len() > 8 { "***" } else { "" }
        );
    } else {
        println!("TERCEN_TOKEN not set");
    }
    println!();
}

/// Process a Tercen task: fetch data, generate plot, upload result
async fn process_task(
    client_arc: std::sync::Arc<tercen::TercenClient>,
    task_id: &str,
    _logger: &tercen::TercenLogger<'_>,
    config: &config::OperatorConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use tercen::client::proto::GetRequest;

    println!("=== Task Processing Started ===");
    println!("Task ID: {}\n", task_id);

    // Step 1: Fetch task information
    println!("[1/5] Fetching task information...");
    let mut task_service = client_arc.task_service()?;
    let request = tonic::Request::new(GetRequest {
        id: task_id.to_string(),
        ..Default::default()
    });

    let response = task_service.get(request).await?;
    let task = response.into_inner();

    println!("✓ Task retrieved");

    // Step 2: Extract cube query from task
    println!("\n[2/5] Extracting cube query...");
    let (cube_query, _computation_task) = extract_cube_query(&task)?;

    println!("✓ Cube query extracted");
    println!("  Main table: {}", cube_query.qt_hash);
    println!("  Column facets: {}", cube_query.column_hash);
    println!("  Row facets: {}", cube_query.row_hash);

    // Find Y-axis table (4th table in schema_ids, if it exists)
    let y_axis_table_id = find_y_axis_table(&client_arc, &task).await.ok();
    if let Some(ref id) = y_axis_table_id {
        println!("  Y-axis table: {}", id);
    } else {
        println!("  Y-axis table: None (will compute from data)");
    }

    // Step 3: Create stream generator
    println!("\n[3/5] Creating stream generator...");
    use ggrs_core::stream::StreamGenerator;

    let stream_gen = ggrs_integration::TercenStreamGenerator::new(
        client_arc.clone(),
        cube_query.qt_hash.clone(),
        cube_query.column_hash.clone(),
        cube_query.row_hash.clone(),
        y_axis_table_id,
        config.chunk_size,
    )
    .await?;

    println!("✓ Stream generator created");
    println!(
        "  Facets: {} columns × {} rows = {} cells",
        stream_gen.n_col_facets(),
        stream_gen.n_row_facets(),
        stream_gen.n_col_facets() * stream_gen.n_row_facets()
    );

    // Step 4: Generate plot
    println!("\n[4/5] Generating plot...");
    use ggrs_core::{EnginePlotSpec, Geom, ImageRenderer, PlotGenerator};

    let plot_spec = EnginePlotSpec::new().add_layer(Geom::point());
    let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;
    let renderer = ImageRenderer::new(
        plot_gen,
        config.default_plot_width,
        config.default_plot_height,
    );

    println!("  Rendering plot (backend: {})...", config.render_backend);
    let png_buffer = renderer.render_to_bytes()?;
    println!("✓ Plot generated ({} bytes)", png_buffer.len());

    // Step 5: Upload result
    println!("\n[5/5] Uploading result to Tercen...");
    tercen::result::save_result(client_arc, &_computation_task, png_buffer).await?;
    println!("✓ Result uploaded successfully");

    println!("\n=== Task Processing Complete ===");
    Ok(())
}

/// Extract CubeQuery from task
fn extract_cube_query(
    task: &tercen::client::proto::ETask,
) -> Result<(CubeQuery, tercen::client::proto::ComputationTask), Box<dyn std::error::Error>> {
    use tercen::client::proto::e_task;

    let task_obj = task.object.as_ref().ok_or("Task has no object")?;

    match task_obj {
        e_task::Object::Computationtask(ct) => {
            let query = ct.query.as_ref().ok_or("ComputationTask has no query")?;
            Ok((query.clone().into(), ct.clone()))
        }
        e_task::Object::Runcomputationtask(rct) => {
            let query = rct
                .query
                .as_ref()
                .ok_or("RunComputationTask has no query")?;

            // Convert RunComputationTask to ComputationTask for result upload
            // (they have the same structure)
            let ct = tercen::client::proto::ComputationTask {
                id: rct.id.clone(),
                owner: rct.owner.clone(),
                project_id: rct.project_id.clone(),
                query: rct.query.clone(),
                ..Default::default()
            };

            Ok((query.clone().into(), ct))
        }
        e_task::Object::Cubequerytask(cqt) => {
            let query = cqt.query.as_ref().ok_or("CubeQueryTask has no query")?;

            // Convert CubeQueryTask to ComputationTask
            let ct = tercen::client::proto::ComputationTask {
                id: cqt.id.clone(),
                owner: cqt.owner.clone(),
                project_id: cqt.project_id.clone(),
                query: cqt.query.clone(),
                ..Default::default()
            };

            Ok((query.clone().into(), ct))
        }
        _ => Err("Unsupported task type".into()),
    }
}

/// Find Y-axis table from task schema_ids
async fn find_y_axis_table(
    client: &std::sync::Arc<tercen::TercenClient>,
    task: &tercen::client::proto::ETask,
) -> Result<String, Box<dyn std::error::Error>> {
    use tercen::client::proto::e_task;
    use tercen::TableStreamer;

    let streamer = TableStreamer::new(client);

    // Get schema_ids from task
    let schema_ids = match task.object.as_ref() {
        Some(e_task::Object::Cubequerytask(cqt)) => &cqt.schema_ids,
        _ => return Err("Task is not a CubeQueryTask".into()),
    };

    // Find the extra table (not qt, column, or row)
    let cube_query = match task.object.as_ref() {
        Some(e_task::Object::Cubequerytask(cqt)) => cqt.query.as_ref().ok_or("No query in task")?,
        _ => return Err("Task is not a CubeQueryTask".into()),
    };

    let known_tables = [
        cube_query.qt_hash.as_str(),
        cube_query.column_hash.as_str(),
        cube_query.row_hash.as_str(),
    ];

    for schema_id in schema_ids {
        if !known_tables.contains(&schema_id.as_str()) {
            // Check if this is the Y-axis table
            let axis_schema = streamer.get_schema(schema_id).await?;
            use tercen::client::proto::e_schema;
            if let Some(e_schema::Object::Cubequerytableschema(cqts)) = axis_schema.object {
                if cqts.query_table_type == "y" {
                    return Ok(schema_id.clone());
                }
            }
        }
    }

    Err("Y-axis table not found".into())
}

/// Simplified CubeQuery struct
struct CubeQuery {
    qt_hash: String,
    column_hash: String,
    row_hash: String,
}

impl From<tercen::client::proto::CubeQuery> for CubeQuery {
    fn from(cq: tercen::client::proto::CubeQuery) -> Self {
        CubeQuery {
            qt_hash: cq.qt_hash,
            column_hash: cq.column_hash,
            row_hash: cq.row_hash,
        }
    }
}
