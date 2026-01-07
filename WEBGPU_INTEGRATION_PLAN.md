# WebGPU Integration Implementation Plan

**Date**: January 7, 2026
**Goal**: Enable GPU-accelerated point rendering with transparent backend selection
**Strategy**: Create all new code in `_v2` files, only overwrite originals after approval

## Current State Assessment

### What EXISTS ✅
- `BackendChoice` enum (Cairo, WebGPU, SVG, Canvas2D)
- `OutputFormat` enum (PNG, SVG)
- `WebGPUBackend` complete implementation (384 lines)
  - `new()` - GPU initialization
  - `upload_points()` - vertex buffer upload
  - `render()` / `render_chunk()` - GPU rendering with streaming support
  - `copy_to_cpu()` - texture download
  - Vertex/fragment shaders in `renderer/shaders/`
- `PointRenderer` with both backends
  - `with_webgpu()` - GPU backend
  - `with_cairo()` - CPU backend
- Modular renderer trait system (`Renderer` trait)

### What DOES NOT EXIST ❌
- `ImageRenderer::builder()` method
- `ImageRendererBuilder` struct
- Integration of modular renderers into main `ImageRenderer`
- Compositing logic (GPU output + Cairo layers)

### Current Problem
- `ImageRenderer` in `render.rs` uses Cairo directly
- No way to switch backends at runtime
- 3.5s to render 475K points (single-threaded CPU)

## Target Architecture

```
Operator specifies: "PNG-GPU", "PNG-CPU", or "SVG"
         ↓
ImageRenderer::builder(plot_gen, width, height)
    .output_format(OutputFormat::Png)
    .points_gpu()  // or .points_cpu()
    .build()?
         ↓
ImageRenderer {
    backend_config: BackendConfig {
        output_format: OutputFormat::Png,
        points: BackendChoice::WebGPU,
        text: BackendChoice::Cairo,
        axes: BackendChoice::Cairo,
    }
}
         ↓
render_to_bytes() {
    // For PNG-GPU:
    1. Render points with WebGPU → RGBA texture
    2. Copy texture to Cairo surface
    3. Render axes/text/title with Cairo on same surface
    4. Encode to PNG

    // For PNG-CPU:
    1. Render everything with Cairo (current behavior)

    // For SVG:
    1. Render everything with SVG backend
}
```

## File Strategy

### New Files (Created, Never Overwrite Until Approved)
1. `ggrs/crates/ggrs-core/src/render_v2.rs` - New renderer with builder
2. `ggrs_plot_operator/src/bin/test_stream_generator_v2.rs` - Test with GPU
3. `ggrs_plot_operator/operator_config_v2.json` - Config with GPU options

### Untouched Files (Current Working Code)
1. `ggrs/crates/ggrs-core/src/render.rs` - **DO NOT TOUCH**
2. `ggrs_plot_operator/src/bin/test_stream_generator.rs` - **DO NOT TOUCH**
3. `ggrs_plot_operator/operator_config.json` - **DO NOT TOUCH**

### Modified Files (Safe Additions Only)
1. `ggrs/crates/ggrs-core/src/lib.rs` - Add `pub mod render_v2;` export
2. `ggrs_plot_operator/Cargo.toml` - Already has correct features, no changes

## Implementation Plan

### Phase 1: Create render_v2.rs with Builder (Safe Copy + Additions)

**Goal**: Create complete new renderer with builder pattern, test in isolation.

#### Step 1.1: Copy render.rs → render_v2.rs
```bash
cp /home/thiago/workspaces/tercen/main/ggrs/crates/ggrs-core/src/render.rs \
   /home/thiago/workspaces/tercen/main/ggrs/crates/ggrs-core/src/render_v2.rs
```

#### Step 1.2: Add BackendConfig to render_v2.rs
**File**: `ggrs/crates/ggrs-core/src/render_v2.rs`
**Location**: After existing imports, before `ImageRenderer` struct

```rust
use crate::renderer::backend::{BackendChoice, OutputFormat};

/// Configuration for which backend to use for each plot element
#[derive(Debug, Clone)]
struct BackendConfig {
    output_format: OutputFormat,
    points: BackendChoice,
    text: BackendChoice,
    axes: BackendChoice,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            output_format: OutputFormat::Png,
            points: BackendChoice::Cairo,
            text: BackendChoice::Cairo,
            axes: BackendChoice::Cairo,
        }
    }
}
```

#### Step 1.3: Add backend_config field to ImageRenderer
**File**: `ggrs/crates/ggrs-core/src/render_v2.rs`

```rust
pub struct ImageRenderer {
    generator: PlotGenerator,
    width: u32,
    height: u32,
    backend_config: BackendConfig,  // NEW

    #[cfg(feature = "webgpu-backend")]
    webgpu_backend: Option<crate::renderer::webgpu::WebGPUBackend>,  // NEW
}
```

Update `ImageRenderer::new()`:
```rust
pub fn new(generator: PlotGenerator, width: u32, height: u32) -> Self {
    Self {
        generator,
        width,
        height,
        backend_config: BackendConfig::default(),
        #[cfg(feature = "webgpu-backend")]
        webgpu_backend: None,
    }
}
```

#### Step 1.4: Add ImageRendererBuilder
**File**: `ggrs/crates/ggrs-core/src/render_v2.rs`
**Location**: After `ImageRenderer` impl block

```rust
/// Builder for ImageRenderer with configurable backends
pub struct ImageRendererBuilder {
    generator: PlotGenerator,
    width: u32,
    height: u32,
    backend_config: BackendConfig,
}

impl ImageRendererBuilder {
    pub fn new(generator: PlotGenerator, width: u32, height: u32) -> Self {
        Self {
            generator,
            width,
            height,
            backend_config: BackendConfig::default(),
        }
    }

    pub fn output_format(mut self, format: OutputFormat) -> Self {
        self.backend_config.output_format = format;
        self
    }

    #[cfg(feature = "webgpu-backend")]
    pub fn points_gpu(mut self) -> Self {
        if BackendChoice::WebGPU.is_available() {
            self.backend_config.points = BackendChoice::WebGPU;
        } else {
            eprintln!("Warning: WebGPU not available, using Cairo fallback");
        }
        self
    }

    pub fn points_cpu(mut self) -> Self {
        self.backend_config.points = BackendChoice::Cairo;
        self
    }

    pub fn build(self) -> Result<ImageRenderer> {
        // Validation
        if self.backend_config.output_format == OutputFormat::Svg {
            if self.backend_config.points != BackendChoice::Svg
                && self.backend_config.points != BackendChoice::Cairo {
                return Err(GgrsError::RenderError(
                    "SVG output requires SVG or Cairo backend".to_string()
                ));
            }
        }

        // Initialize WebGPU if needed
        #[cfg(feature = "webgpu-backend")]
        let webgpu_backend = if self.backend_config.points == BackendChoice::WebGPU {
            Some(
                pollster::block_on(crate::renderer::webgpu::WebGPUBackend::new(
                    self.width,
                    self.height,
                ))
                .map_err(|e| GgrsError::RenderError(format!("WebGPU init: {}", e)))?
            )
        } else {
            None
        };

        Ok(ImageRenderer {
            generator: self.generator,
            width: self.width,
            height: self.height,
            backend_config: self.backend_config,
            #[cfg(feature = "webgpu-backend")]
            webgpu_backend,
        })
    }
}

impl ImageRenderer {
    pub fn builder(generator: PlotGenerator, width: u32, height: u32) -> ImageRendererBuilder {
        ImageRendererBuilder::new(generator, width, height)
    }
}
```

**Verification**: `cargo build` in ggrs directory - does NOT affect operator yet.

---

### Phase 2: Implement GPU Rendering Methods in render_v2.rs

#### Step 2.1: Add GPU point rendering method
**File**: `ggrs/crates/ggrs-core/src/render_v2.rs`
**Location**: In `impl ImageRenderer` block

```rust
#[cfg(feature = "webgpu-backend")]
fn render_points_gpu(&mut self) -> Result<Vec<u8>> {
    let webgpu = self.webgpu_backend.as_mut()
        .ok_or_else(|| GgrsError::RenderError("WebGPU not initialized".to_string()))?;

    let stream_gen = self.generator.generator();
    let mut all_points = Vec::new();

    // Get preferred chunk size
    let chunk_size = stream_gen.preferred_chunk_size().unwrap_or(15000);

    // Collect points from all facets
    for row_idx in 0..self.generator.n_row_facets() {
        for col_idx in 0..self.generator.n_col_facets() {
            let total_rows = stream_gen.n_data_rows(col_idx, row_idx);
            let mut offset = 0;

            while offset < total_rows {
                let end = (offset + chunk_size).min(total_rows);
                let mut chunk_data = stream_gen.query_data_chunk(
                    col_idx,
                    row_idx,
                    crate::stream::Range::new(offset, end)
                );

                // Dequantize if needed (copy from existing render_to_bytes)
                if chunk_data.has_column(".xs") && chunk_data.has_column(".ys") {
                    let x_axis = stream_gen.query_x_axis(col_idx, row_idx);
                    let x_range = match x_axis {
                        crate::stream::AxisData::Numeric(ref data) => (data.min_axis, data.max_axis),
                        _ => return Err(GgrsError::RenderError("X-axis must be numeric".to_string())),
                    };

                    let y_axis = stream_gen.query_y_axis(col_idx, row_idx);
                    let y_range = match y_axis {
                        crate::stream::AxisData::Numeric(ref data) => (data.min_axis, data.max_axis),
                        _ => return Err(GgrsError::RenderError("Y-axis must be numeric".to_string())),
                    };

                    let mut y_ranges = std::collections::HashMap::new();
                    y_ranges.insert(row_idx, y_range);

                    chunk_data = crate::stream::dequantize_chunk(chunk_data, x_range, &y_ranges)?;
                }

                // Extract coordinates
                let x_col = crate::data::column_as_f64(&chunk_data, ".x")?;
                let y_col = crate::data::column_as_f64(&chunk_data, ".y")?;

                // Get axis ranges for coordinate transformation
                let x_axis = stream_gen.query_x_axis(col_idx, row_idx);
                let y_axis = stream_gen.query_y_axis(col_idx, row_idx);

                let (x_min, x_max) = match x_axis {
                    crate::stream::AxisData::Numeric(ref data) => (data.min_axis, data.max_axis),
                    _ => (0.0, 1.0),
                };

                let (y_min, y_max) = match y_axis {
                    crate::stream::AxisData::Numeric(ref data) => (data.min_axis, data.max_axis),
                    _ => (0.0, 1.0),
                };

                // Convert to GPU vertices
                for i in 0..chunk_data.nrow() {
                    if let (Some(x), Some(y)) = (x_col.get(i), y_col.get(i)) {
                        // Normalize to [0, 1]
                        let x_norm = (x - x_min) / (x_max - x_min);
                        let y_norm = (y - y_min) / (y_max - y_min);

                        // Convert to NDC [-1, 1]
                        let x_ndc = x_norm * 2.0 - 1.0;
                        let y_ndc = 1.0 - (y_norm * 2.0 - 1.0); // Flip Y

                        all_points.push(crate::renderer::webgpu::PointVertex {
                            position_x: x_ndc as f32,
                            position_y: y_ndc as f32,
                            color_r: 0.0,
                            color_g: 0.0,
                            color_b: 0.0,
                            color_a: 1.0,
                            radius: 2.0,
                            _padding: 0.0,
                        });
                    }
                }

                offset = end;
            }
        }
    }

    eprintln!("Uploading {} points to GPU...", all_points.len());
    webgpu.upload_points(&all_points);

    eprintln!("Rendering on GPU...");
    webgpu.render().map_err(|e| GgrsError::RenderError(e))?;

    eprintln!("Copying from GPU to CPU...");
    let gpu_rgba = pollster::block_on(webgpu.copy_to_cpu())
        .map_err(|e| GgrsError::RenderError(e))?;

    eprintln!("GPU rendering complete, got {} bytes", gpu_rgba.len());

    // For now, just convert RGBA to PNG directly (Phase 3 will add Cairo compositing)
    self.rgba_to_png(&gpu_rgba)
}

/// Convert RGBA buffer to PNG (temporary, will be replaced by Cairo compositing)
fn rgba_to_png(&self, rgba: &[u8]) -> Result<Vec<u8>> {
    use png::{BitDepth, ColorType, Encoder};

    let mut png_data = Vec::new();
    {
        let mut encoder = Encoder::new(&mut png_data, self.width, self.height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);

        let mut writer = encoder.write_header()
            .map_err(|e| GgrsError::RenderError(format!("PNG header: {}", e)))?;

        writer.write_image_data(rgba)
            .map_err(|e| GgrsError::RenderError(format!("PNG write: {}", e)))?;
    }

    Ok(png_data)
}
```

#### Step 2.2: Add CPU rendering method (delegates to existing code)
```rust
fn render_points_cpu(&self) -> Result<Vec<u8>> {
    // Just call existing render_to_bytes() implementation
    self.render_to_bytes()
}
```

#### Step 2.3: Add new public render method
```rust
/// Render to PNG bytes with mutable access (required for GPU)
#[cfg(feature = "webgpu-backend")]
pub fn render_to_bytes_mut(&mut self) -> Result<Vec<u8>> {
    match self.backend_config.points {
        BackendChoice::WebGPU => self.render_points_gpu(),
        BackendChoice::Cairo => self.render_points_cpu(),
        _ => Err(GgrsError::RenderError(
            format!("Unsupported backend: {:?}", self.backend_config.points)
        )),
    }
}

/// Render to file with mutable access (required for GPU)
#[cfg(all(feature = "cairo-backend", feature = "webgpu-backend"))]
pub fn render_to_file_mut<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
    let png_data = self.render_to_bytes_mut()?;
    std::fs::write(path, png_data)
        .map_err(|e| GgrsError::RenderError(format!("File write: {}", e)))?;
    Ok(())
}
```

**Verification**: `cargo build --features webgpu-backend,cairo-backend` in ggrs directory.

---

### Phase 3: Create Test Binary v2

#### Step 3.1: Copy and modify test binary
```bash
cp /home/thiago/workspaces/tercen/main/ggrs_plot_operator/src/bin/test_stream_generator.rs \
   /home/thiago/workspaces/tercen/main/ggrs_plot_operator/src/bin/test_stream_generator_v2.rs
```

#### Step 3.2: Update imports
**File**: `src/bin/test_stream_generator_v2.rs`

```rust
use ggrs_core::render_v2::{ImageRenderer, ImageRendererBuilder};  // v2!
use ggrs_core::renderer::backend::OutputFormat;
```

#### Step 3.3: Update rendering section
**File**: `src/bin/test_stream_generator_v2.rs`
**Find**: The section that creates `ImageRenderer`
**Replace with**:

```rust
log_phase(start, "PHASE 5.1: Creating PlotGenerator");
println!("Creating plot generator...");
let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;

log_phase(start, "PHASE 5.2: Creating ImageRenderer with GPU");
println!("Creating image renderer with GPU acceleration...");

let mut renderer = ImageRenderer::builder(
    plot_gen,
    config.default_plot_width,
    config.default_plot_height,
)
.output_format(OutputFormat::Png)
.points_gpu()  // Enable GPU!
.build()?;

println!("  Backend: GPU-accelerated points + Cairo text/axes");
println!("  Dimensions: {}x{}", config.default_plot_width, config.default_plot_height);

log_phase(start, "PHASE 5.3: Rendering plot with GPU");
println!("Rendering plot...");
renderer.render_to_file_mut("plot_v2.png")?;  // Different filename!

log_phase(start, "PHASE 5.4: Saving PNG");
println!("Saving plot to plot_v2.png...");
let file_size = std::fs::metadata("plot_v2.png")?.len();
println!("✓ Plot saved to plot_v2.png ({} bytes)", file_size);
```

#### Step 3.4: Register binary in Cargo.toml
**File**: `Cargo.toml`
**Add**:

```toml
[[bin]]
name = "test_stream_generator_v2"
path = "src/bin/test_stream_generator_v2.rs"
```

**Verification**: `cargo build --bin test_stream_generator_v2 --features webgpu-backend,cairo-backend`

---

### Phase 4: Export render_v2 Module

#### Step 4.1: Add module export
**File**: `ggrs/crates/ggrs-core/src/lib.rs`
**Add line**:

```rust
pub mod render_v2;  // GPU-accelerated renderer
```

**Verification**: Operator can now import from `ggrs_core::render_v2`.

---

### Phase 5: Test GPU Rendering

#### Step 5.1: Create test script
**File**: `test_local_v2.sh`

```bash
#!/bin/bash

echo "============================================"
echo "Testing GPU-accelerated renderer (v2)"
echo "============================================"

# Clean old output
rm -f plot_v2.png target/debug/test_stream_generator_v2

# Build with GPU support
echo "Building with WebGPU support..."
cargo build --bin test_stream_generator_v2 --features webgpu-backend

# Run test
echo "Running GPU test..."
TERCEN_URI="http://127.0.0.1:50051" \
TERCEN_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9..." \
WORKFLOW_ID="28e3c9888e9935f667aed6f07c007c7c" \
STEP_ID="b9659735-27db-4480-b398-4e391431480f" \
./target/debug/test_stream_generator_v2

# Check output
if [ -f "plot_v2.png" ]; then
    echo "✓ GPU rendering successful!"
    ls -lh plot_v2.png
else
    echo "✗ GPU rendering failed - no output"
    exit 1
fi
```

```bash
chmod +x test_local_v2.sh
```

#### Step 5.2: Run test
```bash
./test_local_v2.sh
```

**Expected Output**:
- GPU initialization messages
- Point upload to GPU
- GPU rendering
- Output: `plot_v2.png` (may have only points, no axes yet)

**Verification Points**:
- Does GPU initialize without errors?
- Are points uploaded?
- Does rendering complete?
- Is PNG generated?
- **Compare timing**: GPU should be faster than Cairo

---

## Testing Strategy

### Test Sequence

#### Test 1: Verify Original Still Works
```bash
./test_local.sh
# Uses test_stream_generator.rs (original)
# Should work exactly as before: ~4s, full plot
```

#### Test 2: Verify v2 Compiles
```bash
cargo build --bin test_stream_generator_v2 --features webgpu-backend,cairo-backend
# Should compile without errors
```

#### Test 3: Test GPU Rendering
```bash
./test_local_v2.sh
# Uses test_stream_generator_v2.rs (GPU version)
# Expected: Faster rendering, may have incomplete axes (Phase 2 limitation)
```

#### Test 4: Compare Outputs
```bash
# Original (Cairo-only)
ls -lh plot.png
identify plot.png

# GPU version
ls -lh plot_v2.png
identify plot_v2.png

# Should both be 800x600 PNG
# GPU version may lack axes/labels initially
```

---

## Phase 6: Cairo Compositing (If Needed)

**Only if Phase 5 shows points-only output without axes/text.**

This phase would add method `composite_with_cairo()` that:
1. Takes GPU RGBA buffer
2. Creates Cairo surface
3. Copies GPU pixels
4. Renders axes/text/title on top
5. Returns composited PNG

We'll assess if this is needed after Phase 5 testing.

---

## Success Criteria

✅ **Phase 1**: `render_v2.rs` compiles, builder works
✅ **Phase 2**: GPU rendering methods compile
✅ **Phase 3**: `test_stream_generator_v2` compiles
✅ **Phase 4**: Module exported, operator can import
✅ **Phase 5**: GPU test runs, `plot_v2.png` generated

**Final Success**:
- Original test (`./test_local.sh`) still works: ~4s
- GPU test (`./test_local_v2.sh`) works: <1s
- Both produce valid plots

## Approval Checkpoints

**After each phase, I will ask for approval before proceeding:**

1. ✋ After Phase 1: "render_v2.rs created with builder, ready for Phase 2?"
2. ✋ After Phase 2: "GPU rendering methods added, ready for Phase 3?"
3. ✋ After Phase 3: "test_stream_generator_v2 created, ready for Phase 4?"
4. ✋ After Phase 4: "Module exported, ready for Phase 5 testing?"
5. ✋ After Phase 5: "GPU test results - proceed to Phase 6 or approve v2?"

## Rollback Strategy

**Nothing to rollback** - all new code is in separate files:
- If v2 doesn't work: Delete v2 files, keep using originals
- If v2 works: I'll ask for approval to overwrite originals

## Files Created (Never Touches Originals)

1. ✅ `ggrs/crates/ggrs-core/src/render_v2.rs`
2. ✅ `ggrs_plot_operator/src/bin/test_stream_generator_v2.rs`
3. ✅ `ggrs_plot_operator/test_local_v2.sh`

## Files Modified (Safe Additions Only)

1. `ggrs/crates/ggrs-core/src/lib.rs` - Add `pub mod render_v2;`
2. `ggrs_plot_operator/Cargo.toml` - Add test_stream_generator_v2 binary

## Files NEVER Modified

1. ❌ `ggrs/crates/ggrs-core/src/render.rs`
2. ❌ `ggrs_plot_operator/src/bin/test_stream_generator.rs`
3. ❌ `ggrs_plot_operator/test_local.sh`
4. ❌ `ggrs_plot_operator/operator_config.json`
5. ❌ All `src/tercen/` files
6. ❌ `src/ggrs_integration/stream_generator.rs`

---

**Ready to start Phase 1?** Say "Yes, start Phase 1" and I'll create `render_v2.rs`.
