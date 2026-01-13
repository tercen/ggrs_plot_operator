//! Operator configuration
//!
//! Loads configuration from operator_config.json

use serde::Deserialize;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct OperatorConfig {
    /// Number of rows to fetch per chunk when streaming data
    pub chunk_size: usize,

    /// Maximum number of chunks to process (safety limit)
    pub max_chunks: usize,

    /// Whether to cache computed axis ranges
    pub cache_axis_ranges: bool,

    /// Default plot width in pixels
    pub default_plot_width: u32,

    /// Default plot height in pixels
    pub default_plot_height: u32,

    /// Render backend: "cpu" or "gpu"
    #[serde(default = "default_backend")]
    pub render_backend: String,

    /// Point size (default: 4, range: 1-10)
    #[serde(default = "default_point_size")]
    pub point_size: u32,
}

fn default_backend() -> String {
    "cpu".to_string()
}

fn default_point_size() -> u32 {
    4
}

impl Default for OperatorConfig {
    fn default() -> Self {
        Self {
            chunk_size: 10_000,
            max_chunks: 1000,
            cache_axis_ranges: true,
            default_plot_width: 2000,
            default_plot_height: 2000,
            render_backend: "cpu".to_string(),
            point_size: 4,
        }
    }
}

impl OperatorConfig {
    /// Load configuration from operator_config.json
    ///
    /// Falls back to default values if file doesn't exist or can't be parsed
    pub fn load() -> Self {
        match fs::read_to_string("operator_config.json") {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(config) => {
                    println!("✓ Loaded configuration from operator_config.json");
                    config
                }
                Err(e) => {
                    eprintln!("⚠ Failed to parse operator_config.json: {}", e);
                    eprintln!("  Using default configuration");
                    Self::default()
                }
            },
            Err(_) => {
                println!("ℹ operator_config.json not found, using default configuration");
                Self::default()
            }
        }
    }
}
