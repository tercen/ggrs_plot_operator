# Continue From Here

**Last Updated**: 2026-02-04
**Status**: Refactoring complete, ready for new features

---

## Current State

The codebase has been significantly cleaned up and refactored:

### Recent Changes (2026-02-04)

1. **Rendering path consolidation**
   - Removed standard rendering path from ggrs-core (~1000 lines)
   - Only lightweight path (`stream_and_render_direct`) remains
   - Removed `PanelContext`, `use_lightweight_rendering` field

2. **Stream generator refactoring**
   - Extracted `add_color_columns()` to `src/tercen/color_processor.rs`
   - Moved `string_to_transform()` to ggrs-core as `Transform::parse()`
   - Reduced `stream_generator.rs` by ~250 lines

3. **Documentation cleanup**
   - Deleted 13 obsolete SESSION_*.md files
   - Deleted 18 obsolete plan/status docs
   - Cleaned up test artifacts from root

### Transform Support

Log, asinh, and logicle transforms are now supported:
- Transform info passed via `NumericAxisData.transform`
- `Transform::parse()` in ggrs-core handles string parsing
- Supported formats: "log", "ln", "log10", "sqrt", "asinh", "asinh:cofactor=5", "logicle:T=262144,W=0.5,M=4.5,A=0"

---

## Project Architecture

```
src/
├── tercen/                    # Tercen gRPC integration
│   ├── client.rs              # TercenClient with auth
│   ├── context/               # TercenContext trait + impls
│   ├── table.rs               # TableStreamer
│   ├── colors.rs              # Color types, palette extraction
│   ├── color_processor.rs     # add_color_columns() function
│   ├── palettes.rs            # PALETTE_REGISTRY
│   └── ...
├── ggrs_integration/
│   ├── stream_generator.rs    # TercenStreamGenerator (implements StreamGenerator)
│   └── cached_stream_generator.rs
├── pipeline.rs                # Plot generation orchestration
├── config.rs                  # OperatorConfig from operator.json
└── main.rs                    # Production entry point
```

---

## Test Configuration

Edit `test_local.sh` to select test example:
- **EXAMPLE1**: Heatmap with divergent palette
- **EXAMPLE2**: Simple scatter (no X-axis table)
- **EXAMPLE3**: Scatter with X-axis table (crabs dataset)
- **EXAMPLE4**: Log transform test

---

## Development Workflow

```bash
# Build
cargo build --profile dev-release

# Quality checks (MANDATORY)
cargo fmt && cargo clippy -- -D warnings && cargo test

# Local test
./test_local.sh [cpu|gpu]
```

### Cargo.toml Dependency

Check dependency mode before committing:
```toml
# Local dev (uncomment for local ggrs changes):
# ggrs-core = { path = "../ggrs/crates/ggrs-core", features = [...] }

# CI/Production (default):
ggrs-core = { git = "https://github.com/tercen/ggrs", branch = "main", features = [...] }
```

---

## Known Issues

See `REVIEW_REPORT.md` for:
- Fallback violations using `unwrap_or_default()`
- Stale TODOs
- Dead code with `#[allow(dead_code)]`

---

## Session History

- 2026-02-04: Refactoring complete - rendering path, color processor, transform parsing, docs cleanup
- 2026-02-03: Transform support implemented (log, asinh, logicle)
