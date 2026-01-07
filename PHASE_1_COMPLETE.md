# Phase 1 Complete: render_v2.rs with Builder Pattern

## Status: ✅ COMPLETE - Ready for Phase 2

All Phase 1 objectives achieved:
- ✅ Created `render_v2.rs` as copy of `render.rs`
- ✅ Added `BackendConfig` struct with default implementation
- ✅ Updated `ImageRenderer` struct with backend fields
- ✅ Created `ImageRendererBuilder` with builder pattern
- ✅ Fixed module visibility (use public re-exports)
- ✅ Exported `render_v2` module in `lib.rs`
- ✅ **Compilation successful** with cairo-backend and webgpu-backend features
- ✅ Only minor clippy warnings (unused imports/variables)

## Files Modified

### `/home/thiago/workspaces/tercen/main/ggrs/crates/ggrs-core/src/render_v2.rs` (NEW)
- Copied from `render.rs` (2588 lines)
- **Lines 12**: Changed import from `crate::renderer::backend::*` to `crate::renderer::*`
- **Lines 258-276**: Added `BackendConfig` struct:
  ```rust
  #[derive(Debug, Clone)]
  struct BackendConfig {
      output_format: OutputFormat,
      points: BackendChoice,
      text: BackendChoice,
      axes: BackendChoice,
  }
  ```
- **Lines 285-289**: Added backend fields to `ImageRenderer`:
  ```rust
  backend_config: BackendConfig,
  #[cfg(feature = "webgpu-backend")]
  webgpu_backend: Option<crate::renderer::WebGPUBackend>,
  ```
- **Lines 293-302**: Updated `new()` to initialize backend fields
- **Lines 305-307**: Added `builder()` method
- **Lines 2506-2587**: Added `ImageRendererBuilder` implementation:
  - `.new()`: Constructor
  - `.output_format()`: Set output format (PNG/SVG)
  - `.points_gpu()`: Enable GPU for points (conditional feature)
  - `.points_cpu()`: Use Cairo for points
  - `.build()`: Validate config and initialize WebGPU if needed

### `/home/thiago/workspaces/tercen/main/ggrs/crates/ggrs-core/src/lib.rs`
- **Line 19**: Added `pub mod render_v2;` (between `render` and `renderer`)

## Usage Example

```rust
use ggrs_core::render_v2::ImageRenderer;
use ggrs_core::renderer::{BackendChoice, OutputFormat};

// Legacy: Cairo for everything (unchanged behavior)
let renderer = ImageRenderer::new(plot_gen, 800, 600);

// New: Builder pattern with GPU acceleration
let renderer = ImageRenderer::builder(plot_gen, 800, 600)
    .output_format(OutputFormat::Png)
    .points_gpu()  // GPU for points (if available)
    .build()?;
```

## Build Verification

```bash
cd /home/thiago/workspaces/tercen/main/ggrs/crates/ggrs-core
cargo build --features "cairo-backend,webgpu-backend"
# ✅ Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.18s
```

## Next Steps (Phase 2)

Phase 2 objectives:
1. Implement GPU rendering methods in `ImageRenderer`:
   - `render_points_gpu()`: Upload points to GPU, render, copy back to Cairo
   - `composite_gpu_to_cairo()`: Composite GPU texture onto Cairo surface
2. Modify `render_cell()` to check `backend_config.points`:
   - If `BackendChoice::WebGPU` → use GPU rendering path
   - Otherwise → use existing Cairo path (unchanged)
3. Keep ALL existing Cairo rendering code intact
4. Only add new GPU code paths, never modify working code

## Technical Details

### Module Visibility Fix
The key fix was changing imports in `render_v2.rs` to use the public re-exports:

**Before (ERROR)**:
```rust
use crate::renderer::backend::{BackendChoice, OutputFormat};
use crate::renderer::webgpu::WebGPUBackend;
```

**After (WORKS)**:
```rust
use crate::renderer::{BackendChoice, OutputFormat};
use crate::renderer::WebGPUBackend;
```

This works because `renderer/mod.rs` declares modules as private (`mod backend;`) but re-exports types publicly (`pub use backend::BackendChoice;`).

### WebGPU Initialization
The builder's `.build()` method initializes WebGPU backend if requested:
```rust
#[cfg(feature = "webgpu-backend")]
let webgpu_backend = if self.backend_config.points == BackendChoice::WebGPU {
    Some(pollster::block_on(crate::renderer::WebGPUBackend::new(width, height))?)
} else {
    None
};
```

Uses `pollster::block_on()` to handle WebGPU's async initialization in a sync context.

## Approval Checkpoint

**Phase 1 is ready for user approval before proceeding to Phase 2.**

User should verify:
- `render.rs` is UNTOUCHED (original working code preserved)
- `render_v2.rs` compiles successfully
- Builder pattern API looks correct
- Ready to proceed with GPU rendering implementation

---

**Created**: 2026-01-07
**Status**: ✅ COMPLETE
**Next**: Awaiting user approval for Phase 2
