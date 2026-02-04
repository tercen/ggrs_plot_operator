# Commands

Build, test, and development commands for ggrs_plot_operator.

## Build

```bash
# Fast development builds (incremental compilation)
cargo build --profile dev-release

# Production builds (LTO enabled, slow)
cargo build --release
```

## Quality Checks (MANDATORY)

Run before considering any task complete:

```bash
cargo fmt && cargo clippy -- -D warnings && cargo test
```

## Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Local development test (requires Tercen instance)
./test_local.sh [backend]  # cpu (default) or gpu
```

### Test Examples in test_local.sh

Edit the script to uncomment the desired WORKFLOW_ID/STEP_ID:

| Example | Description |
|---------|-------------|
| EXAMPLE1 | Heatmap with divergent palette |
| EXAMPLE2 | Simple scatter (no X-axis table) |
| EXAMPLE3 | Scatter with X-axis table (crabs dataset) |
| EXAMPLE4 | Log transform test |

## Local Development

```bash
# Required environment variables
export TERCEN_URI=http://127.0.0.1:50051
export TERCEN_TOKEN=your_token
export WORKFLOW_ID=your_workflow_id
export STEP_ID=your_step_id

# Run dev binary
cargo run --bin dev --profile dev-release
```

## Proto Setup

```bash
git submodule update --init --recursive
```

## Operator Config Override

Create `operator_config.json` to override defaults:

```json
{
  "backend": "gpu",
  "plot.width": "800",
  "legend.position": "right",
  "png.compression": "fast"
}
```

## Git Policy

- Never commit/push unless explicitly requested
- Run quality checks before reporting task complete
- Use `cargo fmt` before any commit