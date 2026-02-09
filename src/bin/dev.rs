//! GGRS Plot Operator - Development entry point
//!
//! This binary is for local testing with workflow/step IDs (like Python's OperatorContextDev).
//! It generates plots and saves them to local files instead of uploading to Tercen.
//!
//! Usage:
//! ```bash
//! export TERCEN_URI=http://127.0.0.1:50051
//! export TERCEN_TOKEN=your_token_here
//! export WORKFLOW_ID=your_workflow_id
//! export STEP_ID=your_step_id
//! cargo run --bin dev
//! ```

use ggrs_plot_operator::config::OperatorConfig;
use ggrs_plot_operator::memprof;
use ggrs_plot_operator::pipeline;
use ggrs_plot_operator::tercen::{DevContext, TercenClient, TercenContext};
use std::sync::Arc;
use std::time::Instant;

fn log_phase(start: Instant, phase: &str) {
    let elapsed = start.elapsed();
    eprintln!("[PHASE @{:.3}s] {}", elapsed.as_secs_f64(), phase);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let m0 = memprof::checkpoint_return("main() START");
    let t0 = memprof::time_start("main() START");

    log_phase(start, "START: Development test");
    println!("=== GGRS Plot Operator - Development Mode ===\n");

    // Read environment variables
    let uri = std::env::var("TERCEN_URI").unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());
    let token =
        std::env::var("TERCEN_TOKEN").expect("TERCEN_TOKEN environment variable is required");
    let workflow_id =
        std::env::var("WORKFLOW_ID").expect("WORKFLOW_ID environment variable is required");
    let step_id = std::env::var("STEP_ID").expect("STEP_ID environment variable is required");

    println!("Configuration:");
    println!("  URI: {}", uri);
    println!("  Token: {}...", &token[..10.min(token.len())]);
    println!("  Workflow ID: {}", workflow_id);
    println!("  Step ID: {}", step_id);
    println!();

    // Connect to Tercen
    log_phase(start, "PHASE 1: Connecting to Tercen");
    println!("Connecting to Tercen...");
    std::env::set_var("TERCEN_URI", &uri);
    std::env::set_var("TERCEN_TOKEN", &token);

    let client = TercenClient::from_env().await?;
    let client_arc = Arc::new(client);
    println!("✓ Connected successfully\n");
    let m1 = memprof::delta("After TercenClient::from_env()", m0);
    let t1 = memprof::time_delta("After TercenClient::from_env()", t0, t0);

    // Create DevContext
    log_phase(start, "PHASE 2: Creating DevContext");
    println!("Creating DevContext from workflow/step...");
    let ctx = DevContext::from_workflow_step(client_arc.clone(), &workflow_id, &step_id).await?;
    println!("✓ Context created\n");
    let _ = memprof::delta("After DevContext::from_workflow_step()", m1);
    let _ = memprof::time_delta("After DevContext::from_workflow_step()", t0, t1);

    // Load configuration
    let config = load_dev_config(ctx.point_size())?;
    println!("Configuration loaded:");
    println!("  Chunk size: {}", config.chunk_size);
    println!(
        "  Point size: {} (from crosstab: {:?})",
        config.point_size,
        ctx.point_size()
    );
    println!("  Backend: {}", config.backend);
    println!("  PNG compression: {}", config.png_compression);
    println!();

    // Generate plots using shared pipeline
    log_phase(start, "PHASE 3: Generating plots");
    let plot_results = pipeline::generate_plots(&ctx, &config).await?;

    // Save results to local files
    log_phase(start, "PHASE 4: Saving to local files");
    println!("\nSaving {} plot(s) to local files...", plot_results.len());

    for (i, plot) in plot_results.iter().enumerate() {
        let filename = if plot_results.len() > 1 {
            format!("{}_{}.{}", plot.filename, i + 1, plot.output_ext)
        } else {
            format!("{}.{}", plot.filename, plot.output_ext)
        };

        std::fs::write(&filename, &plot.png_buffer)?;
        println!(
            "✓ Saved {} ({} bytes, {}×{})",
            filename,
            plot.png_buffer.len(),
            plot.width,
            plot.height
        );
    }

    log_phase(start, "COMPLETE");
    println!("\n=== Development Test Complete ===");
    println!("All checks passed!");

    Ok(())
}

/// Load configuration from operator_config.json if it exists
fn load_dev_config(
    ui_point_size: Option<i32>,
) -> Result<OperatorConfig, Box<dyn std::error::Error>> {
    use ggrs_plot_operator::tercen::client::proto::{OperatorRef, OperatorSettings, PropertyValue};
    use std::fs;

    let config_path = "operator_config.json";
    let config_json = match fs::read_to_string(config_path) {
        Ok(json) => json,
        Err(_) => {
            println!("  No operator_config.json found, using defaults");
            return Ok(OperatorConfig::from_properties(None, ui_point_size)?);
        }
    };

    let config_map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&config_json)
        .map_err(|e| format!("Failed to parse operator_config.json: {}", e))?;

    // Convert JSON map to PropertyValue list
    let mut property_values = Vec::new();
    for (key, value) in config_map {
        let value_str = match value {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => continue,
        };

        property_values.push(PropertyValue {
            name: key,
            value: value_str,
        });
    }

    let operator_settings = OperatorSettings {
        operator_ref: Some(OperatorRef {
            operator_id: "dev".to_string(),
            name: String::new(),
            operator_kind: String::new(),
            operator_spec: None,
            version: String::new(),
            url: None,
            property_values,
        }),
        namespace: String::new(),
        environment: Vec::new(),
        operator_model: None,
    };

    println!("  Loaded configuration from operator_config.json");
    Ok(OperatorConfig::from_properties(
        Some(&operator_settings),
        ui_point_size,
    )?)
}
