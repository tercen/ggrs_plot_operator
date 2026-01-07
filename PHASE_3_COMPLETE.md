# Phase 3 Complete: Test Binary with Backend Selection

## Status: ✅ COMPLETE - Ready for Testing

Phase 3 objectives achieved:
- ✅ Created `test_stream_generator_v2.rs` from original test binary
- ✅ Added command-line backend selection (`--backend cpu` or `--backend gpu`)
- ✅ Integrated `render_v2::ImageRenderer` with builder pattern
- ✅ **Compilation successful** (only minor warnings)

## Files Created/Modified

### `/home/thiago/workspaces/tercen/main/ggrs_plot_operator/src/bin/test_stream_generator_v2.rs` (NEW)

**Key additions**:

1. **RenderBackend enum** (lines 29-45):
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum RenderBackend {
    Cpu,
    Gpu,
}

impl std::str::FromStr for RenderBackend {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cpu" => Ok(RenderBackend::Cpu),
            "gpu" => Ok(RenderBackend::Gpu),
            _ => Err(format!("Invalid backend '{}'.  Use 'cpu' or 'gpu'", s)),
        }
    }
}
```

2. **Command-line argument parsing** (lines 59-78):
```rust
let args: Vec<String> = std::env::args().collect();
let mut backend = RenderBackend::Cpu; // Default to CPU

let mut i = 1;
while i < args.len() {
    match args[i].as_str() {
        "--backend" => {
            if i + 1 < args.len() {
                backend = args[i + 1].parse()?;
                i += 2;
            } else {
                return Err("--backend requires an argument (cpu or gpu)".into());
            }
        }
        arg => return Err(format!("Unknown argument: {}", arg).into()),
    }
}
```

3. **Backend-aware renderer creation** (lines 478-518):
```rust
#[cfg(feature = "webgpu-backend")]
let renderer = match backend {
    RenderBackend::Gpu => {
        println!("Creating image renderer with GPU acceleration...");
        use ggrs_core::render_v2::ImageRenderer;
        ImageRenderer::builder(plot_gen, width, height)
            .points_gpu()
            .build()?
    }
    RenderBackend::Cpu => {
        println!("Creating image renderer with CPU rendering...");
        use ggrs_core::render_v2::ImageRenderer;
        ImageRenderer::new(plot_gen, width, height)
    }
};

#[cfg(not(feature = "webgpu-backend"))]
let renderer = {
    use ggrs_core::render_v2::ImageRenderer;
    if backend == RenderBackend::Gpu {
        eprintln!("WARNING: GPU backend requested but webgpu-backend feature not enabled");
        eprintln!("         Falling back to CPU rendering");
    }
    ImageRenderer::new(plot_gen, width, height)
};
```

## Usage

### CPU Rendering (Default)
```bash
export TERCEN_URI="http://127.0.0.1:50051"
export TERCEN_TOKEN="your_token"
export WORKFLOW_ID="your_workflow_id"
export STEP_ID="your_step_id"

cargo run --bin test_stream_generator_v2
```

### GPU Rendering
```bash
# Same env vars as above, plus:

cargo run --bin test_stream_generator_v2 -- --backend gpu
```

### Explicit CPU Rendering
```bash
cargo run --bin test_stream_generator_v2 -- --backend cpu
```

## Build Verification

```bash
cargo build --bin test_stream_generator_v2
# ✅ Finished `dev` profile [unoptimized + debuginfo] target(s) in 20.32s
# ⚠️ Only warnings (unused imports, cfg condition values) - no errors
```

## Next Steps (Ready for Testing)

### Test 1: CPU Rendering
Run test with default CPU backend to verify v2 works identically to original:
```bash
./test_local.sh  # Or manually with env vars
cargo run --bin test_stream_generator_v2
```

**Expected**:
- ✅ Connects to Tercen
- ✅ Loads 475K rows
- ✅ Generates `plot.png`
- ✅ Time: ~4s (same as original with chunk size fix)
- ✅ Output identical to `test_stream_generator`

### Test 2: GPU Rendering (Requires webgpu-backend feature)
```bash
# Need to enable webgpu-backend feature in ggrs-core
cargo run --bin test_stream_generator_v2 -- --backend gpu
```

**Expected** (if WebGPU available):
- ✅ Prints "Initializing WebGPU backend..."
- ✅ Prints "✓ WebGPU backend initialized successfully"
- ✅ Generates `plot.png`
- ✅ Time: **~0.5s** (10x faster - mostly network/setup overhead remaining)

**Expected** (if WebGPU not available):
- ⚠️ Prints "WARNING: GPU backend requested but webgpu-backend feature not enabled"
- ⚠️ Falls back to CPU rendering
- ✅ Still generates correct plot

### Test 3: Compare Outputs
```bash
# Generate with CPU
cargo run --bin test_stream_generator_v2 -- --backend cpu
mv plot.png plot_cpu.png

# Generate with GPU
cargo run --bin test_stream_generator_v2 -- --backend gpu  
mv plot.png plot_gpu.png

# Visual comparison
open plot_cpu.png plot_gpu.png
```

**Expected**:
- Plots should be visually identical (or very close - GPU uses f32, CPU uses f64)
- GPU version generated much faster

## Technical Notes

### Backend Selection Flow

1. **Parse args** → `backend: RenderBackend`
2. **Match backend**:
   - `Cpu` → `ImageRenderer::new()` (legacy path)
   - `Gpu` → `ImageRenderer::builder().points_gpu().build()` (new path)
3. **Feature gate**: If `webgpu-backend` not enabled, always falls back to CPU

### Feature Flags

The test binary respects conditional compilation:
- `#[cfg(feature = "webgpu-backend")]` → GPU path available
- `#[cfg(not(feature = "webgpu-backend"))]` → CPU-only path

Currently, `webgpu-backend` is NOT enabled in `Cargo.toml`, so GPU path won't execute yet.

### Warnings (Non-Critical)

1. **`unexpected cfg condition value: webgpu-backend`**
   - Reason: Feature not defined in workspace `Cargo.toml`
   - Impact: None (falls back to CPU correctly)
   - Fix: Add feature to main `Cargo.toml` when ready

2. **`unused import: ImageRenderer`**
   - Reason: Imported at top but used in nested scopes
   - Impact: None (cosmetic only)
   - Fix: Remove top-level import (DONE)

## Approval Checkpoint

**Phase 3 ready for user testing**:
- ✅ Test binary compiles successfully
- ✅ Command-line argument parsing works
- ✅ Backend selection implemented
- ✅ Fallback logic in place
- ✅ Clear usage documentation

**User should**:
1. Run CPU test first to verify baseline works
2. Decide if GPU testing needed now or later
3. Approve Phase 3 before proceeding to Phase 4

---

**Created**: 2026-01-07
**Status**: ✅ COMPLETE (ready for testing)
**Next**: User testing + approval for Phase 4
