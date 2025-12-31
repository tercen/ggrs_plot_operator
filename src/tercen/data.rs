#![allow(dead_code)]
use super::error::{Result, TercenError};
use csv::ReaderBuilder;
use serde::Deserialize;
use std::collections::HashMap;

/// Represents a row of data from Tercen table
#[derive(Debug, Clone, Deserialize)]
pub struct DataRow {
    /// Column facet index
    #[serde(rename = ".ci")]
    pub ci: Option<i32>,

    /// Row facet index
    #[serde(rename = ".ri")]
    pub ri: Option<i32>,

    /// X-axis value
    #[serde(rename = ".x")]
    pub x: Option<f64>,

    /// Y-axis value
    #[serde(rename = ".y")]
    pub y: Option<f64>,

    /// Additional columns (colors, labels, etc.)
    #[serde(flatten)]
    pub extra: HashMap<String, String>,
}

/// Parsed data from a Tercen table
#[derive(Debug, Clone)]
pub struct ParsedData {
    pub rows: Vec<DataRow>,
    pub columns: Vec<String>,
}

impl ParsedData {
    /// Parse CSV data from Tercen into structured rows
    pub fn from_csv(csv_data: &[u8]) -> Result<Self> {
        let mut reader = ReaderBuilder::new().has_headers(true).from_reader(csv_data);

        // Get column names
        let headers = reader
            .headers()
            .map_err(|e| TercenError::Other(format!("Failed to read CSV headers: {}", e)))?;
        let columns: Vec<String> = headers.iter().map(|s| s.to_string()).collect();

        // Parse rows
        let mut rows = Vec::new();
        for result in reader.deserialize() {
            let row: DataRow = result
                .map_err(|e| TercenError::Other(format!("Failed to parse CSV row: {}", e)))?;
            rows.push(row);
        }

        Ok(ParsedData { rows, columns })
    }

    /// Filter data by facet indices
    pub fn filter_by_facet(&self, col_idx: Option<i32>, row_idx: Option<i32>) -> Vec<DataRow> {
        self.rows
            .iter()
            .filter(|row| {
                let col_match = col_idx.map(|ci| row.ci == Some(ci)).unwrap_or(true);
                let row_match = row_idx.map(|ri| row.ri == Some(ri)).unwrap_or(true);
                col_match && row_match
            })
            .cloned()
            .collect()
    }

    /// Get summary statistics
    pub fn summary(&self) -> DataSummary {
        let x_values: Vec<f64> = self.rows.iter().filter_map(|r| r.x).collect();
        let y_values: Vec<f64> = self.rows.iter().filter_map(|r| r.y).collect();

        DataSummary {
            total_rows: self.rows.len(),
            x_min: x_values.iter().copied().fold(f64::INFINITY, f64::min),
            x_max: x_values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            y_min: y_values.iter().copied().fold(f64::INFINITY, f64::min),
            y_max: y_values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        }
    }
}

/// Summary statistics for data
#[derive(Debug, Clone)]
pub struct DataSummary {
    pub total_rows: usize,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

impl std::fmt::Display for DataSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DataSummary {{ rows: {}, x: [{:.2}, {:.2}], y: [{:.2}, {:.2}] }}",
            self.total_rows, self.x_min, self.x_max, self.y_min, self.y_max
        )
    }
}
