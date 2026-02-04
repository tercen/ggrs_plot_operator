# Code Review Report

**Date**: 2026-02-04
**Repositories Reviewed**: ggrs_plot_operator, ggrs-core

## Executive Summary

Both repositories follow the documented architecture principles reasonably well. The main areas of concern are: (1) multiple fallback violations using `unwrap_or_default()` and `unwrap_or()` patterns that could mask bugs, (2) several TODOs indicating incomplete implementations, and (3) many session documentation files that are over a year old and likely obsolete.

## Principle Violations

### Fallback/Recovery Violations

| File | Line(s) | Issue |
|------|---------|-------|
| `src/tercen/context/production_context.rs` | 98 | `unwrap_or_default()` - silently returns empty value |
| `src/tercen/context/dev_context.rs` | 86, 155 | `unwrap_or_default()` - silently returns empty value |
| `src/tercen/table.rs` | 97 | `unwrap_or_default()` - columns default to empty vec |
| `src/tercen/properties.rs` | 19 | `unwrap_or_default()` - property lookup returns empty |
| `src/tercen/table_convert.rs` | 99, 108, 117, 126 | `unwrap_or()` - handles nulls with defaults (has TODO) |
| `src/tercen/palettes.rs` | 61 | `unwrap_or([128,128,128])` - fallback to gray on parse error |
| `src/tercen/colors.rs` | 269 | `unwrap_or("Spectral")` - fallback palette name |
| `src/ggrs_integration/stream_generator.rs` | 501-502, 514, 525 | `unwrap_or(&".ri"/.".ci")` - fallback facet variable names |
| `src/ggrs_integration/stream_generator.rs` | 1477, 1596-1597 | `unwrap_or(0)`, `unwrap_or(0.0)` - fallback for missing values |
| `src/ggrs_integration/stream_generator.rs` | 1685 | `unwrap_or(&color_map.default_color)` - fallback color |
| `src/config.rs` | 273 | `unwrap_or(4)` - fallback point size |
| `src/config.rs` | 409 | `unwrap_or((0.95, 0.95))` - fallback legend position |

**ggrs-core violations:**

| File | Line(s) | Issue |
|------|---------|-------|
| `src/render.rs` | 1998-1999, 2621-2622, 2836-2837 | `unwrap_or_default()` on `.ci`/`.ri` columns - returns empty Vec on missing columns |
| `src/stream.rs` | 287, 679, 693 | `unwrap_or(1.0/default)` - parameter fallbacks |
| `src/data.rs` | 21, 26, 382, 387, 408 | `unwrap_or(f64::NAN/0)` - handling null values |
| `src/panel/coord_mapper.rs` | 249 | `unwrap_or((f64::NAN, f64::NAN))` - fallback on dequantize failure |
| `src/scale/continuous.rs` | 135 | `unwrap_or((0.0, 1.0))` - default scale range |
| `src/scale/log.rs` | 125 | `unwrap_or((1.0, 10.0))` - default log scale range |
| `src/scale/logicle.rs` | 143 | `unwrap_or((0.0, 1.0))` - default logicle range |
| `src/engine.rs` | 805, 984, 999 | `unwrap_or()` - fallback chunk size and scale references |

### Error Handling Issues

| File | Line(s) | Issue |
|------|---------|-------|
| `src/tercen/palettes.rs` | 207 | `eprintln!("WARN: ...")` - logs error but doesn't propagate |
| `src/tercen/colors.rs` | 125-165 | `rescale_from_quartiles` returns clone on error instead of Result |
| `src/config.rs` | 158-159, 170-171, 193-194, 258-265, 308-309 | `eprintln!("⚠ Invalid...")` - logs but uses default (fallback) |

### Separation of Concerns

| File | Line(s) | Issue |
|------|---------|-------|
| `src/ggrs_integration/stream_generator.rs` | 1528-1731 | `add_color_columns()` method is ~200 lines doing data transformation - could be extracted to a dedicated color processor module |
| `src/tercen/colors.rs` | 428-549 | `extract_color_info_from_step()` navigates deeply into proto structures (model.axis.xyAxis[0].colors) - tightly coupled to proto schema |

### Abstraction Issues

| File | Line(s) | Issue |
|------|---------|-------|
| `src/ggrs_integration/stream_generator.rs` | 171-219 | `string_to_transform()` duplicates transform parsing logic that could live in ggrs-core |
| `src/config.rs` | 26-41 | `HeatmapCellAggregation::parse()` accepts invalid values and returns default - could use proper error handling |

### Dead Code / Unused Items

| File | Line(s) | Issue |
|------|---------|-------|
| `src/tercen/client.rs` | 31, 40, 46, 52, 134, 162, 181 | Multiple `#[allow(dead_code)]` annotations |
| `src/tercen/error.rs` | 31 | `#[allow(dead_code)]` on error type |
| `src/tercen/logger.rs` | 31 | `#[allow(dead_code)]` on logger |
| `src/ggrs_integration/stream_generator.rs` | 264, 286, 290, 305 | `#[allow(dead_code)]` on fields |
| `src/ggrs_integration/cached_stream_generator.rs` | 28, 42, 47 | `#[allow(dead_code)]` on fields |

### Stale TODOs

| File | Line(s) | Issue |
|------|---------|-------|
| `src/tercen/table_convert.rs` | 99, 108, 117, 126 | `// TODO: Handle nulls properly` - incomplete null handling |
| `src/ggrs_integration/stream_generator.rs` | 617 | `// TODO: Load async if needed` - incomplete legend scale loading |
| `src/ggrs_integration/stream_generator.rs` | 1535 | `// TODO: Handle multiple color factors` - incomplete feature |
| `src/main.rs.template` | 64, 75 | `// TODO: Implement health check` and `// TODO: Implement main operator execution` - template file has incomplete implementations |

**ggrs-core TODOs:**

| File | Line(s) | Issue |
|------|---------|-------|
| `src/decoration_layout.rs` | 85 | `// TODO: Get from theme` - hardcoded spacing |
| `src/renderer/axis.rs` | 72 | `// TODO: Implement axis rendering` - incomplete |
| `src/renderer/point.rs` | 241 | `// TODO: Implement Cairo point rendering` - incomplete |
| `src/renderer/text.rs` | 73 | `// TODO: Implement text rendering` - incomplete |
| `src/render.rs` | 1665, 2227 | `// TODO: Implement WebGPU rendering`, `// TODO: Get from theme` |
| `src/theme/elements.rs` | 289 | `// TODO: should use line height` |

## Outdated Files

### Candidates for Deletion

| File | Reason |
|------|--------|
| `SESSION_2025-01-15.md` | Over 1 year old (dated 2025-01-15) |
| `SESSION_2025-01-16_PAGES_DEBUG.md` | Over 1 year old |
| `SESSION_2025-01-18_PAGINATION_OPTIMIZATION.md` | Over 1 year old |
| `SESSION_2025-01-19_PAGINATION_FIX.md` | Over 1 year old |
| `SESSION_2025-01-22_TEXT_ELEMENTS.md` | Over 1 year old |
| `SESSION_2025-01-22_LAYOUT_AND_ROTATION.md` | Over 1 year old |
| `SESSION_2025-01-27_TICK_ROTATION.md` | Over 1 year old |
| `SESSION_2025-01-28_CACHE_AND_CLIPPY.md` | Over 1 year old |
| `SESSION_2025-01-29_SCHEMA_IDS_FIX.md` | Over 1 year old |
| `SESSION_COLOR_LABELS.md` | No date, but related to old sessions |
| `docs/SESSION_2025-01-05.md` | Over 1 year old |
| `docs/SESSION_2025-01-07.md` | Over 1 year old |
| `docs/SESSION_2025-01-08.md` | Over 1 year old |
| `src/main.rs.template` | Template file with incomplete TODOs - unclear if still used |

### Candidates for Update

| File | Reason |
|------|--------|
| `CONTINUE.md` | Should reflect current status after session work |
| `README.md` | Modified but uncommitted - review for accuracy |
| `CLAUDE.md` | Modified but uncommitted - review for accuracy |

### Orphaned/Temporary Files in Root

| File | Reason |
|------|--------|
| `.~lock.review_points.txt#` | Lock file, should not be committed |
| `CHECKPOINT_POSITIONING_SIMPLE.md` | Possibly obsolete checkpoint doc |
| `PLAN_TRANSFORMS.md` | Implementation plan - review if still active |
| `REVIEW.md` | Previous review file - may duplicate this report |
| `checkpoint_plot_*.png` | Test output files, should not be in git |
| `colors.png`, `comparison.png`, `crosstab.png`, etc. | Test output images |
| `memory_usage_*.csv`, `memory_usage_*.png` | Profiling artifacts |
| `operator_out.png`, `palettes.png`, `plot.png`, etc. | Test output images |
| `stage_*.png`, `stage_*.csv` | Test artifacts |
| `test_pagination.sh` | Test script - consider moving to tests/ |
| `review_points.txt` | Review notes - consider cleaning up |

## Statistics

- **Total violations found**: 51
- **Critical violations**: 6 (the `unwrap_or_default()` on .ci/.ri columns in ggrs-core render.rs - could cause silent rendering failures)
- **ggrs_plot_operator files reviewed**: 25 Rust files
- **ggrs-core files reviewed**: ~40 Rust files (key modules)
- **Outdated files identified**: 13 session files + multiple temporary/test artifacts
- **Stale TODOs**: 14

## Recommendations (Priority Order)

1. **Critical**: Review and address `unwrap_or_default()` usage in `ggrs-core/src/render.rs:1998-1999, 2621-2622, 2836-2837` - missing `.ci`/`.ri` columns will silently produce empty vectors, causing incorrect rendering with no error

2. **High**: Address TODOs marked with `// TODO: Handle nulls properly` in `table_convert.rs` - current null handling may silently corrupt data

3. **Medium**: Clean up old SESSION_*.md files - they occupy space and may cause confusion about current status

4. **Medium**: Consider extracting `add_color_columns()` from stream_generator.rs into a dedicated color processing module

5. **Low**: Remove `#[allow(dead_code)]` annotations and either use or delete the associated code

6. **Low**: Clean up test artifacts (PNG files, CSV files) from the repository root