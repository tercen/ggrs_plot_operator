# Testing with Workflow ID and Step ID

The test binary now supports automatically extracting table IDs from a workflow step, similar to Python's `OperatorContextDev`.

## Current Status

The implementation is partially complete. Due to proto complexity (nested `oneof` structures), you have two options:

### Option 1: Manual Table ID Extraction (Works Now)

Get the table IDs from your workflow step manually, then pass them directly:

```bash
export TERCEN_URI=https://tercen.com:5400
export TERCEN_TOKEN=your_token
export WORKFLOW_ID=your_workflow_id
export STEP_ID=your_step_id

# This will partially work - it will try to extract table IDs
# If it fails, note the error and use Option 2
cargo run --bin test_stream_generator
```

### Option 2: Use Task ID (Recommended for Now)

If you have a task ID from a previous run, use the main operator to see table IDs:

```bash
export TERCEN_URI=https://tercen.com:5400
export TERCEN_TOKEN=your_token
export TERCEN_TASK_ID=your_task_id

cargo run  # Main operator will show table IDs

# Look for output like:
#   Table hashes:
#     qt_hash (main data): abc123...
#     column_hash: col456...
#     row_hash: row789...
```

Then manually create the stream generator with those IDs by modifying the test binary temporarily.

## What Works

✅ Connects to Tercen with token
✅ Gets workflow by ID
✅ Finds step by ID
✅ Calls `WorkflowService.getCubeQuery()` (when step has no task)
✅ Gets `CubeQuery` from existing task (when step already ran)

## What Needs Work

The proto unwrapping needs refinement:
- `EWorkflow` wraps `Workflow` in a oneof
- `EStep` wraps different step types (TableStep, CrossTabStep, etc.) in a oneof
- Each step type has its own model structure

This is solvable but requires careful proto navigation.

## Next Session

We can either:
1. **Fix the proto unwrapping** (30 min) - Make workflow/step extraction fully automatic
2. **Skip to plot generation** - Test with manual table IDs, move forward with rendering

The core `TercenStreamGenerator` is complete and working - we just need a convenience method to get table IDs from workflow/step.

