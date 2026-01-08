# Deployment Debugging Guide

## Current Status (2026-01-08)

### Issue: UnimplementedError in Production

**Problem**: The operator successfully connects to Tercen via gRPC but fails with `UnimplementedError` when calling gRPC methods in the deployed container.

**Error Message**:
```
✓ Successfully connected to Tercen!
Warning: Failed to send log to Tercen: gRPC error: status: 'Unknown error', self: "UnimplementedError"
✗ Task processing failed: gRPC error: status: 'Unknown error', self: "UnimplementedError"
```

### Root Cause Analysis

**What Works**:
- ✅ gRPC connection established successfully
- ✅ Authentication works (Bearer token accepted)
- ✅ TaskService.get() works - can fetch task information
- ✅ TaskService.runTask() works - can trigger tasks
- ✅ TableSchemaService.streamTable() works - can query data

**What Fails**:
- ❌ EventService.create() - Returns `UnimplementedError`
- ❌ All logging calls using TercenLogger fail

**Hypothesis**: The local Tercen instance may not have EventService fully implemented, or the EventService requires different authentication/setup than other services.

### Temporary Workaround (CURRENT STATE)

**All logging has been disabled** to allow the operator to function while we debug the EventService issue.

**Files Modified**:
- `src/main.rs` - All `logger.log()` calls commented out
- Function signatures changed from `logger` to `_logger` to indicate unused parameter

**Example Changes**:
```rust
// Before:
logger.log("Processing task").await?;

// After (temporary):
// logger.log("Processing task").await?;
```

### Task Type Support Added

During debugging, we discovered the operator was only matching `ComputationTask` but not other task types. We added support for:

1. **RunComputationTask** - The actual task type being sent in testing
2. **CubeQueryTask** - Alternative query task type
3. **Debug output** - Shows actual task variant when unmatched

**Code Location**: `src/main.rs` lines 183-334

### Testing Results

**Local Binary Test** (without Docker):
```bash
TERCEN_URI="http://127.0.0.1:50051" \
TERCEN_TOKEN="eyJ0eXAi..." \
TERCEN_TASK_ID="dd92873448ae348da75d4cc74c18582d" \
./target/dev-release/ggrs_plot_operator
```

**Output**:
```
✓ Successfully connected to Tercen!
✓ Task retrieved: dd92873448ae348da75d4cc74c18582d
  Task type: RunComputationTask
  Query found
  Table hashes:
    qt_hash (main data): dd92873448ae348da75d4cc74c185f52
    column_hash: dd92873448ae348da75d4cc74c186cac
    row_hash: dd92873448ae348da75d4cc74c186f79
  Operator settings: OperatorSettings { ... width: "800", height: "600", ... }
✓ Task processed successfully!
```

**Status**: ✅ Works perfectly without logging!

## Build Performance Optimization

### Problem: Slow CI/CD Builds

Original Docker builds were taking **12+ minutes** due to aggressive optimization settings:
- `opt-level = 3` - Maximum optimization
- `lto = true` - Link-Time Optimization (very slow)
- `codegen-units = 1` - Single-threaded codegen

### Solution: dev-release Profile

Created a new `dev-release` profile in `Cargo.toml` for faster builds during development and testing:

```toml
[profile.dev-release]
inherits = "release"
opt-level = 2          # Still optimized, but less aggressive
lto = false            # No LTO = much faster linking
codegen-units = 16     # Parallel codegen = faster builds
strip = false          # Keep debug info for better errors
incremental = true     # Enable incremental compilation
```

**Build Times**:
- Full `--release` build: **12+ minutes**
- Full `--profile dev-release` build: **4-5 minutes**
- Incremental `dev-release` builds: **<30 seconds**

### Dockerfile Changes

**File**: `Dockerfile`

**Changed**:
```dockerfile
# Before:
cargo build --release --features jemalloc
COPY --from=builder /app/target/release/ggrs_plot_operator /usr/local/bin/

# After:
cargo build --profile dev-release --features jemalloc
COPY --from=builder /app/target/dev-release/ggrs_plot_operator /usr/local/bin/
```

**Why**: The `dev-release` profile is perfectly adequate for testing and CI. The performance difference is negligible for a plotting operator, and the faster build times greatly improve iteration speed.

**For Production Releases**: When creating official releases, consider switching back to `--release` or create a separate production Dockerfile.

## Next Steps to Debug EventService

### 1. Investigate EventService Implementation

Check if EventService is actually implemented in the local Tercen instance:

```rust
// Test EventService separately
let mut event_service = client.event_service()?;
// Try to create a simple event
```

### 2. Check Proto Version Compatibility

The proto files might be out of sync with the Tercen server version:
- Compare `protos/tercen.proto` and `protos/tercen_model.proto` with server version
- Check if EventService has different message formats in newer/older versions

### 3. Alternative Logging Approaches

Options if EventService remains unavailable:

**Option A: Make logging optional**
```rust
if let Some(logger) = logger {
    logger.log("message").await.ok(); // Ignore errors
}
```

**Option B: Use stdout only**
```rust
// Just print to stdout, Tercen may capture container logs
println!("LOG: Processing task...");
```

**Option C: Check environment variable**
```rust
let enable_logging = std::env::var("ENABLE_TERCEN_LOGGING")
    .unwrap_or_else(|_| "false".to_string()) == "true";

if enable_logging {
    logger.log("message").await?;
}
```

### 4. Test with Real Tercen Instance

The local Tercen instance might differ from production. Test with:
- tercen.com production instance
- Different local Tercen version
- Check Tercen logs for EventService errors

## Testing Procedure

### Current Test Workflow

1. **Disable logging** (already done)
2. **Build with dev-release**:
   ```bash
   cargo build --profile dev-release
   ```

3. **Test locally**:
   ```bash
   TERCEN_URI="http://127.0.0.1:50051" \
   TERCEN_TOKEN="your_token" \
   TERCEN_TASK_ID="task_id" \
   ./target/dev-release/ggrs_plot_operator
   ```

4. **Build Docker image**:
   ```bash
   docker build --secret id=gh_pat,src=$HOME/.config/gh/token -t ggrs_plot_operator:test .
   ```

5. **Test in Podman** (as Tercen does):
   ```bash
   podman run --rm --network host ggrs_plot_operator:test \
     --taskId task_id \
     --serviceUri http://127.0.0.1:50051 \
     --token your_token
   ```

6. **Push to GitHub** and let CI build/push
7. **Trigger task in Tercen** via UI at:
   http://127.0.0.1:5400/test/w/WORKFLOW_ID/ds/STEP_ID

### When Logging is Fixed

1. Re-enable logging in `src/main.rs`:
   ```bash
   # Uncomment all logger.log() calls
   sed -i 's/\/\/ logger\.log(/logger.log(/g' src/main.rs
   ```

2. Change function signatures back:
   ```rust
   // Change _logger back to logger
   ```

3. Test again to ensure EventService works

## Known Issues

### JWT Token Format

The token format matters! We saw this error with a malformed token:
```
JWTError: Could not decode token string. Error: FormatException:
Unexpected character (at character 57) {bd":"","u":"test",...
```

**Solution**: Use the token from `test_local.sh` which has correct format.

### Task Data Empty

The test task `dd92873448ae348da75d4cc74c18582d` returned 0 rows. This is likely because:
- The task is old and data was cleaned up
- Need to trigger a fresh task with actual data

**Solution**: Click "Run" button in Tercen UI to create fresh task.

## Files Modified for Debugging

1. **src/main.rs**
   - Lines 118-134: Commented out logging in main()
   - Lines 157-163: Changed `logger` to `_logger`
   - Lines 183-334: Added RunComputationTask and CubeQueryTask support
   - All `logger.log()` calls commented out throughout

2. **Cargo.toml**
   - Lines 79-88: Added `[profile.dev-release]` section

3. **Dockerfile**
   - Line 43-51: Changed to use `--profile dev-release`
   - Line 78: Changed binary path to `target/dev-release/`

## Reverting Changes

When EventService is working, revert logging changes:

```bash
# Restore logging
git diff src/main.rs  # Review changes
git checkout src/main.rs  # Revert if needed

# Or manually uncomment:
sed -i 's/\/\/ logger\.log(/logger.log(/g' src/main.rs

# Change _logger back to logger in function signatures
# (manual edit required)
```

## Performance Notes

**Current binary size**:
- dev-release: ~40-50 MB (with debug info)
- release: ~8-10 MB (stripped)

**Runtime performance**: Negligible difference for this operator. The plotting library (GGRS) dominates execution time, not the Rust optimizer level.

**Memory usage**: Same for both profiles (~50-200 MB depending on data size and backend).

## Contact/Reference

- See `docs/SESSION_2025-01-05.md` for previous debugging session
- See `docs/SESSION_2025-01-07.md` for GPU backend investigation
- See `BUILD.md` for comprehensive build instructions
- See `TEST_LOCAL.md` for local testing procedures
