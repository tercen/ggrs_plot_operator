// Module declarations
// These modules are organized to make future library extraction easy:
// - `tercen`: All Tercen gRPC client code (will become `tercen-rust` crate)
// - `ggrs_integration`: GGRS-specific integration code
// - `config`: Operator configuration
#![allow(dead_code)] // Allow unused functions when building as lib
pub mod config;
pub mod ggrs_integration;
pub mod tercen;

use tercen::tson_to_dataframe;

#[cfg(feature = "jemalloc")]
use tikv_jemallocator::Jemalloc;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() {
    println!("GGRS Plot Operator v{}", env!("CARGO_PKG_VERSION"));
    println!("Phase 4+: Data Query & Logging Test");

    // Parse command-line arguments
    // Production: Tercen passes --taskId, --serviceUri, --token
    // Dev: Can pass --workflowId, --stepId (like Python OperatorContextDev)
    let args: Vec<String> = std::env::args().collect();
    let mut task_id_arg: Option<String> = None;
    let mut workflow_id_arg: Option<String> = None;
    let mut step_id_arg: Option<String> = None;
    let mut service_uri_arg: Option<String> = None;
    let mut token_arg: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--taskId" if i + 1 < args.len() => {
                task_id_arg = Some(args[i + 1].clone());
                i += 2;
            }
            "--workflowId" if i + 1 < args.len() => {
                workflow_id_arg = Some(args[i + 1].clone());
                i += 2;
            }
            "--stepId" if i + 1 < args.len() => {
                step_id_arg = Some(args[i + 1].clone());
                i += 2;
            }
            "--serviceUri" if i + 1 < args.len() => {
                service_uri_arg = Some(args[i + 1].clone());
                i += 2;
            }
            "--token" if i + 1 < args.len() => {
                token_arg = Some(args[i + 1].clone());
                i += 2;
            }
            _ => i += 1,
        }
    }

    // Override environment variables with command-line arguments (priority: CLI > env)
    // Production mode: taskId provided
    if let Some(task_id) = &task_id_arg {
        std::env::set_var("TERCEN_TASK_ID", task_id);
    }
    // Dev mode: workflowId and stepId provided (used by test scripts)
    if let Some(workflow_id) = &workflow_id_arg {
        std::env::set_var("WORKFLOW_ID", workflow_id);
    }
    if let Some(step_id) = &step_id_arg {
        std::env::set_var("STEP_ID", step_id);
    }
    if let Some(uri) = &service_uri_arg {
        std::env::set_var("TERCEN_URI", uri);
    }
    if let Some(token) = &token_arg {
        std::env::set_var("TERCEN_TOKEN", token);
    }

    // Load operator configuration
    let config = config::OperatorConfig::load();

    // Print environment info to verify operator context
    if let Ok(task_id) = std::env::var("TERCEN_TASK_ID") {
        println!("TERCEN_TASK_ID: {}", task_id);
    } else {
        println!("TERCEN_TASK_ID not set (expected when running outside Tercen)");
    }

    if let Ok(uri) = std::env::var("TERCEN_URI") {
        println!("TERCEN_URI: {}", uri);
    } else {
        println!("TERCEN_URI not set (expected when running outside Tercen)");
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

    // Try to connect to Tercen
    println!("\nAttempting to connect to Tercen...");
    match tercen::TercenClient::from_env().await {
        Ok(client) => {
            println!("✓ Successfully connected to Tercen!");

            // Try to get task if TERCEN_TASK_ID is set
            if let Ok(task_id) = std::env::var("TERCEN_TASK_ID") {
                println!("\nFetching task information (skipping logs for now)...");

                // Skip logging temporarily to test if TaskService works
                let logger = tercen::TercenLogger::new(&client, task_id.clone());

                match process_task(&client, &task_id, &logger, &config).await {
                    Ok(()) => {
                        println!("✓ Task processed successfully!");
                        // if let Err(e) = logger.log("Task completed successfully").await {
                        //     eprintln!("Warning: Failed to send log: {}", e);
                        // }
                    }
                    Err(e) => {
                        eprintln!("✗ Task processing failed: {}", e);
                        // if let Err(log_err) =
                        //     logger.log(format!("Task processing failed: {}", e)).await
                        // {
                        //     eprintln!("Warning: Failed to send error log: {}", log_err);
                        // }
                    }
                }
            } else {
                println!("\nNo TERCEN_TASK_ID set, skipping task processing");
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to connect to Tercen: {}", e);
            eprintln!("\nNote: To test connection, set environment variables:");
            eprintln!("  export TERCEN_URI=https://tercen.com:5400");
            eprintln!("  export TERCEN_TOKEN=your_token_here");
            eprintln!("  export TERCEN_TASK_ID=your_task_id_here");
        }
    }

    println!("Operator completed!");
    std::process::exit(0);
}

/// Process a Tercen task: fetch data, parse, and prepare for plotting
async fn process_task(
    client: &tercen::TercenClient,
    task_id: &str,
    _logger: &tercen::TercenLogger<'_>,
    config: &config::OperatorConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use tercen::client::proto::GetRequest;

    println!("Starting task processing (logging disabled for testing)...");
    // // logger.log("Start task processing").await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Step 1: Fetch task information
    let mut task_service = client.task_service()?;
    let request = tonic::Request::new(GetRequest {
        id: task_id.to_string(),
        ..Default::default()
    });

    let response = task_service.get(request).await?;
    let task = response.into_inner();

    println!("✓ Task retrieved: {}", task_id);
    // // logger.log("Task retrieved successfully").await?;

    // Step 2: Extract table IDs from task (if available)
    // Note: In Phase 4, we demonstrate the structure
    // In Phase 5-6, we'll actually fetch and parse real data
    if let Some(obj) = &task.object {
        match obj {
            tercen::client::proto::e_task::Object::Computationtask(ct) => {
                println!("  Task type: ComputationTask");
                // logger.log("Processing ComputationTask").await?;

                // Check if query exists
                if let Some(query) = &ct.query {
                    println!("  Query found");
                    // logger.log("Query structure detected").await?;

                    // Extract table hashes from CubeQuery
                    let qt_hash = &query.qt_hash;
                    let column_hash = &query.column_hash;
                    let row_hash = &query.row_hash;

                    println!("  Table hashes:");
                    println!("    qt_hash (main data): {}", qt_hash);
                    println!("    column_hash: {}", column_hash);
                    println!("    row_hash: {}", row_hash);

                    // logger.log(format!("Main data table: {}", qt_hash)).await?;
                    // logger.log(format!("Column facet table: {}", column_hash)).await?;
                    // logger.log(format!("Row facet table: {}", row_hash)).await?;

                    // Query the main data table
                    if !qt_hash.is_empty() {
                        println!("\n  Querying main data table...");
                        // logger.log("Starting data query").await?;

                        match query_and_log_data(client, qt_hash, _logger, config).await {
                            Ok(()) => {
                                println!("  ✓ Data query completed successfully");
                                // logger.log("Data query completed").await?;
                            }
                            Err(e) => {
                                eprintln!("  ✗ Data query failed: {}", e);
                                // logger.log(format!("Data query failed: {}", e)).await?;
                            }
                        }
                    } else {
                        println!("  ⚠ No qt_hash found");
                        // logger.log("No main data table hash").await?;
                    }

                    // Log operator settings if present
                    if let Some(settings) = &query.operator_settings {
                        println!("  Operator settings: {:?}", settings);
                        // logger.log("Operator settings detected").await?;
                    }
                } else {
                    println!("  ⚠ No query in task");
                    // logger.log("No query structure in task").await?;
                }
            }
            tercen::client::proto::e_task::Object::Runcomputationtask(rct) => {
                println!("  Task type: RunComputationTask");
                // logger.log("Processing RunComputationTask").await?;

                // Check if query exists
                if let Some(query) = &rct.query {
                    println!("  Query found");
                    // logger.log("Query structure detected").await?;

                    // Extract table hashes from CubeQuery
                    let qt_hash = &query.qt_hash;
                    let column_hash = &query.column_hash;
                    let row_hash = &query.row_hash;

                    println!("  Table hashes:");
                    println!("    qt_hash (main data): {}", qt_hash);
                    println!("    column_hash: {}", column_hash);
                    println!("    row_hash: {}", row_hash);

                    // logger.log(format!("Main data table: {}", qt_hash)).await?;
                    // logger.log(format!("Column facet table: {}", column_hash)).await?;
                    // logger.log(format!("Row facet table: {}", row_hash)).await?;

                    // Query the main data table
                    if !qt_hash.is_empty() {
                        println!("\n  Querying main data table...");
                        // logger.log("Starting data query").await?;

                        match query_and_log_data(client, qt_hash, _logger, config).await {
                            Ok(()) => {
                                println!("  ✓ Data query completed successfully");
                                // logger.log("Data query completed").await?;
                            }
                            Err(e) => {
                                eprintln!("  ✗ Data query failed: {}", e);
                                // logger.log(format!("Data query failed: {}", e)).await?;
                            }
                        }
                    } else {
                        println!("  ⚠ No qt_hash found");
                        // logger.log("No main data table hash").await?;
                    }

                    // Log operator settings if present
                    if let Some(settings) = &query.operator_settings {
                        println!("  Operator settings: {:?}", settings);
                        // logger.log("Operator settings detected").await?;
                    }
                } else {
                    println!("  ⚠ No query in task");
                    // logger.log("No query structure in task").await?;
                }
            }
            tercen::client::proto::e_task::Object::Cubequerytask(cqt) => {
                println!("  Task type: CubeQueryTask");
                // logger.log("Processing CubeQueryTask").await?;

                // CubeQueryTask also has a query field
                if let Some(query) = &cqt.query {
                    println!("  Query found");

                    let qt_hash = &query.qt_hash;
                    let column_hash = &query.column_hash;
                    let row_hash = &query.row_hash;

                    println!("  Table hashes:");
                    println!("    qt_hash (main data): {}", qt_hash);
                    println!("    column_hash: {}", column_hash);
                    println!("    row_hash: {}", row_hash);

                    if !qt_hash.is_empty() {
                        println!("\n  Querying main data table...");

                        match query_and_log_data(client, qt_hash, _logger, config).await {
                            Ok(()) => {
                                println!("  ✓ Data query completed successfully");
                            }
                            Err(e) => {
                                eprintln!("  ✗ Data query failed: {}", e);
                            }
                        }
                    } else {
                        println!("  ⚠ No qt_hash found");
                    }

                    if let Some(settings) = &query.operator_settings {
                        println!("  Operator settings: {:?}", settings);
                    }
                } else {
                    println!("  ⚠ No query in task");
                }
            }
            other_variant => {
                // Debug: Print the actual variant name
                println!("  Task type: Other (variant: {:?})", other_variant);
                // logger.log("Non-computation task type").await?;
            }
        }
    } else {
        println!("  ⚠ No task object found");
        // logger.log("Empty task object").await?;
    }

    Ok(())
}

/// Query data from a table and log information about each chunk
async fn query_and_log_data(
    client: &tercen::TercenClient,
    table_id: &str,
    _logger: &tercen::TercenLogger<'_>,
    config: &config::OperatorConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use tercen::TableStreamer;

    let streamer = TableStreamer::new(client);

    // Configuration from operator_config.json
    let chunk_size = config.chunk_size as i64;
    let max_chunks = config.max_chunks;

    let mut chunk_num = 0;
    let mut total_rows = 0;
    let mut offset = 0;

    println!("  Fetching data in chunks of {} rows...", chunk_size);
    // logger.log(format!("Chunk size: {} rows", chunk_size)).await?;

    loop {
        chunk_num += 1;

        if chunk_num > max_chunks {
            println!("  ⚠ Safety limit reached ({} chunks)", max_chunks);
            // logger.log(format!("Safety limit reached: {} chunks", max_chunks)).await?;
            break;
        }

        println!("\n  --- Chunk {} ---", chunk_num);
        // logger.log(format!("Fetching chunk {}", chunk_num)).await?;

        // Stream this chunk
        let tson_data = streamer
            .stream_tson(table_id, None, offset, chunk_size)
            .await?;

        if tson_data.is_empty() {
            println!("  No more data (empty chunk)");
            // logger.log("End of data reached").await?;
            break;
        }

        // Parse the TSON data
        let df = tson_to_dataframe(&tson_data)?;
        let row_count = df.nrow();

        total_rows += row_count;

        println!("  Rows in chunk: {}", row_count);
        // logger.log(format!("Chunk {}: {} rows", chunk_num, row_count)).await?;

        // Log first entry of this chunk
        // TODO: Re-implement sample row display for Arrow format
        /* Commented out old ParsedData code
        if row_count > 0 {
            // Sample first row logging
        }
        */

        // Check if this was the last chunk (fewer rows than requested)
        if row_count < chunk_size as usize {
            println!("  Last chunk (fewer than {} rows)", chunk_size);
            // logger.log("Last chunk detected").await?;
            break;
        }

        offset += chunk_size;
    }

    println!("\n  === Summary ===");
    println!("  Total chunks processed: {}", chunk_num);
    println!("  Total rows: {}", total_rows);

    // logger.log(format!(
    //     "Query complete: {} chunks, {} total rows",
    //     chunk_num, total_rows
    // )).await?;

    Ok(())
}
