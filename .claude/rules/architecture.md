# Architecture Rules

Core design principles for the ggrs_plot_operator.

## Separation of Concerns (CRITICAL)

**No struct/module should handle multiple disparate functionalities.** Each component should have a single, well-defined responsibility.

```rust
// ❌ BAD: Mixed responsibilities
struct DataHandler {
    fn fetch_data() { /* gRPC calls */ }
    fn transform_data() { /* Polars operations */ }
    fn render_plot() { /* GGRS rendering */ }
    fn upload_result() { /* gRPC calls */ }
}

// ✅ GOOD: Separated concerns
struct TercenClient { /* gRPC communication only */ }
struct DataTransformer { /* Polars operations only */ }
struct PlotRenderer { /* GGRS rendering only */ }
struct ResultUploader { /* Result handling only */ }
```

When adding functionality, ask: "Does this belong here, or should it be a separate component?"

## Abstraction Hierarchies

Design with traits as interfaces. Centralize common behavior, specialize only what differs.

```rust
// ✅ GOOD: Trait as interface
pub trait TercenContext {
    fn client(&self) -> &Arc<TercenClient>;
    fn cube_query(&self) -> &CubeQuery;
    fn color_infos(&self) -> &[ColorInfo];
    // ... common interface
}

// ✅ GOOD: Shared implementation via default methods or helper functions
impl TercenContext for ProductionContext { /* production-specific */ }
impl TercenContext for DevContext { /* dev-specific */ }

// ✅ GOOD: Generic code works with any implementation
pub async fn generate_plots<C: TercenContext>(ctx: &C) -> Result<Vec<PlotResult>> {
    // Works with both ProductionContext and DevContext
}
```

### Hierarchy Guidelines

1. **Traits define contracts** - What operations are available
2. **Common code in shared functions/modules** - Avoid duplication
3. **Specific implementations only override what differs**
4. **Prefer composition over deep inheritance**

## Builders and Factories

Use builders for complex object construction. Use factories when object creation involves logic.

```rust
// ✅ GOOD: Builder pattern for complex configuration
let stream_config = TercenStreamConfig::new(qt_hash, col_hash, row_hash, chunk_size)
    .y_axis_table(y_table_id)
    .x_axis_table(x_table_id)
    .colors(color_infos)
    .page_factors(page_factors)
    .schema_cache(cache);

// ✅ GOOD: Factory method when creation involves logic
impl TercenStreamGenerator {
    pub async fn new(client: Arc<TercenClient>, config: TercenStreamConfig) -> Result<Self> {
        // Complex initialization: load facets, compute ranges, etc.
    }
}
```

## Coupling Management

Minimize coupling between components. Depend on abstractions, not concretions.

```rust
// ❌ BAD: Direct dependency on concrete type
fn process(client: &TercenClient, config: &ProductionConfig) { }

// ✅ GOOD: Depend on trait/abstraction
fn process<C: TercenContext>(ctx: &C) { }

// ✅ GOOD: Use dependency injection
struct Pipeline<'a, C: TercenContext> {
    ctx: &'a C,
    config: &'a OperatorConfig,
}
```

## Three-Layer Design

```
Tercen gRPC API
      ↓
[1] src/tercen/         → gRPC client, auth, streaming (tonic/prost)
      ↓
[2] TSON → Polars       → Columnar data transformation
      ↓
[3] src/ggrs_integration/ → Implements GGRS StreamGenerator trait
      ↓
ggrs-core library       → Plot rendering (../ggrs/crates/ggrs-core)
      ↓
PNG Output
```

## Columnar Architecture (CRITICAL)

Never build row-by-row structures. Always stay columnar with Polars.

```rust
// ✅ GOOD: Columnar operations
let filtered = df.lazy().filter(col(".ci").eq(lit(0))).collect()?;

// ❌ BAD: Row-by-row iteration
for row in 0..df.height() { build_record(df, row); }
```

## No Fallback Strategies (CRITICAL)

**By default, NO implementation should include ANY fallback procedures.** Implement fallbacks ONLY when explicitly instructed to do so.

Fallbacks mask bugs and make debugging harder. If something fails, it should fail loudly.

```rust
// ❌ BAD: Fallback pattern
if data.has_column(".ys") { use_ys() } else { use_y() }

// ❌ BAD: Silent default
let value = config.get("key").unwrap_or_default();

// ✅ GOOD: Trust the specification
data.column(".ys")

// ✅ GOOD: Explicit requirement
let value = config.get("key").ok_or("Missing required config 'key'")?;
```

## Error Handling: Fail Fast, Fail Loud

**Do NOT attempt to recover from errors.** When an error occurs:
1. Provide an informative error message describing what went wrong
2. Propagate the error (throw/return Err)

```rust
// ❌ BAD: Silent recovery
let schema = client.get_schema(&id).await.unwrap_or_default();

// ❌ BAD: Logging and continuing
if let Err(e) = client.get_schema(&id).await {
    eprintln!("Warning: {}", e);
    return default_schema();
}

// ✅ GOOD: Informative error and propagation
let schema = client.get_schema(&id).await.map_err(|e| {
    format!("Failed to fetch schema for table '{}': {}. \
             Verify the table ID exists and is accessible.", id, e)
})?;

// ✅ GOOD: Panic with context for invariant violations
let range = axis_ranges.get(&(col_idx, row_idx)).unwrap_or_else(|| {
    panic!(
        "No axis range for cell ({}, {}). \
         This indicates missing axis range data or a bug in facet indexing.",
        col_idx, row_idx
    )
});
```

### Error Message Guidelines

- State WHAT failed (operation, resource)
- Include relevant IDs/values for debugging
- Suggest possible causes when known
- Never swallow errors silently

## Context Trait Pattern

The `TercenContext` trait abstracts production vs development environments:

```rust
// Both implement TercenContext
ProductionContext::from_task_id(client, task_id)  // Production
DevContext::new(client, workflow_id, step_id)      // Local development
```

Key methods: `cube_query()`, `color_infos()`, `page_factors()`, `chart_kind()`, `point_size()`

## Direct Rendering Path

The renderer uses `stream_and_render_direct()` for all rendering. This is the only rendering path - the standard path with ChartContext overhead has been removed.

**IMPORTANT**: Any rendering modifications must update `stream_and_render_direct()` in `ggrs-core/src/render.rs`.

## Component Responsibilities

| Component | Responsibility |
|-----------|---------------|
| `TercenStreamGenerator` | Streams raw data, provides facet metadata. Does NOT know about chart types. |
| `Pipeline` | Orchestrates plots, selects geom (tile vs point), configures theme/scales. |
| `TercenContext` | Abstracts production/dev environments, extracts workflow metadata. |
