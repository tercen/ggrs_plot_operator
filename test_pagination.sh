#!/bin/bash
# Test pagination with cache verification

set -e

# Configuration from test_local.sh
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJodHRwOi8vMTI3LjAuMC4xOjU0MDAiLCJleHAiOjE3NzEzNTY5MTAsImRhdGEiOnsiZCI6IiIsInUiOiJ0ZXN0IiwiZSI6MTc3MTM1NjkxMDQ0Nn19.irjJcnJnFR7TuUFz_7fyMXcC7wL4WvYBPsXpDkplv8c"
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="b9659735-27db-4480-b398-4e391431480f"

# Create operator config
cat > operator_config.json <<EOJ
{
  "chunk_size": 15000,
  "max_chunks": 100000,
  "cache_axis_ranges": true,
  "default_plot_width": 6000,
  "default_plot_height": 2000,
  "render_backend": "cpu",
  "legend.position": "right",
  "legend.justification": "0.1,0.1"
}
EOJ

echo "============================================"
echo "Testing pagination with cache verification"
echo "============================================"
echo ""

# Build main operator (not test binary!)
echo "Building main operator..."
cargo build --profile dev-release --bin ggrs_plot_operator 2>&1 | tail -5
echo ""

# Run and capture stderr to see cache messages
echo "Running operator (watch for Cache HIT/MISS messages)..."
echo ""
timeout 60 ./target/dev-release/ggrs_plot_operator 2>&1 | grep -E "(Cache (HIT|MISS)|Page [0-9]+/|Generating Page)" || true

echo ""
echo "============================================"
echo "Test complete! Check for cache messages above."
echo "============================================"
