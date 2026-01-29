//! GGRS Plot Operator - Production entry point
//!
//! This operator receives tabular data from Tercen via gRPC, generates plots using GGRS,
//! and returns PNG images back to Tercen for visualization.
//!
//! For local development/testing, use the `dev` binary instead:
//! ```bash
//! cargo run --bin dev
//! ```

pub mod config;
pub mod ggrs_integration;
pub mod memprof;
pub mod pipeline;
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

    // Parse command-line arguments (Tercen passes --taskId, --serviceUri, --token)
    let args: Vec<String> = std::env::args().collect();
    parse_args(&args);

    // Print environment info
    print_env_info();

    // Connect to Tercen
    println!("Attempting to connect to Tercen...");
    match tercen::TercenClient::from_env().await {
        Ok(client) => {
            println!("✓ Successfully connected to Tercen!\n");

            let client_arc = std::sync::Arc::new(client);

            // Process task if TERCEN_TASK_ID is set
            if let Ok(task_id) = std::env::var("TERCEN_TASK_ID") {
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
    use tercen::TercenContext;

    println!("=== Task Processing Started ===");
    println!("Task ID: {}\n", task_id);

    // Create ProductionContext
    let ctx = tercen::ProductionContext::from_task_id(client_arc.clone(), task_id).await?;

    // Load configuration
    let config = config::OperatorConfig::from_properties(ctx.operator_settings(), ctx.point_size());

    // Generate plots using shared pipeline
    let plot_results = pipeline::generate_plots(&ctx, &config).await?;

    // Upload results to Tercen
    println!("\n[5/5] Uploading result(s) to Tercen...");

    let mut task_service = client_arc.task_service()?;
    let request = tonic::Request::new(tercen::client::proto::GetRequest {
        id: task_id.to_string(),
        ..Default::default()
    });
    let response = task_service.get(request).await?;
    let mut task = response.into_inner();

    if plot_results.len() == 1 {
        let plot = plot_results.into_iter().next().unwrap();
        tercen::result::save_result(
            client_arc.clone(),
            ctx.project_id(),
            ctx.namespace(),
            plot.png_buffer,
            plot.width,
            plot.height,
            &mut task,
        )
        .await?;
        println!("✓ Result uploaded and linked successfully");
    } else {
        println!("  Uploading {} plots...", plot_results.len());
        for plot in &plot_results {
            println!(
                "    - {}: {} bytes ({}×{})",
                plot.label,
                plot.png_buffer.len(),
                plot.width,
                plot.height
            );
        }

        tercen::result::save_results(
            client_arc.clone(),
            ctx.project_id(),
            ctx.namespace(),
            plot_results,
            &mut task,
        )
        .await?;
        println!("✓ All plots uploaded successfully");
    }

    println!("\n=== Task Processing Complete ===");
    Ok(())
}
