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
    println!("Phase 2: gRPC Connection");

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
                println!("\nFetching task information...");
                match get_task_info(&client, &task_id).await {
                    Ok(state) => {
                        println!("✓ Successfully fetched task: {}", task_id);
                        println!("  Task state: {}", state);
                    }
                    Err(e) => {
                        eprintln!("✗ Failed to fetch task: {}", e);
                    }
                }
            } else {
                println!("\nNo TERCEN_TASK_ID set, skipping task fetch");
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

    // Simulate operator processing time
    println!("\nProcessing...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    println!("Operator completed!");
    std::process::exit(0);
}

async fn get_task_info(
    client: &tercen::TercenClient,
    task_id: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use tercen::client::proto::GetRequest;

    let mut task_service = client.task_service()?;
    let request = tonic::Request::new(GetRequest {
        id: task_id.to_string(),
        ..Default::default()
    });

    let response = task_service.get(request).await?;
    let _task = response.into_inner();

    // Task object is a oneof enum - just return a success message
    let state = format!("Task retrieved successfully (ID: {})", task_id);

    Ok(state)
}
