# CLAUDE.md

This file provides guidance to Claude Code when working with this repository.

## Project Overview

**ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with Tercen's gRPC API. It receives tabular data, generates high-performance plots, and returns PNG images for visualization.

**Version**: 0.0.2 (Faceting Support + Operator Properties)

**Status**: ✅ Production-ready (logging disabled due to EventService issue)

## Current Status

**What's Working**:
- ✅ Full plot pipeline: gRPC → TSON → Polars → GGRS → PNG
- ✅ GPU acceleration (OpenGL: 0.5s vs CPU: 3.1s for 475K rows)
- ✅ Faceting with independent Y-axes (row/column/grid)
- ✅ Property-based config (plot.width, plot.height with "auto", backend)
- ✅ **Continuous color support** (numeric color factors with palette interpolation)
- ✅ Result upload with Tercen model format
- ✅ CI/CD release workflow

**Known Issues**:
- ❌ EventService logging disabled (returns UnimplementedError)
- See `DEPLOYMENT_DEBUG.md` for details

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

See `BUILD.md`, `TEST_LOCAL.md` for comprehensive instructions.

### Quick Debugging

- **Not connecting?** Check `TERCEN_URI`, `TERCEN_TOKEN` env vars
- **Build failing?** Run `cargo clean && cargo build --profile dev-release`
- **Faceting issues?** Verify `.ci`/`.ri` columns, check facet metadata tables

## Architecture

### Module Structure

```
src/
├── main.rs                      # Entry point (logging disabled)
├── config.rs                    # Property-based configuration
├── tercen/                      # Pure Tercen gRPC client
│   ├── client.rs               # TercenClient with auth
│   ├── table.rs                # TableStreamer (chunked)
│   ├── tson_convert.rs         # TSON → Polars (columnar)
│   ├── facets.rs               # Facet metadata loading
│   ├── properties.rs           # PropertyReader, PlotDimension
│   ├── colors.rs               # Color palette parsing & interpolation
│   ├── result.rs               # Result upload
│   └── error.rs                # TercenError types
├── ggrs_integration/
│   └── stream_generator.rs     # TercenStreamGenerator (GGRS trait)
└── bin/
    └── test_stream_generator.rs # Test binary
```

### Three-Layer Design

1. **gRPC Client** (`src/tercen/`): TercenClient, TableStreamer, services (tonic, prost, tokio)
2. **Data Transform** (Columnar!): TSON → Polars DataFrame (NO row-by-row processing)
3. **GGRS Integration**: TercenStreamGenerator, lazy facet loading, GPU rendering

### Data Flow

```
1. TercenStreamGenerator::new()
   → Connect via gRPC
   → Load facet metadata (row.csv, column.csv)
   → Load/compute Y-axis ranges

2. GGRS calls query_data_chunk(col_idx, row_idx)
   → Stream TSON chunks (offset + limit)
   → Parse TSON → Polars DataFrame (columnar!)
   → Filter: .ci == col_idx AND .ri == row_idx
   → Return quantized coords (.xs/.ys as i64)

3. GGRS dequantizes in render pipeline
   → Formula: value = (quantized / 65535) × (max - min) + min
   → Creates .x/.y columns with actual values

4. GGRS renders → PNG
   → GPU (OpenGL): 0.5s, 162 MB
   → CPU (Cairo): 3.1s, 49 MB

5. Upload to Tercen
   → Encode PNG to base64
   → Create result table (.content, filename, mimetype, plot_width, plot_height)
   → Upload via TableSchemaService with full Tercen model format
```

### Data Structure

**Main data** (TSON):
```
.ci, .ri, .xs, .ys, sp, ...
0,   0,   12845, 15632, "B", ...
```
- `.ci`/`.ri`: Facet indices (i64)
- `.xs`/`.ys`: Quantized coords (uint16 as i64, range 0-65535)

**Facet metadata**: `column.csv`, `row.csv` with factor values

## Operator Properties

Defined in `operator.json`:

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `plot.width` | String | `""` (auto) | Width in pixels or "auto" (derives from col facets) |
| `plot.height` | String | `""` (auto) | Height in pixels or "auto" (derives from row facets) |
| `backend` | Enum | `"cpu"` | Render backend: "cpu" (Cairo) or "gpu" (OpenGL) |

**Auto dimensions**: `800px + (n_facets - 1) × 400px`, capped at 4000px

**Example**: 1 facet → 800px, 4 facets → 2000px, 10+ facets → 4000px

**Note**: Point size hardcoded (4) - should come from crosstab aesthetics in future.

## Color Support (Version 0.0.2)

### Overview

The operator supports **continuous color mapping** from numeric color factors to RGB colors using palette interpolation.

### Architecture

**Color Pipeline**:
```
1. Extract color info from workflow
   → WorkflowService.get(workflow_id)
   → Find step.model.axis.xyAxis[0].colors
   → Parse palette (JetPalette, RampPalette)

2. Stream color data alongside coordinates
   → Include color factor column (e.g., "Age") in streaming request
   → Raw f64 values (8 bytes per value)

3. Map values to RGB using palette interpolation
   → Binary search for surrounding color stops
   → Linear interpolation: rgb = (1-t)×lower + t×upper
   → Convert to hex strings (#FFFFFF format)

4. Pass to GGRS
   → Add .color aesthetic conditionally
   → GGRS renders points with interpolated colors
```

### Implementation Details

**Module**: `src/tercen/colors.rs` (323 lines)

**Core Types**:
```rust
pub struct ColorInfo {
    pub factor_name: String,      // e.g., "Age"
    pub factor_type: String,       // e.g., "double"
    pub palette: ColorPalette,
}

pub struct ColorPalette {
    pub stops: Vec<ColorStop>,     // Sorted by value
}

pub struct ColorStop {
    pub value: f64,
    pub color: [u8; 3],            // RGB
}
```

**Key Functions**:
- `extract_color_info_from_step()`: Extract color factors and palettes from workflow
- `parse_palette()`: Convert Tercen EPalette (JetPalette, RampPalette) to ColorPalette
- `interpolate_color()`: Linear interpolation between color stops
- `int_to_rgb()`: Convert AARRGGBB (32-bit) to RGB bytes

**Color Format**:
- Tercen stores colors as 32-bit integers: AARRGGBB (alpha-red-green-blue)
- Operator converts to hex strings: `#FFFFFF` (GGRS requirement)
- Missing values default to gray: `#808080`

### Usage in Stream Generator

**Location**: `src/ggrs_integration/stream_generator.rs`

```rust
// Store color info
color_infos: Vec<ColorInfo>,

// Constructor
pub async fn new(
    // ... other params ...
    color_infos: Vec<ColorInfo>,
) -> Result<Self>

// Add color aesthetic conditionally
let mut aes = Aes::new().x(".x").y(".y");
if !color_infos.is_empty() {
    aes = aes.color(".color");
}

// Stream color column alongside coordinates
let mut columns = vec![".ci", ".ri", ".xs", ".ys"];
for color_info in &self.color_infos {
    columns.push(color_info.factor_name.clone());
}

// Add .color column with hex strings
fn add_color_columns(&self, df: DataFrame) -> Result<DataFrame> {
    // Extract f64 values
    // Interpolate to RGB using palette
    // Convert to hex strings (#FFFFFF)
    // Add .color column
}
```

### Limitations

1. **Single Color Factor**: Only first color factor used if multiple exist
   - GGRS currently supports single color aesthetic
   - Future: Map to size, alpha, or other aesthetics

2. **Continuous Colors Only**: Categorical colors not yet implemented
   - `.colorLevels` column not supported
   - `CategoryPalette` type not handled
   - Future: Version 0.0.4

3. **No Color Legend**: Plot doesn't include color scale legend yet
   - Future: Add legend showing color-to-value mapping

4. **No Color Optimization**: Color values sent as raw f64 (8 bytes)
   - X/Y use quantization (2 bytes)
   - Color quantization not available in Tercen data format

### Performance

**Test Dataset**: 475,688 rows with "Age" color factor (9.5 to 60.5)

**Results**:
- Processing time: 12.6 seconds (< 5% overhead)
- Peak memory: 138 MB
- Throughput: ~37,700 points/second
- Color interpolation: < 0.1s

**Impact**: Minimal overhead from color support

### Testing

**Unit Tests** (`src/tercen/colors.rs`):
- Palette parsing (JetPalette, RampPalette)
- Color interpolation (in-range, edge cases, out-of-bounds)
- int_to_rgb conversion

**Integration Test** (`./test_local.sh`):
- Workflow: 28e3c9888e9935f667aed6f07c007c7c
- Color factor: "Age" (numeric)
- Output: plot.png with colored points

All tests passing.

### Implementation

```rust
// Extract from task
let (cube_query, project_id, namespace, operator_settings) = extract_cube_query(&task)?;

// Create config (uses defaults if None)
let config = OperatorConfig::from_properties(operator_settings.as_ref());

// Resolve "auto" after knowing facet counts
let (plot_width, plot_height) = config.resolve_dimensions(
    stream_gen.n_col_facets(),
    stream_gen.n_row_facets(),
);
```

## Key Technical Decisions

### Columnar Architecture (CRITICAL!)

**Never build row-by-row structures. Always stay columnar.**

✅ **DO**: Use Polars lazy API, `vstack_mut()`, zero-copy operations
❌ **DON'T**: Build `Vec<Record>` or iterate rows

**Why**: 10x+ performance, lower memory usage

### NO FALLBACK STRATEGIES

**Never add fallback logic unless explicitly requested.**

```rust
// ❌ BAD: Fallback pattern
if data.has_column(".ys") { use_ys() } else { use_y() }

// ✅ GOOD: Trust the specification
data.column(".ys")  // User said .ys exists
```

**Rationale**: Fallbacks mask bugs, add complexity, hurt performance

**Only use fallbacks for**:
1. User-requested backward compatibility
2. Error recovery at system boundaries (user input validation)

### Memory Efficiency

- **Streaming**: Process in chunks (default: 10K rows)
- **Lazy Faceting**: Only load data for rendered facet cells
- **Quantized Coords**: Transmit 2 bytes/coord, dequantize on demand
- **Schema-Based Limiting**: Use row count to prevent infinite loops

**Results**: 475K rows in 0.5s (GPU), memory stable at 162MB

### GPU Backend

- OpenGL selected over Vulkan (162 MB vs 314 MB, 49% reduction)
- 10x speedup with 3.3x memory overhead (acceptable trade-off)
- Property `backend` in `operator.json`: "cpu" or "gpu"

## Development Workflow

### Pre-Commit Checklist (MANDATORY!)

```bash
cargo fmt --check          # Must pass
cargo fmt                  # Apply formatting
cargo clippy -- -D warnings # Zero warnings required
cargo build --profile dev-release
cargo test
```

**NEVER consider code complete until all checks pass.**

### Testing

**⚠️ CRITICAL: ALWAYS use credentials from test_local.sh**

```bash
# Recommended
./test_local.sh

# Manual
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="eyJ0eXAi..."
export WORKFLOW_ID="workflow_id"
export STEP_ID="step_id"
cargo run --profile dev-release --bin test_stream_generator
```

### Git Policy

❌ Never commit/push without explicit user request
✅ Run quality checks
✅ Use `git status`, `git diff` to show changes
✅ Create commits only when user explicitly asks

## Proto Files (Submodule)

**Important**: Proto files are in `tercen_grpc_api` submodule (NOT copied locally)

- Repository: https://github.com/tercen/tercen_grpc_api
- Files: `tercen.proto`, `tercen_model.proto`
- Setup: `git submodule update --init --recursive`
- Compiled via `build.rs` at build time

## Core Dependencies

```toml
tokio = "1.49"              # Async runtime
tonic = "0.14"              # gRPC client
prost = "0.14"              # Protobuf serialization
polars = "0.51"             # Columnar DataFrame operations
ggrs-core = { git = "https://github.com/tercen/ggrs", features = ["webgpu-backend", "cairo-backend"] }
rustson = { git = "..." }   # TSON parsing
thiserror = "1.0"           # Error derive macros
anyhow = "1.0"              # Error context
base64 = "0.22"             # PNG encoding
```

## Roadmap

**Version 0.0.2** (COMPLETE):
- ✅ Multi-facet scatter plots (row/column/grid with FreeY scales)
- ✅ Property-based config (auto plot dimensions)
- ✅ GPU/CPU backend switching
- ✅ **Continuous color support** (numeric color factors with palette interpolation)

**Version 0.0.3** (Future):
- 🎯 Plot legend (including color scale legend)
- 🎯 Categorical color support (ColorLevels column)
- 🎯 Minimal/white themes
- 🎯 Optimize bulk streaming for multi-facet

**Version 0.0.4** (Future):
- Textual elements (axis labels, legend, title)
- Manual axis ranges
- SVG, PDF output formats

## Documentation

**Primary**:
- `DEPLOYMENT_DEBUG.md` - Current issues and workarounds
- `docs/09_FINAL_DESIGN.md` - Complete architecture
- `docs/10_IMPLEMENTATION_PHASES.md` - Implementation roadmap
- `BUILD.md` - Build guide
- `TEST_LOCAL.md` - Testing procedures

**External**:
- [Tercen gRPC API](https://github.com/tercen/tercen_grpc_api)
- [GGRS Library](https://github.com/tercen/ggrs)
