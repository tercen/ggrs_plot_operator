# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## Project Overview

**ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with Tercen's gRPC API. It receives tabular data, generates high-performance plots with faceting and colors, and returns PNG images for visualization.

**Version**: 0.0.3-dev (Legend Support - In Progress)
**Status**: ✅ Production-ready (logging disabled due to EventService issue)

---

## 🎯 Current Development Session (2025-01-15)

### Session Summary: Legend Rendering Infrastructure

**Completed Today:**
1. ✅ **GGRS Legend Rendering** - Implemented complete legend system in `ggrs/crates/ggrs-core/src/render.rs`:
   - `draw_continuous_legend()`: Gradient bars with min/max labels for numeric color scales
   - `draw_discrete_legend()`: Colored circles with category labels for categorical scales
   - `parse_hex_color()`: Hex color string parser for theme colors
   - Integration in `render_to_file_cairo()` with proper surface flushing
   - Theme-aware styling (extracts colors/sizes from Element enum via pattern matching)

2. ✅ **Type System Fixes**:
   - Element enum pattern matching (no helper methods, must match Text/Rect/Line/Blank)
   - LegendPosition::Inside type cast (f64 → i32 for pixel coordinates)
   - String/&str handling (format!() returns String, need .as_str())
   - Proper None handling in LegendPosition match

3. ✅ **Build & Test**:
   - GGRS builds cleanly (4.8s release build)
   - Operator builds cleanly (12.7s dev-release)
   - Test workflow runs successfully (44K rows, 1608 facet cells, 0.6s total)

**Current Issue - Legend Not Displayed:**
- The legend rendering code works, but `query_legend_scale()` returns `LegendScale::None`
- **Root Cause**: Line 1116-1120 in `stream_generator.rs`
  ```rust
  // Level-based categorical colors (.colorLevels)
  // TODO: Implement by streaming a sample or full data to extract unique categories
  LegendScale::None
  ```
- Test workflow uses `.colorLevels` (level indices 0-7) without explicit category mappings
- Category names exist in color table (05d2ba1b9d4b123ae85f75cc061a3a00) but aren't loaded

**Tomorrow's Task:**
Implement `query_legend_scale()` for `.colorLevels`-based categorical colors:

**File**: `/home/thiago/workspaces/tercen/main/ggrs_plot_operator/src/ggrs_integration/stream_generator.rs`
**Lines**: 1116-1120

**Implementation Plan**:
1. **Stream color table data** to get category names:
   ```rust
   // In query_legend_scale(), for Categorical with empty mappings:
   if let Some(color_table_id) = self.color_table_ids.get(0) {
       // Stream the color table (e.g., "Country" column)
       let streamer = TableStreamer::new(&self.client);
       let data = streamer.stream_tson(color_table_id, None, 0, 1000).await?;
       let df = tson_to_dataframe(&data)?;

       // Extract unique category names from column (factor_name = "Country")
       let categories: Vec<String> = df.column(&color_info.factor_name)?
           .unique()?
           .iter()
           .map(|v| v.to_string())
           .collect();

       return LegendScale::Discrete {
           values: categories,
           aesthetic_name: color_info.factor_name.clone(),
       };
   }
   ```

2. **Key Data Structures**:
   - `self.color_table_ids: Vec<String>` - Already stored during init (from `extract_color_info_from_step()`)
   - Color table ID: `05d2ba1b9d4b123ae85f75cc061a3a00` (query_table_type = "color_0")
   - Contains column: `"Country"` (string type)
   - Has unique category values that map to `.colorLevels` indices

3. **Design Considerations**:
   - This requires async streaming, but `query_legend_scale()` is sync
   - **Option A**: Make query_legend_scale() load categories during init (preferred)
   - **Option B**: Cache categories in TercenStreamGenerator struct during `new()`
   - **Option C**: Load on-demand with tokio::block_in_place (current pattern)

4. **Testing**:
   ```bash
   TERCEN_URI="http://127.0.0.1:50051" \
   TERCEN_TOKEN="eyJ0eXAi..." \
   WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c" \
   STEP_ID="b9659735-27db-4480-b398-4e391431480f" \
   cargo run --profile dev-release --bin test_stream_generator
   ```
   - Should see: "DEBUG: Drawing legend" in output
   - Plot.png should show legend on right side with colored circles and country names

**Files Modified Today**:
- `/home/thiago/workspaces/tercen/main/ggrs/crates/ggrs-core/src/render.rs` (lines 68, 73, 493-501, 573-658, 1283-1537)
- CLAUDE.md (this file)

**Key References**:
- Color table investigation: Phase 2.5 in test output shows color table structure
- Legend scale types: `/home/thiago/workspaces/tercen/main/ggrs/crates/ggrs-core/src/legend.rs`
- Color extraction: `src/tercen/colors.rs` - `extract_color_info_from_step()`

---

## Quick Reference

### Essential Commands

```bash
# Build (use dev-release for 4-5 min builds)
cargo build --profile dev-release

# Quality Checks (MANDATORY before code completion!)
cargo fmt --check && cargo fmt
cargo clippy -- -D warnings
cargo test

# Local Testing
./test_local.sh

# Docker & Release
docker build -t ggrs_plot_operator:local .
git tag 0.1.0 && git push origin 0.1.0  # NO 'v' prefix
```

### Quick Debugging

- **Not connecting?** Check `TERCEN_URI`, `TERCEN_TOKEN` env vars
- **Build failing?** Run `cargo clean && cargo build --profile dev-release`
- **Faceting issues?** Verify `.ci`/`.ri` columns exist, check facet metadata tables
- **Legend not showing?** Check `query_legend_scale()` returns Continuous/Discrete (not None)

See `BUILD.md`, `TEST_LOCAL.md`, `DEPLOYMENT_DEBUG.md` for comprehensive instructions.

---

## Architecture

### Module Structure

```
src/
├── main.rs                      # Entry point (logging disabled)
├── config.rs                    # Property-based configuration
├── tercen/                      # Pure Tercen gRPC client (future crate)
│   ├── client.rs               # TercenClient with auth
│   ├── table.rs                # TableStreamer (chunked streaming)
│   ├── tson_convert.rs         # TSON → Polars DataFrame (columnar)
│   ├── facets.rs               # Facet metadata loading
│   ├── properties.rs           # PropertyReader, PlotDimension
│   ├── colors.rs               # Color palette parsing & interpolation
│   ├── result.rs               # Result upload (Phase 8)
│   └── error.rs                # TercenError types
├── ggrs_integration/
│   └── stream_generator.rs     # TercenStreamGenerator (GGRS StreamGenerator trait)
└── bin/
    └── test_stream_generator.rs # Test binary

tercen_grpc_api/                # Git submodule (canonical proto files)
```

### Three-Layer Design

1. **gRPC Client** (`src/tercen/`): Connection, auth, streaming (tonic, prost, tokio)
2. **Data Transform** (Columnar): TSON → Polars DataFrame - **NO row-by-row processing!**
3. **GGRS Integration**: TercenStreamGenerator, lazy facet loading, GPU rendering

### Data Flow

```
1. TercenStreamGenerator::new()
   → Connect via gRPC, load facets, load Y-axis ranges, load color info

2. GGRS calls query_data_chunk(col_idx, row_idx)
   → Stream TSON chunks → Parse to Polars → Filter by .ci/.ri
   → Add .color column based on color mappings
   → Return quantized coords (.xs/.ys as i64)

3. GGRS dequantizes in render pipeline
   → Converts (.xs/.ys) to (.x/.y) using axis ranges
   → Renders with GPU (OpenGL) or CPU (Cairo)

4. Legend rendering (NEW!)
   → query_legend_scale() returns Continuous/Discrete/None
   → draw_legend() renders gradient or discrete keys
```

---

## Features

### Current Features (Version 0.0.2)

**Core Functionality**:
- ✅ Full plot pipeline: gRPC → TSON → Polars → GGRS → PNG
- ✅ GPU acceleration (OpenGL: 0.5s vs CPU: 3.1s for 475K points)
- ✅ Columnar architecture (Polars) - 10x+ performance vs row-by-row
- ✅ Chunked streaming with schema-based row limits
- ✅ Quantized coordinates (.xs/.ys uint16) with dequantization

**Faceting**:
- ✅ Row/column/grid faceting with independent Y-axes
- ✅ FacetSpec auto-detection from `.ci`/.ri` columns
- ✅ Facet labels from metadata tables (row.csv, column.csv)
- ✅ Per-facet Y-axis ranges from Y-axis table

**Colors**:
- ✅ **Continuous colors**: Numeric factors with palette interpolation
- ✅ **Categorical colors**: String factors with explicit color mappings
- ✅ **Level-based colors**: `.colorLevels` integer indices (0-7) with default palette
- ✅ Palette parsing from Tercen ColorList (ColorElement with numeric/string values)

**Configuration**:
- ✅ Property-based config from `operator.json`
- ✅ Auto plot dimensions (800px + (n_facets-1) × 400px, cap 4000px)
- ✅ Backend selection (cpu/gpu via properties)
- ✅ Point size (hardcoded 4, should come from crosstab aesthetics)

**Result Upload**:
- ✅ PNG encoding to base64
- ✅ Full Tercen model TSON format (matches Python toJson)
- ✅ FileService integration
- ✅ Task update with fileResultId

**Legend Rendering (Version 0.0.3 - In Progress)**:
- ✅ **GGRS legend infrastructure**: draw_continuous_legend(), draw_discrete_legend()
- ✅ **Theme-aware styling**: Extracts colors/sizes from theme elements
- ⚠️ **Partially working**: Renders when query_legend_scale() returns Continuous/Discrete
- ❌ **Blocked**: Need to load category names for `.colorLevels`-based colors

**Known Issues**:
- ❌ EventService logging disabled (returns UnimplementedError in production)
- ⚠️ Legend not showing for `.colorLevels` categorical colors (TODO in stream_generator.rs:1116-1120)

### Roadmap

**Version 0.0.3** (Current - Legend Support):
- ✅ GGRS legend rendering infrastructure
- 🚧 Load category names from color tables for `.colorLevels`-based legends
- 🎯 Test with continuous color workflow
- 🎯 Test with explicit categorical color mappings

**Version 0.0.4** (Future):
- Textual elements (axis labels, legend text, plot title)
- Manual axis ranges (override auto-computed ranges)
- Additional themes (minimal, white)

**Version 0.0.5** (Future):
- Additional output formats (SVG, PDF)
- Re-enable EventService logging when available
- Bulk streaming optimization for multi-facet (reduce per-facet data redundancy)

---

## Key Technical Decisions

### Columnar Architecture (CRITICAL!)

**Never build row-by-row structures. Always stay columnar.**

```rust
// ✅ GOOD: Columnar operations
let filtered = df.lazy()
    .filter(col(".ci").eq(lit(0)))
    .collect()?;

// ❌ BAD: Row-by-row iteration
for row in 0..df.height() {
    let record = build_record(df, row); // NO!
}
```

**Why**: 10x+ performance, lower memory, aligns with Polars/GGRS architecture.

### NO FALLBACK STRATEGIES

**Never implement fallback logic unless explicitly requested.**

```rust
// ❌ BAD: Fallback pattern
if data.has_column(".ys") {
    use_ys()
} else {
    use_y()  // Masks bugs!
}

// ✅ GOOD: Trust the specification
data.column(".ys")  // User said .ys exists
```

**Rationale**: Fallbacks mask bugs, add complexity, hurt performance. Only use for backward compatibility or user input validation.

### Memory Efficiency

- **Streaming**: Process in chunks (default: 10K rows), don't load entire table
- **Lazy Faceting**: Only load data for facet cells being rendered
- **Schema-Based Limiting**: Use table schema row count to prevent infinite loops
- **Quantized Coordinates**: Transmit 2 bytes/coordinate (uint16), dequantize on demand
- **Progressive Dequantization**: Process and discard chunks immediately

**Results**: 475K rows in 0.5s (GPU) or 3.1s (CPU), memory stable at 162MB (GPU) or 49MB (CPU).

### GPU Backend

- **Configuration**: Property `backend` = "cpu" or "gpu" in `operator.json`
- **OpenGL vs Vulkan**: OpenGL selected (162MB vs 314MB, 49% reduction)
- **Performance**: 10x speedup vs CPU for same quality
- **Trade-off**: 3.3x memory overhead acceptable for 10x speed

---

## Operator Properties

Properties defined in `operator.json`, read via `PropertyReader`:

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `plot.width` | String | `""` (auto) | Plot width in pixels or "auto" |
| `plot.height` | String | `""` (auto) | Plot height in pixels or "auto" |
| `backend` | Enum | `"cpu"` | Render backend: "cpu" or "gpu" |

**Auto Dimensions**: `800px + (n_facets - 1) × 400px`, capped at 4000px

**Implementation**: See `src/tercen/properties.rs` (PropertyReader, PlotDimension) and `src/config.rs` (OperatorConfig).

---

## Color Support

### Color Types

1. **Continuous Colors** (numeric factors):
   - Color palette with numeric stops (e.g., [0.0, 0.5, 1.0])
   - Interpolation between stops using `interpolate_color()`
   - Returns gradient colors based on normalized value

2. **Categorical Colors** (string factors):
   - Explicit color mappings: `{category → "#RRGGBB"}`
   - Parsed from Tercen ColorList (ColorElement with string values)
   - Direct lookup in CategoryColorMap

3. **Level-Based Colors** (`.colorLevels` integers):
   - Default palette (8 colors) when no explicit mappings
   - Level indices (0-7) map to palette positions
   - **Issue**: Category names not loaded → No legend (TODO for tomorrow!)

### Implementation Files

- `src/tercen/colors.rs`: Palette parsing, color interpolation, category mapping
- `src/ggrs_integration/stream_generator.rs`: Color column addition (`add_color_columns()`)
- Color data stored in `color_table_ids` (e.g., "Country" column table)

---

## Development Workflow

### Pre-Commit Checklist (MANDATORY!)

```bash
# 1. Format check
cargo fmt --check
cargo fmt

# 2. Lint (zero warnings required)
cargo clippy -- -D warnings

# 3. Build check
cargo build --profile dev-release

# 4. Test check
cargo test
```

**NEVER consider code complete until all checks pass.** CI will fail otherwise.

### Testing Workflow

**⚠️ CRITICAL: Use credentials from test_local.sh**

```bash
# Recommended: Use test script
./test_local.sh

# Manual testing
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAi..."
export WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c"
export STEP_ID="b9659735-27db-4480-b398-4e391431480f"
cargo run --profile dev-release --bin test_stream_generator
```

### Git Policy for Claude Code

- ❌ Never commit/push unless explicitly requested
- ✅ Run quality checks: fmt, clippy, build, test
- ✅ Use `git status` and `git diff` to show changes
- ✅ Stage with `git add` if requested
- ✅ Create commits only when user explicitly asks

**Default behavior: User handles commits/pushes manually.**

---

## Proto Files (Submodule)

Proto files managed via `tercen_grpc_api` submodule:
- Repository: https://github.com/tercen/tercen_grpc_api
- Files: `tercen.proto` (services), `tercen_model.proto` (data models)
- Setup: `git submodule update --init --recursive`
- Compiled at build time via `build.rs`

---

## Core Dependencies

```toml
tokio = "1.49"              # Async runtime
tonic = "0.14"              # gRPC client
prost = "0.14"              # Protobuf
polars = "0.51"             # Columnar DataFrame (CRITICAL!)
rustson = { git = "..." }   # TSON parsing
ggrs-core = { git = "https://github.com/tercen/ggrs", features = ["webgpu-backend", "cairo-backend"] }
thiserror = "1.0"           # Error macros
anyhow = "1.0"              # Error context
base64 = "0.22"             # PNG encoding
```

---

## Documentation

### Primary Docs (Read These First!)
- **CLAUDE.md** (this file) - Overview, current status, session logs
- **DEPLOYMENT_DEBUG.md** - Current issues and debugging
- **BUILD.md** - Build instructions
- **TEST_LOCAL.md** - Local testing procedures

### Supporting Docs
- `docs/09_FINAL_DESIGN.md` - Complete architecture
- `docs/10_IMPLEMENTATION_PHASES.md` - Implementation roadmap
- `docs/GPU_BACKEND_MEMORY.md` - GPU optimization analysis

### External Resources
- [Tercen gRPC API](https://github.com/tercen/tercen_grpc_api)
- [GGRS Library](https://github.com/tercen/ggrs)
- [Tercen C# Client](https://github.com/tercen/TercenCSharpClient) - Reference implementation

---

## Appendix: Historical Issue Logs

*Moved to separate files for brevity:*
- Result upload fixes → See git history (2025-01-09)
- Faceting implementation → See git history (2025-01-12)
- Operator properties → See git history (2025-01-14)
- Categorical colors → See git history (2025-01-14)
- **Legend rendering → THIS SESSION (2025-01-15)**
