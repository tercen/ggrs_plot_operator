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
pub mod data;
pub mod table;

// Re-exports for convenience
pub use client::TercenClient;
#[allow(unused_imports)]
pub use data::{DataRow, DataSummary, ParsedData};
#[allow(unused_imports)]
pub use error::{Result, TercenError};
pub use logger::TercenLogger;
#[allow(unused_imports)]
pub use table::TableStreamer;
