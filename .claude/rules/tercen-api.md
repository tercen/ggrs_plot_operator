# Tercen API Rules

gRPC integration with Tercen platform.

## Table IDs and Hashes

The hashes in `CubeQuery` ARE the table IDs - pass directly to `get_schema()`:

| Source | Table | Purpose |
|--------|-------|---------|
| `cube_query.qt_hash` | Main data table | `.xs`, `.ys`, `.ci`, `.ri`, color factors |
| `cube_query.column_hash` | Column facet table | Facet labels for columns |
| `cube_query.row_hash` | Row facet table | Facet labels for rows, page factors |
| `schema_ids` (Y-axis) | Y-axis range table | `.minY`, `.maxY` per facet - **REQUIRED** |
| `schema_ids` (X-axis) | X-axis range table | `.minX`, `.maxX` - optional |
| `schema_ids` (color_N) | Color tables | Color factor data |

**Important**: Y-axis table is required for dequantization. If not found, throw error.

## gRPC Services

| Service | Key Methods | Notes |
|---------|-------------|-------|
| `TableSchemaService` | `get`, `streamTable` | Fetch schema by ID, stream table data |
| `TaskService` | `get` | Fetch task by ID |
| `WorkflowService` | `get`, `getCubeQuery` | Fetch workflow, get CubeQuery |

No batch/list methods available - only single `get` by ID.

## TSON Format

Tercen uses TSON binary format for streaming. Conversion:

```rust
use crate::tercen::{tson_to_dataframe, TableStreamer};

let streamer = TableStreamer::new(&client);
let tson_data = streamer.stream_tson(&table_id, Some(columns), offset, limit).await?;
let df = tson_to_dataframe(&tson_data)?;
```

## Key Tercen Modules

| Module | Purpose |
|--------|---------|
| `client.rs` | TercenClient with gRPC auth |
| `context/` | TercenContext trait + implementations |
| `table.rs` | TableStreamer for chunked data streaming |
| `tson_convert.rs` | TSON → Polars DataFrame conversion |
| `colors.rs` | Color palette extraction, ChartKind enum |
| `pages.rs` | Multi-page plot support |
| `facets.rs` | Facet metadata handling |

## Color Mapping

```rust
pub enum ColorMapping {
    Continuous(ColorPalette),  // f64 factor → palette interpolation
    Categorical(CategoryColorMap),  // .colorLevels (int32) → default palette
}
```

Continuous colors use factor column values. Categorical colors use `.colorLevels` column.

## Pagination

Page factors filter facets (not data). GGRS matches data to facets via `original_index` mapping.

```rust
// Page filter passed to FacetInfo::load_with_filter()
let page_filter = Some(&page_value.values);  // e.g., {"sex": "female"}
```

## Schema Caching

For multi-page plots, schemas are cached and reused:

```rust
let schema_cache = if page_values.len() > 1 {
    Some(new_schema_cache())
} else {
    None
};
```