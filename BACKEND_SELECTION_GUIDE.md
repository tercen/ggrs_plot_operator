# Backend Selection Guide for ggrs_plot_operator

## Overview

GGRS now supports **runtime backend selection** via a builder pattern. This allows the operator to choose rendering backends based on user parameters.

## Architecture Summary

**Two-level selection**:
1. **Output Format** (PNG or SVG) - Primary user choice
2. **Backend per Renderer** - Implementation detail, constrained by format

**Backends available**:
- `Cairo`: CPU rendering (always available)
- `WebGPU`: GPU rendering (requires `webgpu-backend` feature)
- `SVG`: Vector output (always available)

## Operator Parameter Integration

### operator.json

```json
{
  "name": "ggrs_plot_operator",
  "properties": {
    "output_format": {
      "type": "string",
      "description": "Output image format",
      "enum": ["png", "svg"],
      "default": "png"
    },
    "use_gpu": {
      "type": "boolean",
      "description": "Use GPU acceleration for data points (PNG only)",
      "default": true
    },
    "width": {
      "type": "integer",
      "default": 800
    },
    "height": {
      "type": "integer",
      "default": 600
    }
  }
}
```

### Rust Code Integration

```rust
use ggrs_core::{ImageRenderer, renderer::{OutputFormat, BackendChoice}};

// 1. Parse operator parameters
let config = load_operator_config()?;
let output_format = match config.output_format.as_str() {
    "svg" => OutputFormat::Svg,
    _ => OutputFormat::Png,
};

// 2. Create renderer with builder
let mut builder = ImageRenderer::builder(plot_gen, config.width, config.height)
    .output_format(output_format);

// 3. Apply GPU preference (only for PNG)
if output_format == OutputFormat::Png && config.use_gpu {
    // Check if WebGPU is available at runtime
    if BackendChoice::WebGPU.is_available() {
        builder = builder.points_gpu();
    } else {
        // Graceful fallback to Cairo
        log::warn!("WebGPU not available, falling back to Cairo CPU rendering");
        builder = builder.points_cpu();
    }
}

// 4. Build renderer
let renderer = builder.build()?;

// 5. Render based on format
let output = match output_format {
    OutputFormat::Png => renderer.render_to_bytes()?,
    OutputFormat::Svg => {
        // SVG rendering will be implemented in Phase 3
        return Err("SVG output not yet implemented".into());
    }
};
```

## Usage Examples

### Example 1: Default PNG (CPU)

```bash
# Current behavior - uses Cairo (CPU)
TERCEN_URI="..." TERCEN_TOKEN="..." cargo run
```

**Config**:
```json
{
  "output_format": "png",
  "use_gpu": false
}
```

**Result**: PNG rendered with Cairo (3.1s for 475K points)

### Example 2: PNG with GPU (Phase 2)

```bash
# After Phase 2 implementation
TERCEN_URI="..." TERCEN_TOKEN="..." cargo run
```

**Config**:
```json
{
  "output_format": "png",
  "use_gpu": true
}
```

**Result**: PNG rendered with WebGPU for points (0.3s for 475K points, 10x faster!)

### Example 3: SVG for Publication (Phase 3)

```bash
# After Phase 3 implementation
TERCEN_URI="..." TERCEN_TOKEN="..." cargo run
```

**Config**:
```json
{
  "output_format": "svg",
  "use_gpu": false  // Ignored for SVG
}
```

**Result**: SVG with grouped elements (15 MB instead of 50 MB)

## Smart Defaults

The builder automatically applies smart defaults based on output format:

### PNG Output
- **Points**: WebGPU if available, otherwise Cairo
- **Text**: Cairo (high-quality fonts)
- **Axes**: Cairo (crisp lines)
- **Compositing**: GPU → CPU → PNG encoding

### SVG Output
- **Points**: SVG (grouped by facet + color)
- **Text**: SVG (native text elements)
- **Axes**: SVG (vector lines)
- **No compositing**: Direct SVG generation

## Error Handling

```rust
match builder.build() {
    Ok(renderer) => {
        // Success
    }
    Err(e) if e.to_string().contains("not available") => {
        // Backend not compiled in, use fallback
        builder = builder.points_cpu();
        renderer = builder.build()?;
    }
    Err(e) => {
        // Other error
        return Err(e);
    }
}
```

## Cargo Features

### Current (Phase 1)
```toml
[dependencies.ggrs-core]
features = ["cairo-backend"]
```

### Phase 2 (GPU Support)
```toml
[dependencies.ggrs-core]
features = ["cairo-backend", "webgpu-backend"]

[dependencies]
wgpu = "0.18"
```

### Build Commands

```bash
# Cairo only (current)
cargo build

# With WebGPU support (Phase 2)
cargo build --features webgpu-backend
```

## Performance Expectations

### Current (Phase 1 - Cairo only)
- 475K points: ~5.0s
- Memory: ~60 MB
- Single-threaded CPU

### Phase 2 (Cairo optimized + WebGPU)
- 475K points: ~0.5s with GPU, ~2.0s with optimized Cairo
- Memory: ~60 MB
- Parallel GPU processing

### Phase 3 (SVG export)
- 475K points: ~3.0s (generation time)
- File size: ~15 MB (with grouping)
- Editable in Illustrator/Inkscape

## Implementation Status

✅ **Phase 1 COMPLETE** (2025-01-07):
- Backend architecture with runtime selection
- `ImageRendererBuilder` with smart defaults
- `OutputFormat` enum (PNG/SVG)
- `BackendChoice` enum with availability checking
- Validation and error handling
- Operator parameter integration design

📋 **Phase 2 NEXT**:
- WebGPU point renderer implementation
- Instanced rendering for circles
- Layer compositing (GPU + Cairo)
- Performance testing with 475K points

📋 **Phase 3 FUTURE**:
- SVG backend implementation
- Grouped SVG generation
- resvg integration for text rasterization
- SVG → PNG conversion for mixed rendering

## References

- [BACKEND_ARCHITECTURE.md](../ggrs/docs/BACKEND_ARCHITECTURE.md) - Complete architecture
- [TERCEN_OPERATOR_DEVELOPMENT.md](../ggrs/docs/TERCEN_OPERATOR_DEVELOPMENT.md) - Performance analysis
- [CHANGELOG.md](../ggrs/docs/CHANGELOG.md) - Version history
