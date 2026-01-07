#!/bin/bash
# Summarize memory usage across all tested chunk sizes

echo "============================================"
echo "Memory Usage Summary"
echo "============================================"
echo ""
printf "%-12s %-15s %-15s %-15s\n" "Chunk Size" "Max Memory" "Mean Memory" "Samples"
printf "%-12s %-15s %-15s %-15s\n" "----------" "----------" "-----------" "-------"

for csv in memory_usage_chunk_*.csv; do
    if [ -f "$csv" ]; then
        # Extract chunk size from filename
        chunk_size=$(echo "$csv" | sed 's/memory_usage_chunk_//' | sed 's/.csv//')

        # Calculate stats from CSV
        max_kb=$(tail -n +2 "$csv" | sort -n | tail -1)
        mean_kb=$(tail -n +2 "$csv" | awk '{sum+=$1; count++} END {if(count>0) print sum/count; else print 0}')
        samples=$(tail -n +2 "$csv" | wc -l)

        # Convert to MB
        max_mb=$(echo "scale=2; $max_kb / 1024" | bc)
        mean_mb=$(echo "scale=2; $mean_kb / 1024" | bc)

        printf "%-12s %-15s %-15s %-15s\n" "$chunk_size" "${max_mb} MB" "${mean_mb} MB" "$samples"
    fi
done | sort -n -k1

echo ""
echo "CSV files: memory_usage_chunk_*.csv"
echo "PNG charts: memory_usage_chunk_*.png"
