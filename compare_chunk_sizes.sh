#!/bin/bash
# Compare memory usage across different chunk sizes
#
# This script runs the test with multiple chunk sizes and generates
# memory usage charts for comparison.

set -e

# Chunk sizes to test
CHUNK_SIZES=(100 500 1000 5000 10000 50000)

echo "============================================"
echo "Memory Usage Comparison Across Chunk Sizes"
echo "============================================"
echo ""
echo "Testing chunk sizes: ${CHUNK_SIZES[@]}"
echo ""

# Run tests for each chunk size
for size in "${CHUNK_SIZES[@]}"; do
    echo ""
    echo "=========================================="
    echo "Testing chunk_size=$size"
    echo "=========================================="
    ./test_local.sh $size

    # Brief pause between tests
    sleep 2
done

echo ""
echo "============================================"
echo "All tests complete!"
echo "============================================"
echo ""
echo "Generated files:"
for size in "${CHUNK_SIZES[@]}"; do
    echo "  - memory_usage_chunk_${size}.png"
    echo "  - memory_usage_chunk_${size}.csv"
done
echo ""
echo "To view memory statistics:"
echo "  column -t -s',' memory_usage_chunk_*.csv | head -20"
echo ""
echo "To compare peak memory usage:"
echo "  for f in memory_usage_chunk_*.csv; do echo -n \"\$f: \"; tail -n +2 \$f | cut -d',' -f2 | sort -n | tail -1; done"
