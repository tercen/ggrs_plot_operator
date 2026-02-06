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
| `stream_generator.rs` | Data columns, axis ranges, transforms, color priority |
| `pipeline.rs` | Chart kind, geom selection, theme config, opacity |
| `tson_convert.rs` | TSON parsing, column types |
| `colors.rs` | Palette extraction, color mapping |
| `operator_properties.rs` | Property validation, Result errors |
| `draw_primitives.rs` | BatchRenderer flush methods, opacity, line continuity |

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

## Heatmap Coordinate System

Discrete scales have specific coordinate behavior:

| Aspect | Value |
|--------|-------|
| Category positions | Integers: 0, 1, 2, ..., n-1 |
| Scale range | -0.5 to n-0.5 (0.5 expansion at edges) |
| Tile bounds | Centered at integer, spans i-0.5 to i+0.5 |
| Y-axis | Inverted: ri=0 at TOP, ri=n-1 at bottom |

### "Gray strip at edges"
- Scale uses (-0.5, n-0.5) but tiles use (0, n)
- Fix: Use `x_scale.range()` / `y_scale.range()` for tile coordinate conversion

### "Labels not centered on rows"
- Label positioning must account for Y inversion
- Formula: `y_frac = ((n - 1.0) - i as f64 + 0.5) / n`

## Property Validation Errors

All `OperatorPropertyReader` methods return `Result<T, String>`. Invalid values produce errors:

### "Invalid value for property 'X'"
- Check operator_config.json for typos or wrong value types
- Enum properties must match values from operator.json exactly
- Numeric properties must be parseable as f64/i32 and within range
- Coordinate pairs must be "x,y" format with valid floats

### "Colors wrong in multi-layer plots"
- Check `per_layer_colors` vs `color_infos` priority in stream_generator.rs
- `per_layer_colors` must take priority when present
- Verify `.axisIndex` column is correctly used for per-layer routing

### "Lines not connecting across chunks"
- `flush_lines()` uses `prev_line_points` HashMap to carry forward last point per color
- Verify `prev_line_points` is passed by mutable reference and persists across flush calls
- Check that `add_line_point` is called (not `add_point_with_shape`) when `is_line_geom` is true

## Memory Profiling

The `memprof` module tracks memory at checkpoints:

```rust
let m0 = memprof::checkpoint_return("Before operation");
// ... operation ...
let _m1 = memprof::delta("After operation", m0);
```

`test_local.sh` outputs memory charts to `memory_usage_backend_*.png`.