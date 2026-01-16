//! GGRS Plot Operator - Main entry point
//!
//! This operator receives tabular data from Tercen via gRPC, generates plots using GGRS,
//! and returns PNG images back to Tercen for visualization.
//!
//! Module organization:
//! - `tercen`: Tercen gRPC client library (future tercen-rust crate)
//! - `ggrs_integration`: GGRS-specific integration code
//! - `config`: Operator configuration

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
    println!("Build timestamp: 2026-01-12 08:50:00"); // Force cache bust

    // Parse command-line arguments
    // Production: Tercen passes --taskId, --serviceUri, --token
    // Dev: Can pass --workflowId, --stepId (like Python OperatorContextDev)
    let args: Vec<String> = std::env::args().collect();
    parse_args(&args);

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

                match process_task(client_arc.clone(), &task_id).await {
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
    let mut task = response.into_inner();

    println!("✓ Task retrieved");

    // Step 2: Extract cube query, project_id, namespace, and operator settings from task
    println!("\n[2/5] Extracting cube query and properties...");
    let (cube_query, project_id, namespace, operator_settings) = extract_cube_query(&task)?;

    // Debug: Print operator settings if available
    if let Some(ref settings) = operator_settings {
        println!("\n=== Debug: Operator Settings ===");
        if let Some(ref op_ref) = settings.operator_ref {
            println!("  Operator ID: {}", op_ref.operator_id);
            println!("  Property values count: {}", op_ref.property_values.len());
            for prop in &op_ref.property_values {
                println!("    Property '{}' = '{}'", prop.name, prop.value);
            }
        } else {
            println!("  No operator_ref in settings");
        }
    } else {
        println!("\n=== Debug: No operator settings in task ===");
    }

    // Load operator configuration from properties
    let config = config::OperatorConfig::from_properties(operator_settings.as_ref());

    println!("\n✓ Cube query extracted");
    println!("  Main table: {}", cube_query.qt_hash);
    println!("  Column facets: {}", cube_query.column_hash);
    println!("  Row facets: {}", cube_query.row_hash);
    println!("  Project ID: {}", project_id);
    println!("  Namespace: {}", namespace);
    println!("\n✓ Configuration loaded");
    println!("  Backend: {}", config.backend);
    println!("  Point size: {}", config.point_size);
    println!(
        "  Plot dimensions: {:?} × {:?}",
        config.plot_width, config.plot_height
    );

    // Find Y-axis table (4th table in schema_ids, if it exists)
    let y_axis_table_id = find_y_axis_table(&client_arc, &task).await.ok();
    if let Some(ref id) = y_axis_table_id {
        println!("  Y-axis table: {}", id);
    } else {
        println!("  Y-axis table: None (will compute from data)");
    }

    // Step 2.5: Fetch workflow and extract color information
    println!("\n[2.5/5] Extracting color information...");
    let color_infos = extract_color_info(&client_arc).await?;
    if color_infos.is_empty() {
        println!("  No color factors defined");
    } else {
        for (i, info) in color_infos.iter().enumerate() {
            println!("  Color {} : '{}'", i + 1, info.factor_name);
            println!("    Type: {}", info.factor_type);
            match &info.mapping {
                tercen::ColorMapping::Continuous(palette) => {
                    if let Some((min, max)) = palette.range() {
                        println!("    Range: {} to {}", min, max);
                        println!("    Stops: {}", palette.stops.len());
                    }
                }
                tercen::ColorMapping::Categorical(color_map) => {
                    println!("    Categories: {}", color_map.mappings.len());
                }
            }
        }
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
        color_infos,
    )
    .await?;

    println!("✓ Stream generator created");
    println!(
        "  Facets: {} columns × {} rows = {} cells",
        stream_gen.n_col_facets(),
        stream_gen.n_row_facets(),
        stream_gen.n_col_facets() * stream_gen.n_row_facets()
    );

    // Resolve "auto" plot dimensions now that we know facet counts
    let (plot_width, plot_height) =
        config.resolve_dimensions(stream_gen.n_col_facets(), stream_gen.n_row_facets());
    println!(
        "  Resolved plot size: {}×{} pixels",
        plot_width, plot_height
    );

    // Step 4: Generate plot
    println!("\n[4/5] Generating plot...");
    use ggrs_core::renderer::{BackendChoice, OutputFormat};
    use ggrs_core::{EnginePlotSpec, Geom, PlotGenerator, PlotRenderer};
    use std::fs;

    let plot_spec = EnginePlotSpec::new().add_layer(Geom::point_sized(config.point_size as f64));
    let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;
    let renderer = PlotRenderer::new(&plot_gen, plot_width as u32, plot_height as u32);

    println!("  Rendering plot (backend: {})...", config.backend);
    let backend = match config.backend.as_str() {
        "gpu" => BackendChoice::WebGPU,
        _ => BackendChoice::Cairo,
    };

    // Render to temporary file
    let temp_path = "temp_plot.png";
    renderer.render_to_file(temp_path, backend, OutputFormat::Png)?;

    // Read PNG into memory
    let png_buffer = fs::read(temp_path)?;
    fs::remove_file(temp_path)?;

    println!("✓ Plot generated ({} bytes)", png_buffer.len());

    // Step 5: Upload result and update task
    println!("\n[5/5] Uploading result to Tercen...");
    tercen::result::save_result(
        client_arc.clone(),
        &project_id,
        &namespace,
        png_buffer,
        plot_width,
        plot_height,
        &mut task,
    )
    .await?;
    println!("✓ Result uploaded and linked successfully");

    println!("\n=== Task Processing Complete ===");
    Ok(())
}

/// Type alias for cube query extraction result
type CubeQueryResult = (
    CubeQuery,
    String,
    String,
    Option<tercen::client::proto::OperatorSettings>,
);

/// Extract CubeQuery, project_id, namespace, and operator settings from task
fn extract_cube_query(
    task: &tercen::client::proto::ETask,
) -> Result<CubeQueryResult, Box<dyn std::error::Error>> {
    use tercen::client::proto::e_task;

    let task_obj = task.object.as_ref().ok_or("Task has no object")?;

    match task_obj {
        e_task::Object::Computationtask(ct) => {
            let query = ct.query.as_ref().ok_or("ComputationTask has no query")?;
            let namespace = query
                .operator_settings
                .as_ref()
                .map(|os| os.namespace.clone())
                .unwrap_or_default();
            let operator_settings = query.operator_settings.clone();
            Ok((
                query.clone().into(),
                ct.project_id.clone(),
                namespace,
                operator_settings,
            ))
        }
        e_task::Object::Runcomputationtask(rct) => {
            let query = rct
                .query
                .as_ref()
                .ok_or("RunComputationTask has no query")?;
            let namespace = query
                .operator_settings
                .as_ref()
                .map(|os| os.namespace.clone())
                .unwrap_or_default();
            let operator_settings = query.operator_settings.clone();
            Ok((
                query.clone().into(),
                rct.project_id.clone(),
                namespace,
                operator_settings,
            ))
        }
        e_task::Object::Cubequerytask(cqt) => {
            let query = cqt.query.as_ref().ok_or("CubeQueryTask has no query")?;
            let namespace = query
                .operator_settings
                .as_ref()
                .map(|os| os.namespace.clone())
                .unwrap_or_default();
            let operator_settings = query.operator_settings.clone();
            Ok((
                query.clone().into(),
                cqt.project_id.clone(),
                namespace,
                operator_settings,
            ))
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

/// Extract color information from workflow
async fn extract_color_info(
    client: &std::sync::Arc<tercen::TercenClient>,
) -> Result<Vec<tercen::ColorInfo>, Box<dyn std::error::Error>> {
    // Get workflow_id and step_id from environment
    let workflow_id = std::env::var("WORKFLOW_ID").ok();
    let step_id = std::env::var("STEP_ID").ok();

    // If either is missing, return empty (no colors)
    let (workflow_id, step_id) = match (workflow_id, step_id) {
        (Some(wid), Some(sid)) => (wid, sid),
        _ => {
            println!("  Workflow/Step IDs not provided - skipping color extraction");
            return Ok(Vec::new());
        }
    };

    // Fetch workflow using WorkflowService
    let mut workflow_service = client.workflow_service()?;
    let request = tonic::Request::new(tercen::client::proto::GetRequest {
        id: workflow_id.clone(),
        ..Default::default()
    });

    let response = workflow_service.get(request).await?;
    let e_workflow = response.into_inner();

    // Extract the Workflow from EWorkflow
    let workflow = e_workflow
        .object
        .as_ref()
        .map(|obj| match obj {
            tercen::client::proto::e_workflow::Object::Workflow(wf) => wf,
        })
        .ok_or("EWorkflow has no workflow object")?;

    // Extract color info from step
    let color_infos = tercen::extract_color_info_from_step(workflow, &step_id)?;

    Ok(color_infos)
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
