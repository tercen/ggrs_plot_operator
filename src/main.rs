// Module declarations
// These modules are organized to make future library extraction easy:
// - `tercen`: All Tercen gRPC client code (will become `tercen-rust` crate)
// - `ggrs_integration`: GGRS-specific integration code
mod ggrs_integration;
mod tercen;

#[cfg(feature = "jemalloc")]
use tikv_jemallocator::Jemalloc;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() {
    println!("GGRS Plot Operator v{}", env!("CARGO_PKG_VERSION"));
    println!("Phase 4+: Data Query & Logging Test");

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
                // Create logger for this task
                let logger = tercen::TercenLogger::new(&client, task_id.clone());

                // Send startup log to Tercen
                if let Err(e) = logger.log("Operator started - Phase 4").await {
                    eprintln!("Warning: Failed to send log to Tercen: {}", e);
                } else {
                    println!("✓ Log sent to Tercen");
                }

                println!("\nFetching task information...");
                if let Err(e) = logger.log("Fetching task information").await {
                    eprintln!("Warning: Failed to send log: {}", e);
                }

                match process_task(&client, &task_id, &logger).await {
                    Ok(()) => {
                        println!("✓ Task processed successfully!");
                        if let Err(e) = logger.log("Task completed successfully").await {
                            eprintln!("Warning: Failed to send log: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ Task processing failed: {}", e);
                        if let Err(log_err) =
                            logger.log(format!("Task processing failed: {}", e)).await
                        {
                            eprintln!("Warning: Failed to send error log: {}", log_err);
                        }
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
    logger: &tercen::TercenLogger<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tercen::client::proto::GetRequest;

    logger.log("Start task processing").await?;
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
    logger.log("Task retrieved successfully").await?;

    // Step 2: Extract table IDs from task (if available)
    // Note: In Phase 4, we demonstrate the structure
    // In Phase 5-6, we'll actually fetch and parse real data
    if let Some(obj) = &task.object {
        match obj {
            tercen::client::proto::e_task::Object::Computationtask(ct) => {
                println!("  Task type: ComputationTask");
                logger.log("Processing ComputationTask").await?;

                // Check if query exists
                if let Some(query) = &ct.query {
                    println!("  Query found");
                    logger.log("Query structure detected").await?;

                    // Extract table hashes from CubeQuery
                    let qt_hash = &query.qt_hash;
                    let column_hash = &query.column_hash;
                    let row_hash = &query.row_hash;

                    println!("  Table hashes:");
                    println!("    qt_hash (main data): {}", qt_hash);
                    println!("    column_hash: {}", column_hash);
                    println!("    row_hash: {}", row_hash);

                    logger.log(format!("Main data table: {}", qt_hash)).await?;
                    logger
                        .log(format!("Column facet table: {}", column_hash))
                        .await?;
                    logger.log(format!("Row facet table: {}", row_hash)).await?;

                    // Query the main data table
                    if !qt_hash.is_empty() {
                        println!("\n  Querying main data table...");
                        logger.log("Starting data query").await?;

                        match query_and_log_data(client, qt_hash, logger).await {
                            Ok(()) => {
                                println!("  ✓ Data query completed successfully");
                                logger.log("Data query completed").await?;
                            }
                            Err(e) => {
                                eprintln!("  ✗ Data query failed: {}", e);
                                logger.log(format!("Data query failed: {}", e)).await?;
                            }
                        }
                    } else {
                        println!("  ⚠ No qt_hash found");
                        logger.log("No main data table hash").await?;
                    }

                    // Log operator settings if present
                    if let Some(settings) = &query.operator_settings {
                        println!("  Operator settings: {:?}", settings);
                        logger.log("Operator settings detected").await?;
                    }
                } else {
                    println!("  ⚠ No query in task");
                    logger.log("No query structure in task").await?;
                }
            }
            _ => {
                println!("  Task type: Other");
                logger.log("Non-computation task type").await?;
            }
        }
    } else {
        println!("  ⚠ No task object found");
        logger.log("Empty task object").await?;
    }

    Ok(())
}

/// Query data from a table and log information about each chunk
async fn query_and_log_data(
    client: &tercen::TercenClient,
    table_id: &str,
    logger: &tercen::TercenLogger<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tercen::{ParsedData, TableStreamer};

    let streamer = TableStreamer::new(client);

    // Configuration
    let chunk_size = 10_000; // 10K rows per chunk
    let max_chunks = 10; // Safety limit

    let mut chunk_num = 0;
    let mut total_rows = 0;
    let mut offset = 0;

    println!("  Fetching data in chunks of {} rows...", chunk_size);
    logger
        .log(format!("Chunk size: {} rows", chunk_size))
        .await?;

    loop {
        chunk_num += 1;

        if chunk_num > max_chunks {
            println!("  ⚠ Safety limit reached ({} chunks)", max_chunks);
            logger
                .log(format!("Safety limit reached: {} chunks", max_chunks))
                .await?;
            break;
        }

        println!("\n  --- Chunk {} ---", chunk_num);
        logger.log(format!("Fetching chunk {}", chunk_num)).await?;

        // Stream this chunk
        let csv_data = streamer
            .stream_csv(table_id, None, offset, chunk_size)
            .await?;

        if csv_data.is_empty() {
            println!("  No more data (empty chunk)");
            logger.log("End of data reached").await?;
            break;
        }

        // Parse the CSV data
        let parsed = ParsedData::from_csv(&csv_data)?;
        let row_count = parsed.rows.len();

        total_rows += row_count;

        println!("  Rows in chunk: {}", row_count);
        logger
            .log(format!("Chunk {}: {} rows", chunk_num, row_count))
            .await?;

        // Log first entry of this chunk
        if let Some(first_row) = parsed.rows.first() {
            println!("  First entry:");
            println!("    .ci = {:?}", first_row.ci);
            println!("    .ri = {:?}", first_row.ri);
            println!("    .x  = {:?}", first_row.x);
            println!("    .y  = {:?}", first_row.y);

            // Log extra fields if any
            if !first_row.extra.is_empty() {
                println!("    Extra fields: {}", first_row.extra.len());
                for (key, value) in first_row.extra.iter().take(3) {
                    println!("      {} = {}", key, value);
                }
            }

            logger
                .log(format!(
                    "First entry: ci={:?}, ri={:?}, x={:?}, y={:?}",
                    first_row.ci, first_row.ri, first_row.x, first_row.y
                ))
                .await?;
        }

        // Get summary statistics for this chunk
        let summary = parsed.summary();
        println!("  {}", summary);
        logger.log(format!("Chunk summary: {}", summary)).await?;

        // Check if this was the last chunk (fewer rows than requested)
        if row_count < chunk_size as usize {
            println!("  Last chunk (fewer than {} rows)", chunk_size);
            logger.log("Last chunk detected").await?;
            break;
        }

        offset += chunk_size;
    }

    println!("\n  === Summary ===");
    println!("  Total chunks processed: {}", chunk_num);
    println!("  Total rows: {}", total_rows);

    logger
        .log(format!(
            "Query complete: {} chunks, {} total rows",
            chunk_num, total_rows
        ))
        .await?;

    Ok(())
}
