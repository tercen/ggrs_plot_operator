# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**ggrs_plot_operator** is a Rust-based Tercen operator that integrates the GGRS plotting library with Tercen's gRPC API. It receives tabular data from Tercen, generates high-performance plots with faceting and colors, and returns PNG images.

## Essential Commands

```bash
# Build (use dev-release for faster iteration)
cargo build --profile dev-release     # ~5 min
cargo build --release                 # ~12 min (production only)

# Quality Checks (MANDATORY before code is complete)
cargo fmt && cargo clippy -- -D warnings && cargo test

# Local Testing
./test_local.sh

# Proto submodule setup
git submodule update --init --recursive
```

## Architecture

### Three-Layer Design

1. **Tercen gRPC Client** (`src/tercen/`) - Connection, auth, streaming via tonic/prost
2. **Data Transform** - TSON → Polars DataFrame (columnar)
3. **GGRS Integration** (`src/ggrs_integration/`) - Implements GGRS StreamGenerator trait

### Key Files

- `src/main.rs` - Entry point
- `src/config.rs` - Property-based configuration from `operator.json`
- `src/tercen/client.rs` - TercenClient with auth
- `src/tercen/table.rs` - TableStreamer (chunked streaming)
- `src/tercen/tson_convert.rs` - TSON → Polars DataFrame
- `src/ggrs_integration/stream_generator.rs` - TercenStreamGenerator + global cache
- `src/bin/test_stream_generator.rs` - Test binary

## Core Technical Decisions

### 1. Columnar Architecture (CRITICAL)

**Never build row-by-row structures. Always stay columnar.**

```rust
// ✅ GOOD: Columnar operations
let filtered = df.lazy().filter(col(".ci").eq(lit(0))).collect()?;

// ❌ BAD: Row-by-row iteration
for row in 0..df.height() { build_record(df, row); }
```

### 2. No Fallback Strategies

**Never implement fallback logic unless explicitly requested.** Fallbacks mask bugs.

```rust
// ❌ BAD: Fallback pattern
if data.has_column(".ys") { use_ys() } else { use_y() }

// ✅ GOOD: Trust the specification
data.column(".ys")
```

## Key Dependencies

- `ggrs-core` - Local path `../ggrs/crates/ggrs-core` (switch to git for CI)
- `polars` - Columnar DataFrame (critical for performance)
- `tonic`/`prost` - gRPC client
- `tokio` - Async runtime

## Notes for Claude Code

### Git Policy
- Never commit/push unless explicitly requested
- Run quality checks before reporting task complete

### Code Completion Checklist
1. `cargo fmt`
2. `cargo clippy -- -D warnings`
3. `cargo build --profile dev-release`
4. `cargo test`

### Session Context
Check `CONTINUE.md` for ongoing work. Check `SESSION_*.md` files for recent development context.
