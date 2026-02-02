#!/bin/bash
# Test script for local development with memory tracking and legend positioning
# Uses the 'dev' binary which shares the same pipeline as production
#
# Usage:
#   ./test_local.sh [backend] [legend_position] [legend_position_inside] [legend_justification] [png_compression]
#
# Examples:
#   ./test_local.sh                              # CPU, legend right, default compression
#   ./test_local.sh cpu left                     # Legend on left (default center)
#   ./test_local.sh cpu left "" "0,0"            # Legend on left, bottom-left corner
#   ./test_local.sh cpu left "" "0,1"            # Legend on left, top-left corner
#   ./test_local.sh cpu top "" "0.5,1"           # Legend on top, centered
#   ./test_local.sh cpu inside 0.95,0.05         # Legend inside at bottom-right
#   ./test_local.sh cpu inside 0.95,0.05 1,0     # Inside bottom-right, legend's bottom-right corner anchored
#   ./test_local.sh cpu none                     # No legend
#   ./test_local.sh cpu right "" "" fast         # Fast PNG compression (~30% speedup, +15% file size)
#   ./test_local.sh cpu right "" "" best         # Best PNG compression (~40% slower, -10% file size)
#
# Property explanation (matches ggplot2 3.5.0):
#   - legend.position: "left", "right", "top", "bottom", "inside", "none"
#   - legend.position.inside: "x,y" coordinates for inside positioning
#   - legend.justification: "x,y" anchor point
#     - For left/right: y-value controls vertical (0=bottom, 0.5=center, 1=top)
#     - For top/bottom: x-value controls horizontal (0=left, 0.5=center, 1=right)
#     - For inside: which corner of legend aligns with position.inside coords
#   - png.compression: "fast", "default", "best"

set -e

# Configuration
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJodHRwOi8vMTI3LjAuMC4xOjU0MDAiLCJleHAiOjE3NzE0MzQzMTYsImRhdGEiOnsiZCI6IiIsInUiOiJ0ZXN0IiwiZSI6MTc3MTQzNDMxNjk2MH19.IsYnlDE8fBGlzfD776GKjFxcF35ws48MABWGctYiwFs"
# EXAMPLE1
# Heatmap Data Step
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="b9659735-27db-4480-b398-4e391431480f"

# EXAMPLE2
# Scatter simple (no X-axis table - uses sequential X range)
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="7a8eb4a9-d7bf-4fb9-8385-6f902fb73693"

# EXAMPLE3
# Scatter crabs (has X-axis table)
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="102a1b30-ee6d-42ae-9681-607ceb526b5e"

# Memory tracker path
MEMORY_TRACKER="/home/thiago/workspaces/tercen/main/memory_tracker/target/release/memory_tracker"

# Parse arguments
BACKEND="${1:-cpu}"
LEGEND_POSITION="right" # position test # position test # position test # position test
LEGEND_POSITION_INSIDE="${3:-}"
LEGEND_JUSTIFICATION="0.5,0.5"
PNG_COMPRESSION="fast"

# Valid values for properties (from operator.json)
VALID_BACKENDS=("cpu" "gpu")
VALID_LEGEND_POSITIONS=("left" "right" "top" "bottom" "inside" "none")
VALID_PNG_COMPRESSION=("fast" "default" "best")

# Validate backend
if [[ ! " ${VALID_BACKENDS[@]} " =~ " ${BACKEND} " ]]; then
    echo "ERROR: Invalid backend '$BACKEND'"
    echo "Valid values: ${VALID_BACKENDS[*]}"
    exit 1
fi

# Validate legend position
if [[ ! " ${VALID_LEGEND_POSITIONS[@]} " =~ " ${LEGEND_POSITION} " ]]; then
    echo "ERROR: Invalid legend position '$LEGEND_POSITION'"
    echo "Valid values: ${VALID_LEGEND_POSITIONS[*]}"
    exit 1
fi

# Validate legend.position.inside (if provided)
if [[ -n "$LEGEND_POSITION_INSIDE" ]]; then
    # Check format: x,y where x,y are numbers in [0,1]
    if ! [[ "$LEGEND_POSITION_INSIDE" =~ ^[0-9.]+,[0-9.]+$ ]]; then
        echo "ERROR: Invalid legend.position.inside format '$LEGEND_POSITION_INSIDE'"
        echo "Expected format: 'x,y' where x,y are numbers (e.g., '0.95,0.05')"
        exit 1
    fi
fi

# Validate legend.justification (if provided)
if [[ -n "$LEGEND_JUSTIFICATION" ]]; then
    # Check format: x,y where x,y are numbers in [0,1]
    if ! [[ "$LEGEND_JUSTIFICATION" =~ ^[0-9.]+,[0-9.]+$ ]]; then
        echo "ERROR: Invalid legend.justification format '$LEGEND_JUSTIFICATION'"
        echo "Expected format: 'x,y' where x,y are numbers (e.g., '0.5,0.5')"
        exit 1
    fi
fi

# Validate PNG compression
if [[ ! " ${VALID_PNG_COMPRESSION[@]} " =~ " ${PNG_COMPRESSION} " ]]; then
    echo "ERROR: Invalid png.compression '$PNG_COMPRESSION'"
    echo "Valid values: ${VALID_PNG_COMPRESSION[*]}"
    exit 1
fi

# Fixed chunk size (not configurable from command line anymore)
CHUNK_SIZE=15000

# Update operator_config.json with backend and legend settings
echo "Creating operator_config.json..."
echo "  Backend: $BACKEND"
echo "  Legend position: $LEGEND_POSITION"
echo "  PNG compression: $PNG_COMPRESSION"
[[ -n "$LEGEND_POSITION_INSIDE" ]] && echo "  Legend position inside: $LEGEND_POSITION_INSIDE"
[[ -n "$LEGEND_JUSTIFICATION" ]] && echo "  Legend justification: $LEGEND_JUSTIFICATION"

# Build JSON dynamically based on what's provided
cat > operator_config.json <<EOF
{
  "chunk_size": $CHUNK_SIZE,
  "max_chunks": 100000,
  "cache_axis_ranges": true,
  "default_plot_width": 6000,
  "default_plot_height": 2000,
  "render_backend": "$BACKEND",
  "legend.position": "$LEGEND_POSITION"$(
    [[ -n "$LEGEND_POSITION_INSIDE" ]] && echo ",
  \"legend.position.inside\": \"$LEGEND_POSITION_INSIDE\""
  )$(
    [[ -n "$LEGEND_JUSTIFICATION" ]] && echo ",
  \"legend.justification\": \"$LEGEND_JUSTIFICATION\""
  ),
  "png.compression": "$PNG_COMPRESSION",
  "plot.title": "Test Plot via Local Script",
  "plot.title.position": "top",
  "plot.title.justification": "0,1",
  "axis.x.label": "X Axis Label",
  "axis.y.label": "Y Axis Label"
}
EOF

echo ""
echo "============================================"
echo "Running test with backend=$BACKEND"
echo "============================================"
echo ""

# Clean old binaries and plots to ensure fresh build
echo "Cleaning old binaries and plots..."
rm -f target/debug/dev plot.png plot_*.png
echo ""

# Build first to avoid compilation time in measurements
echo "Building dev binary..."
cargo build --bin dev 2>&1 | tail -10
BUILD_EXIT=$?
if [ $BUILD_EXIT -ne 0 ]; then
    echo ""
    echo "ERROR: Build failed with exit code $BUILD_EXIT"
    echo "Showing full build output:"
    cargo build --bin dev 2>&1
    exit $BUILD_EXIT
fi
echo ""

# Start memory tracker in background, waiting for the process
MEMORY_OUTPUT="memory_usage_backend_${BACKEND}.png"
CSV_OUTPUT="memory_usage_backend_${BACKEND}.csv"

# Start the dev process and immediately get its PID
echo "Starting dev process and memory tracker..."
./target/debug/dev 2>&1 &
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
