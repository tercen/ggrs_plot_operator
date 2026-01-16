#!/bin/bash
# Test script for different legend positions
#
# Tests all legend position configurations:
# - Named positions: left, right, top, bottom, none
# - Inside position: custom coordinates
#
# Usage:
#   ./test_legend_positions.sh [position]
#
# Example:
#   ./test_legend_positions.sh right      # Test right position (default)
#   ./test_legend_positions.sh left       # Test left position
#   ./test_legend_positions.sh top        # Test top position
#   ./test_legend_positions.sh bottom     # Test bottom position
#   ./test_legend_positions.sh inside     # Test inside position (top-right)
#   ./test_legend_positions.sh none       # Test no legend
#   ./test_legend_positions.sh all        # Test all positions

set -e

# Configuration
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJodHRwOi8vMTI3LjAuMC4xOjU0MDAiLCJleHAiOjE3NzExNTYwMjksImRhdGEiOnsiZCI6IiIsInUiOiJ0ZXN0IiwiZSI6MTc3MTE1NjAyOTEzMH19.MKGl8pfmw8bkqiJ4_msNpGBpabIHtVfZ2-4tYNEc93c"
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="b9659735-27db-4480-b398-4e391431480f"

POSITION="${1:-right}"

# Function to create operator_config.json with legend position
create_config() {
    local legend_pos="$1"
    local legend_inside="$2"

    echo "Creating operator_config.json with legend.position='$legend_pos'"

    if [ -n "$legend_inside" ]; then
        echo "  legend.position.inside='$legend_inside'"
        cat > operator_config.json <<EOF
{
  "chunk_size": 15000,
  "max_chunks": 100000,
  "cache_axis_ranges": true,
  "default_plot_width": 6000,
  "default_plot_height": 2000,
  "render_backend": "cpu",
  "legend.position": "$legend_pos",
  "legend.position.inside": "$legend_inside"
}
EOF
    else
        cat > operator_config.json <<EOF
{
  "chunk_size": 15000,
  "max_chunks": 100000,
  "cache_axis_ranges": true,
  "default_plot_width": 6000,
  "default_plot_height": 2000,
  "render_backend": "cpu",
  "legend.position": "$legend_pos"
}
EOF
    fi
}

# Function to run test
run_test() {
    local legend_pos="$1"
    local legend_inside="$2"
    local output_file="plot_legend_${legend_pos}.png"

    if [ "$legend_pos" = "inside" ]; then
        output_file="plot_legend_inside_${legend_inside/,/_}.png"
    fi

    echo ""
    echo "============================================"
    echo "Testing legend position: $legend_pos"
    if [ -n "$legend_inside" ]; then
        echo "Inside coordinates: $legend_inside"
    fi
    echo "============================================"
    echo ""

    # Create config
    create_config "$legend_pos" "$legend_inside"

    # Build
    echo "Building..."
    cargo build --profile dev-release --bin test_stream_generator 2>&1 | tail -5
    echo ""

    # Run test
    echo "Running test..."
    timeout 30 cargo run --profile dev-release --bin test_stream_generator 2>&1 | grep -E "(Legend position|Drawing legend|Plot saved)"

    # Move plot to position-specific file
    if [ -f plot.png ]; then
        mv plot.png "$output_file"
        echo ""
        echo "✓ Plot saved to: $output_file"
    else
        echo ""
        echo "✗ ERROR: plot.png not found"
        return 1
    fi
}

# Main execution
if [ "$POSITION" = "all" ]; then
    echo "Testing all legend positions..."
    echo ""

    # Test all named positions
    for pos in right left top bottom none; do
        run_test "$pos"
    done

    # Test inside positions
    run_test "inside" "0.95,0.95"    # Top-right
    run_test "inside" "0.05,0.95"    # Top-left
    run_test "inside" "0.95,0.05"    # Bottom-right
    run_test "inside" "0.05,0.05"    # Bottom-left
    run_test "inside" "0.5,0.5"      # Center

    echo ""
    echo "============================================"
    echo "All tests complete!"
    echo "============================================"
    echo ""
    echo "Generated plots:"
    ls -lh plot_legend_*.png

else
    # Test single position
    if [ "$POSITION" = "inside" ]; then
        # Default inside position: top-right
        run_test "inside" "0.95,0.95"
    else
        run_test "$POSITION"
    fi
fi

echo ""
echo "Done!"
