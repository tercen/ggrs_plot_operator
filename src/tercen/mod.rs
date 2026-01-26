//! Tercen gRPC client module
//!
//! This module contains all Tercen-specific code and will be extracted
//! into a separate `tercen-rust` crate in the future.
//!
//! Structure:
//! - `client.rs`: Core gRPC client and authentication
//! - `services/`: Service wrappers (task, table, file)
//! - `types.rs`: Common types and conversions
//! - `error.rs`: Error types

// Module declarations
pub mod error;

// Client module (Phase 2)
pub mod client;
pub mod logger;

// Data modules (Phase 4)
pub mod table;
pub mod tson_convert;

// Facet modules (Phase 5)
pub mod facets;

// Result upload modules (Phase 8)
pub mod result;
pub mod table_convert;

// Operator properties (Phase 9 - Version 0.0.2)
pub mod properties;

// Color handling (Version 0.0.3)
pub mod colors;

// Page handling (Version 0.0.4)
pub mod pages;

// Context abstraction (Version 0.0.4)
pub mod context;

// Re-exports for convenience
pub use client::TercenClient;
pub use colors::{
    categorical_color_from_level, extract_color_info_from_step, interpolate_color, parse_palette,
    CategoryColorMap, ColorInfo, ColorMapping, ColorPalette, ColorStop,
};
pub use context::{DevContext, ProductionContext, TercenContext};
#[allow(unused_imports)]
pub use error::{Result, TercenError};
#[allow(unused_imports)]
pub use facets::{FacetGroup, FacetInfo, FacetMetadata};
pub use logger::TercenLogger;
pub use pages::{extract_page_factors, extract_page_values, PageValue};
pub use properties::{PlotDimension, PropertyReader};
pub use result::PlotResult;
#[allow(unused_imports)]
pub use table::TableStreamer;
pub use tson_convert::tson_to_dataframe;
