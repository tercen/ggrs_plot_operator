#!/bin/bash
# Test script for TercenStreamGenerator with memory tracking
#
# Usage:
#   ./test_local.sh [backend]
#
# Example:
#   ./test_local.sh          # Test with CPU backend (default)
#   ./test_local.sh cpu      # Test with CPU backend
#   ./test_local.sh gpu      # Test with GPU backend

set -e

# Configuration
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJodHRwOi8vMTI3LjAuMC4xOjU0MDAiLCJleHAiOjE3NzA0MTYzMjMsImRhdGEiOnsiZCI6IiIsInUiOiJ0ZXN0IiwiZSI6MTc3MDQxNjMyMzQyOX19.oc5mv3ZxGIJs3m1yWpKPXC2m6cf3VNC8ezeD6IQ-q3o"
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="b9659735-27db-4480-b398-4e391431480f"

# Memory tracker path
MEMORY_TRACKER="/home/thiago/workspaces/tercen/main/memory_tracker/target/release/memory_tracker"

# Get backend from argument or use default
BACKEND="gpu"

# Validate backend
if [[ "$BACKEND" != "cpu" && "$BACKEND" != "gpu" ]]; then
    echo "ERROR: Invalid backend '$BACKEND'. Use 'cpu' or 'gpu'"
    exit 1
fi

# Fixed chunk size (not configurable from command line anymore)
CHUNK_SIZE=15000

# Update operator_config.json with backend setting
echo "Setting backend to $BACKEND in operator_config.json..."
cat > operator_config.json <<EOF
{
  "chunk_size": $CHUNK_SIZE,
  "max_chunks": 100000,
  "cache_axis_ranges": true,
  "default_plot_width": 800,
  "default_plot_height": 600,
  "render_backend": "$BACKEND"
}
EOF

echo ""
echo "============================================"
echo "Running test with backend=$BACKEND"
echo "============================================"
echo ""

# Clean old binaries and plot to ensure fresh build
echo "Cleaning old binaries and plot..."
rm -f target/debug/test_stream_generator_v2 plot.png
echo ""

# Build first to avoid compilation time in measurements
echo "Building test binary..."
cargo build --bin test_stream_generator_v2 2>&1 | tail -10
BUILD_EXIT=$?
if [ $BUILD_EXIT -ne 0 ]; then
    echo ""
    echo "ERROR: Build failed with exit code $BUILD_EXIT"
    echo "Showing full build output:"
    cargo build --bin test_stream_generator_v2 2>&1
    exit $BUILD_EXIT
fi
echo ""

# Start memory tracker in background, waiting for the process
MEMORY_OUTPUT="memory_usage_backend_${BACKEND}.png"
CSV_OUTPUT="memory_usage_backend_${BACKEND}.csv"

# Start the test process and immediately get its PID
echo "Starting test process and memory tracker..."
./target/debug/test_stream_generator_v2 2>&1 &
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
