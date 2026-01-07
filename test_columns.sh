#!/bin/bash
# Test different column combinations to understand TSON parsing

export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJodHRwOi8vMTI3LjAuMC4xOjU0MDAiLCJleHAiOjE3NzAzOTUwMzQsImRhdGEiOnsiZCI6IiIsInUiOiJ0ZXN0IiwiZSI6MTc3MDM5NTAzNDc2NH19.uPBcoKYyWpqqlCsywzWsF3p-Ho9Q6SHSV_thj47m0ig"
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="b9659735-27db-4480-b398-4e391431480f"

# We'll create a simple Rust program to test different column combinations
cat > /tmp/test_cols.rs << 'RUST_CODE'
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get environment variables
    let uri = env::var("TERCEN_URI")?;
    let token = env::var("TERCEN_TOKEN")?;
    let workflow_id = env::var("WORKFLOW_ID")?;
    let step_id = env::var("STEP_ID")?;

    println!("Testing different column combinations...\n");

    // Test combinations
    let tests = vec![
        vec![".xs"],
        vec![".ys"],
        vec![".xs", ".ys"],
        vec![".y"],
        vec![".ci"],
        vec![".ri"],
        vec![".ci", ".ri"],
        vec![".xs", ".ys", ".ci"],
        vec![".xs", ".ys", ".ri"],
        vec![".ci", ".ri", ".xs", ".ys"],
    ];

    for cols in tests {
        println!("Testing columns: {:?}", cols);
        // We'd call streamTable here with these columns
        // For now, just print what we'd test
    }

    Ok(())
}
RUST_CODE

echo "Column combination tests to investigate:"
echo "1. .xs alone"
echo "2. .ys alone"
echo "3. .xs + .ys together"
echo "4. .xs + .ys + .ci"
echo "5. .xs + .ys + .ri"
echo "6. All four: .ci + .ri + .xs + .ys"
