#!/bin/bash
# Test script for TercenStreamGenerator with memory tracking
#
# Usage:
#   ./test_local.sh [chunk_size]
#
# Example:
#   ./test_local.sh 1000    # Test with 1000-row chunks
#   ./test_local.sh 10000   # Test with 10K-row chunks (default)

set -e

# Configuration
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJodHRwOi8vMTI3LjAuMC4xOjU0MDAiLCJleHAiOjE3NzAzODg4NzMsImRhdGEiOnsiZCI6IiIsInUiOiJ0ZXN0IiwiZSI6MTc3MDM4ODg3Mzk3OX19.ol2dYJM9KgM5it_m-ijbQwaOFPiaFUoE2t6H7W4YLVs"
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="b9659735-27db-4480-b398-4e391431480f"

# Memory tracker path
MEMORY_TRACKER="/home/thiago/workspaces/tercen/main/memory_tracker/target/release/memory_tracker"

# Get chunk size from argument or use default
CHUNK_SIZE=15000

# Update operator_config.json with requested chunk size
echo "Setting chunk_size to $CHUNK_SIZE in operator_config.json..."
cat > operator_config.json <<EOF
{
  "chunk_size": $CHUNK_SIZE,
  "max_chunks": 100000,
  "cache_axis_ranges": true,
  "default_plot_width": 800,
  "default_plot_height": 600
}
EOF

echo ""
echo "============================================"
echo "Running test with chunk_size=$CHUNK_SIZE"
echo "============================================"
echo ""

# Build first to avoid compilation time in measurements
echo "Building test binary..."
cargo build --bin test_stream_generator 2>&1 | tail -3
echo ""

# Start memory tracker in background, waiting for the process
MEMORY_OUTPUT="memory_usage_chunk_${CHUNK_SIZE}.png"
CSV_OUTPUT="memory_usage_chunk_${CHUNK_SIZE}.csv"

# Start the test process and immediately get its PID
echo "Starting test process and memory tracker..."
./target/debug/test_stream_generator 2>&1 &
TEST_PID=$!

# Start memory tracker immediately
$MEMORY_TRACKER \
    --pid $TEST_PID \
    --interval 5 \
    --output "$MEMORY_OUTPUT" \
    --csv-output "$CSV_OUTPUT" 2>&1 &
TRACKER_PID=$!

# Wait for test to complete
wait $TEST_PID
TEST_EXIT=$?

# Give tracker a moment to finish writing
sleep 0.5

# Stop memory tracker gracefully
kill -TERM $TRACKER_PID 2>/dev/null || true
wait $TRACKER_PID 2>/dev/null || true

echo ""
echo "============================================"
echo "Test completed with exit code: $TEST_EXIT"
echo "Memory chart saved to: $MEMORY_OUTPUT"
echo "Memory data saved to: $CSV_OUTPUT"
echo "============================================"

exit $TEST_EXIT
