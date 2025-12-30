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
    println!("Phase 1: Basic structure");

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

    // Simulate operator processing time
    println!("Processing...");
    std::thread::sleep(std::time::Duration::from_secs(2));

    println!("Operator completed successfully!");
    std::process::exit(0);
}
