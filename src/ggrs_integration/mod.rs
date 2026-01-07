//! GGRS integration module
//!
//! This module bridges Tercen data with the GGRS plotting library.
//!
//! Structure:
//! - `stream_generator.rs`: TercenStreamGenerator implementing GGRS StreamGenerator trait
//! - `plot_builder.rs`: Helper to build GGRS plot specs from operator properties
//! - `renderer.rs`: Wrapper around GGRS ImageRenderer

// Module declarations
pub mod stream_generator;

// Re-exports
pub use stream_generator::TercenStreamGenerator;
