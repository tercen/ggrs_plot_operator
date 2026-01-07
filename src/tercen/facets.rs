//! Facet metadata loading and management
//!
//! This module handles loading and parsing facet tables (column.csv and row.csv)
//! which define the structure of faceted plots.

use super::error::Result;
use super::table::TableStreamer;
use super::tson_convert::tson_to_dataframe;
use super::TercenClient;
use std::collections::HashMap;

/// Represents a single facet group
#[derive(Debug, Clone)]
pub struct FacetGroup {
    /// Index of this facet group (0-based)
    pub index: usize,
    /// Label for display (combination of all column values)
    pub label: String,
    /// Raw column values for this facet
    pub values: HashMap<String, String>,
}

/// Collection of facet groups for one dimension (column or row)
#[derive(Debug, Clone)]
pub struct FacetMetadata {
    /// All facet groups in order
    pub groups: Vec<FacetGroup>,
    /// Column names in the facet table
    pub column_names: Vec<String>,
}

impl FacetMetadata {
    /// Load facet metadata from a Tercen table
    pub async fn load(client: &TercenClient, table_id: &str) -> Result<Self> {
        let streamer = TableStreamer::new(client);

        // Load entire facet table (it's small)
        let arrow_data = streamer.stream_tson(table_id, None, 0, 10000).await?;

        if arrow_data.is_empty() {
            return Ok(FacetMetadata {
                groups: vec![],
                column_names: vec![],
            });
        }

        Self::parse_facet_arrow(&arrow_data)
    }

    /// Parse facet TSON data into structured metadata
    fn parse_facet_arrow(arrow_data: &[u8]) -> Result<Self> {
        let df = tson_to_dataframe(arrow_data)?;

        let column_names: Vec<String> = df.columns().iter().map(|s| s.to_string()).collect();

        // Parse each row as a facet group
        let mut groups = Vec::new();
        for index in 0..df.nrow() {
            let mut values = HashMap::new();
            let mut label_parts = Vec::new();

            for col_name in &column_names {
                if let Ok(value) = df.get_value(index, col_name) {
                    let value_str = value.as_string();
                    values.insert(col_name.clone(), value_str.clone());
                    label_parts.push(value_str);
                }
            }

            let label = label_parts.join(", ");

            groups.push(FacetGroup {
                index,
                label,
                values,
            });
        }

        Ok(FacetMetadata {
            groups,
            column_names,
        })
    }

    /// Get number of facet groups
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Get a specific facet group by index
    pub fn get(&self, index: usize) -> Option<&FacetGroup> {
        self.groups.get(index)
    }
}

/// Complete faceting information for a plot
#[derive(Debug, Clone)]
pub struct FacetInfo {
    /// Column facet metadata
    pub col_facets: FacetMetadata,
    /// Row facet metadata
    pub row_facets: FacetMetadata,
}

impl FacetInfo {
    /// Load both column and row facet metadata
    pub async fn load(
        client: &TercenClient,
        col_table_id: &str,
        row_table_id: &str,
    ) -> Result<Self> {
        // Load both facet tables in parallel
        let (col_result, row_result) = tokio::join!(
            FacetMetadata::load(client, col_table_id),
            FacetMetadata::load(client, row_table_id)
        );

        Ok(FacetInfo {
            col_facets: col_result?,
            row_facets: row_result?,
        })
    }

    /// Get total number of column facets
    pub fn n_col_facets(&self) -> usize {
        if self.col_facets.is_empty() {
            1 // No faceting = 1 facet
        } else {
            self.col_facets.len()
        }
    }

    /// Get total number of row facets
    pub fn n_row_facets(&self) -> usize {
        if self.row_facets.is_empty() {
            1 // No faceting = 1 facet
        } else {
            self.row_facets.len()
        }
    }

    /// Get total number of facet cells (col × row)
    pub fn total_facets(&self) -> usize {
        self.n_col_facets() * self.n_row_facets()
    }

    /// Check if plot has any faceting
    pub fn has_faceting(&self) -> bool {
        !self.col_facets.is_empty() || !self.row_facets.is_empty()
    }
}

// Tests removed - CSV parsing replaced with TSON format
