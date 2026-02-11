#!/bin/bash
# Test script for local development with memory tracking and legend positioning
# Uses the 'dev' binary which shares the same pipeline as production
#
# Usage:
#   ./test_local.sh [backend] [theme] [format]
#
# Examples:
#   ./test_local.sh                    # CPU, gray theme, PNG (default)
#   ./test_local.sh cpu gray           # Gray theme (default ggplot2 style)
#   ./test_local.sh cpu bw             # Black & white theme
#   ./test_local.sh cpu light svg      # Light theme, SVG output
#   ./test_local.sh gpu bw png         # GPU backend, bw theme, PNG
#   ./test_local.sh cpu gray svg       # SVG vector output
#
# Themes (matches ggplot2 exactly):
#   - gray:     Default ggplot2 theme with gray panel background and white grid
#   - bw:       Black and white theme with white background and light gray grid
#   - linedraw: Black lines of various widths on white backgrounds
#   - light:    Light grey lines and axes, directing attention to data
#   - dark:     Dark background (inverse of light), makes colors pop
#   - minimal:  No background, border, or ticks - just grid lines
#   - classic:  Traditional look with axis lines but no grid
#   - void:     Completely empty - only shows the data
#
# Additional configuration can be set in operator_config.json:
#   - legend.position: "left", "right", "top", "bottom", "inside", "none"
#   - legend.position.inside: "x,y" coordinates for inside positioning
#   - legend.justification: "x,y" anchor point
#   - png.compression: "fast", "default", "best"

set -e

# Configuration
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJodHRwOi8vMTI3LjAuMC4xOjU0MDAiLCJleHAiOjE3NzE0MzQzMTYsImRhdGEiOnsiZCI6IiIsInUiOiJ0ZXN0IiwiZSI6MTc3MTQzNDMxNjk2MH19.IsYnlDE8fBGlzfD776GKjFxcF35ws48MABWGctYiwFs"
# EXAMPLE1
# Heatmap Data Step (Divergent palette)
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="b9659735-27db-4480-b398-4e391431480f"

# EXAMPLE2
# Scatter simple (no X-axis table - uses sequential X range)
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="7a8eb4a9-d7bf-4fb9-8385-6f902fb73693"

# EXAMPLE3
# Scatter crabs (has X-axis table)
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="102a1b30-ee6d-42ae-9681-607ceb526b5e"


# EXAMPLE4
# No col, no row, log scale
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="07711b9a-0278-4116-b0fc-ba51b01da29a"

# EXAMPLE3
# Scatter crabs (has X-axis table)
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="102a1b30-ee6d-42ae-9681-607ceb526b5e"



#EXAMPLE5
#Bar plots
#http://127.0.0.1:5400/test/w/28e3c9888e9935f667aed6f07c007c7c/ds/63235187-2f58-48c5-a60c-9ecfc718e9f4
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="63235187-2f58-48c5-a60c-9ecfc718e9f4"

#EXAMPLE6
# Multiple layers
#http://127.0.0.1:5400/test/w/28e3c9888e9935f667aed6f07c007c7c/ds/a7a3aaea-33b9-4dfd-81f9-d86de12d6dcc
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="a7a3aaea-33b9-4dfd-81f9-d86de12d6dcc"

#EXAMPLE7
# Line plot
# http://127.0.0.1:5400/test/w/28e3c9888e9935f667aed6f07c007c7c/ds/f95a4bb6-e911-4cab-bebe-830f1202fede
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="f95a4bb6-e911-4cab-bebe-830f1202fede"

#EXAMPLE8
# SVG Scatter
# http://127.0.0.1:5400/test/w/28e3c9888e9935f667aed6f07c007c7c/ds/2eb69100-57c2-4589-8f81-ad3cb2881083
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="2eb69100-57c2-4589-8f81-ad3cb2881083"

# EXAMPLE3 - Scatter crabs (has X-axis table)
#export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
#export STEP_ID="102a1b30-ee6d-42ae-9681-607ceb526b5e"

# Memory tracker path
MEMORY_TRACKER="/home/thiago/workspaces/tercen/main/memory_tracker/target/release/memory_tracker"

# Parse arguments
BACKEND="${1:-cpu}"
THEME="${2:-gray}"
FORMAT="${3:-png}"

# Fixed values (can be customized in operator_config.json directly)
LEGEND_POSITION="right"
LEGEND_JUSTIFICATION="0.5,0.5"
PNG_COMPRESSION="fast"

# Valid values for properties (from operator.json)
VALID_BACKENDS=("cpu" "gpu")
VALID_THEMES=("gray" "bw" "linedraw" "light" "dark" "minimal" "classic" "void" "publish")
VALID_FORMATS=("png" "svg" "hsvg")

# Validate backend
if [[ ! " ${VALID_BACKENDS[@]} " =~ " ${BACKEND} " ]]; then
    echo "ERROR: Invalid backend '$BACKEND'"
    echo "Valid values: ${VALID_BACKENDS[*]}"
    exit 1
fi

# Validate theme
if [[ ! " ${VALID_THEMES[@]} " =~ " ${THEME} " ]]; then
    echo "ERROR: Invalid theme '$THEME'"
    echo "Valid values: ${VALID_THEMES[*]}"
    exit 1
fi

# Validate format
if [[ ! " ${VALID_FORMATS[@]} " =~ " ${FORMAT} " ]]; then
    echo "ERROR: Invalid format '$FORMAT'"
    echo "Valid values: ${VALID_FORMATS[*]}"
    exit 1
fi

# Fixed chunk size (not configurable from command line anymore)
CHUNK_SIZE=15000

# Update operator_config.json with backend and theme settings
echo "Creating operator_config.json..."
echo "  Backend: $BACKEND"
echo "  Theme: $THEME"
echo "  Format: $FORMAT"
echo "  Legend position: $LEGEND_POSITION"
echo "  PNG compression: $PNG_COMPRESSION"

# Build JSON
cat > operator_config.json <<EOF
{
  "chunk_size": $CHUNK_SIZE,
  "max_chunks": 100000,
  "cache_axis_ranges": true,
  "default_plot_width": 6000,
  "default_plot_height": 2000,
  "backend": "$BACKEND",
  "output.format": "$FORMAT",
  "theme": "$THEME",
  "legend.position": "$LEGEND_POSITION",
  "legend.justification": "$LEGEND_JUSTIFICATION",
  "png.compression": "$PNG_COMPRESSION",
  "plot.title": "Test Plot via Local Script",
  "plot.title.position": "top",
  "plot.title.justification": "0,1",
  "axis.x.label": "X Axis Label",
  "axis.y.label": "Y Axis Label",
  "point.shapes": "19;17;15",
  "point.opacity": "0.5"
}
EOF

echo ""
echo "============================================"
echo "Running test with backend=$BACKEND format=$FORMAT"
echo "============================================"
echo ""

# Clean old binaries and plots to ensure fresh build
echo "Cleaning old binaries and plots..."
rm -f target/debug/dev plot.png plot_*.png plot.svg plot_*.svg
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
