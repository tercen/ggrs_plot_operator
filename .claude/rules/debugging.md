# Debugging Rules

Lessons learned and debugging practices.

## No Recovery, No Fallbacks

When debugging or fixing errors, **do NOT add recovery logic or fallbacks**. Instead:
1. Find the root cause
2. Fix the actual problem
3. Add informative error messages

If you're tempted to add `unwrap_or_default()` or a fallback branch, stop and investigate why the expected value is missing.

## One Change at a Time

**CRITICAL**: Verify ONE change at a time. Don't batch 6 file changes without testing between each.

Bad pattern:
1. Make changes to 6 files
2. Run test
3. Nothing changed - which file was wrong?

Good pattern:
1. Add ONE diagnostic print
2. Verify it works
3. Make next change
4. Verify it works

## Rendering Path

There is only ONE rendering path: `stream_and_render_direct()` in `ggrs-core/src/render.rs`.

This function uses direct Cairo drawing with layout math - no ChartContext overhead.

**When modifying rendering**: Update `stream_and_render_direct()` directly.

## Add Diagnostics First

Before any complex change, add diagnostic prints to verify data flow:

```rust
eprintln!("DEBUG: transform reaches GGRS: {:?}", transform);
eprintln!("DEBUG: axis_range for ({}, {}): {:?}", col, row, range);
```

## Common Debug Points

| Location | What to Check |
|----------|---------------|
| `stream_generator.rs` | Data columns, axis ranges, transforms |
| `pipeline.rs` | Chart kind, geom selection, theme config |
| `tson_convert.rs` | TSON parsing, column types |
| `colors.rs` | Palette extraction, color mapping |

## Error Patterns

### "Column not found"
- Check if column is requested in `stream_bulk_data()`
- Verify TSON table has the column (check schema)

### "Axis range missing"
- Y-axis table is required - check `schema_ids`
- Verify facet indices match axis_ranges keys

### "Colors not appearing"
- Check `color_infos` is populated in context
- Verify `.color` aesthetic is set in `Aes`
- Check `add_color_columns()` is called

### "Transform not applied"
- Verify transform reaches `NumericAxisData.transform`
- Check you're updating the lightweight rendering path
- Add diagnostic print in GGRS to verify

## Memory Profiling

The `memprof` module tracks memory at checkpoints:

```rust
let m0 = memprof::checkpoint_return("Before operation");
// ... operation ...
let _m1 = memprof::delta("After operation", m0);
```

`test_local.sh` outputs memory charts to `memory_usage_backend_*.png`.