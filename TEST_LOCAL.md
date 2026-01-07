# Local Testing Guide

This guide explains how to test the `TercenStreamGenerator` locally using workflow and step IDs, similar to Python's `OperatorContextDev`.

## Prerequisites

1. **Tercen authentication token**: Get this from your Tercen session
2. **Workflow ID and Step ID**: From your Tercen workflow containing the operator

## Method 1: Using the Test Script (Recommended)

```bash
./test_local.sh "your_token" "workflow_id" "step_id"
```

Example:
```bash
./test_local.sh \
  "eyJhbGciOiJIUzI1NiIsInR5cCI..." \
  "5f8a9b2c3d4e5f6g7h8i" \
  "6g9h0i1j2k3l4m5n6o7p"
```

## Method 2: Manual Environment Variables

```bash
export TERCEN_URI=https://tercen.com:5400
export TERCEN_TOKEN=your_token_here
export WORKFLOW_ID=your_workflow_id
export STEP_ID=your_step_id

cargo run --bin test_stream_generator
```

## What the Test Does

The test binary will:

1. **Connect to Tercen** using your token
2. **Fetch workflow and step** from Tercen (like Python's OperatorContextDev)
3. **Extract CubeQuery** with table IDs (qt_hash, column_hash, row_hash)
4. **Load facet metadata** from column and row tables
5. **Compute axis ranges** for all facet cells (scans the main table)
6. **Query sample data** (first 100 rows from facet cell 0,0)
7. **Display results**:
   - Workflow and step names
   - Table IDs extracted
   - Number of column/row facets
   - Axis ranges for each facet cell
   - Sample data rows

## Expected Output

```
=== TercenStreamGenerator Test ===

Configuration:
  URI: https://tercen.com:5400
  Token: eyJhbGciOi...
  Workflow ID: 5f8a9b2c3d4e5f6g7h8i
  Step ID: 6g9h0i1j2k3l4m5n6o7p

Connecting to Tercen...
✓ Connected successfully

Fetching CubeQuery from workflow step...
  Workflow name: My Analysis Workflow
  Step name: GGRS Plot
  Getting CubeQuery from existing task: 7h1i2j3k4l5m6n7o8p9q
✓ CubeQuery retrieved
  Main table (qt_hash): a1b2c3d4e5f6g7h8i9j0
  Column table (column_hash): col_abc123
  Row table (row_hash): row_def456

Creating TercenStreamGenerator...
Loaded facets: 2 columns × 3 rows = 6 cells
Computing axis ranges for all facet cells...
Computed ranges for 6 facet cells
✓ Stream generator created successfully

=== Facet Information ===
Column facets: 2
Row facets: 3
Total facet cells: 6

=== Testing Axis Ranges ===
Facet cell (0, 0):
  X-axis: [49.50, 72.45] (data: [50.00, 72.00])
  Y-axis: [5.24, 8.75] (data: [5.50, 8.50])

Facet cell (0, 1):
  X-axis: [48.80, 67.20] (data: [49.00, 67.00])
  Y-axis: [4.92, 7.56] (data: [5.20, 7.30])

...

=== Testing Data Query ===
Querying data chunk for facet (0, 0), range 0-100...
✓ Received 95 rows
  Columns: [".x", ".y", "sp"]

First 5 rows:
  Row 0: .x=Float(51.0) .y=Float(6.1)
  Row 1: .x=Float(52.0) .y=Float(7.7)
  Row 2: .x=Float(53.0) .y=Float(6.8)
  Row 3: .x=Float(54.0) .y=Float(7.2)
  Row 4: .x=Float(55.0) .y=Float(6.4)

=== Test Complete ===
All checks passed! The TercenStreamGenerator is working correctly.
```

## Troubleshooting

### "TERCEN_TOKEN environment variable is required"
- Make sure you're passing the token as the first argument to the script
- Or export it manually: `export TERCEN_TOKEN=your_token`

### "MAIN_TABLE_ID environment variable is required"
- You need to provide the main table hash
- Get it from a task by running the main operator first

### "Failed to connect to Tercen"
- Check your token is valid
- Verify the URI is correct (default: https://tercen.com:5400)
- Check network connectivity

### "Table not found" or "Permission denied"
- Verify the table IDs are correct
- Make sure your token has access to those tables
- Check that the tables still exist in Tercen

## Next Steps

Once the test passes, you can proceed to:
1. Test plot generation with GGRS
2. Test PNG rendering
3. Test result upload to Tercen
