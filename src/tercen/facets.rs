//! Facet metadata loading and management
//!
//! This module handles loading and parsing facet tables (column.csv and row.csv)
//! which define the structure of faceted plots.

use super::error::Result;
use super::table::TableStreamer;
use super::tson_convert::tson_to_dataframe;
use super::TercenClient;
use crate::ggrs_integration::stream_generator::extract_column_names_from_schema;
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

        // Get row count from schema
        let schema = streamer.get_schema(table_id).await?;

        use crate::tercen::client::proto::e_schema;
        let n_rows = match &schema.object {
            Some(e_schema::Object::Cubequerytableschema(cqts)) => {
                eprintln!("DEBUG: CubeQueryTableSchema nRows={}", cqts.n_rows);
                cqts.n_rows as usize
            }
            Some(e_schema::Object::Tableschema(ts)) => {
                eprintln!("DEBUG: TableSchema nRows={}", ts.n_rows);
                ts.n_rows as usize
            }
            Some(e_schema::Object::Computedtableschema(cts)) => {
                eprintln!("DEBUG: ComputedTableSchema nRows={}", cts.n_rows);
                cts.n_rows as usize
            }
            other => {
                eprintln!("DEBUG: Unknown schema type: {:?}", other);
                0
            }
        };

        if n_rows == 0 {
            return Ok(FacetMetadata {
                groups: vec![],
                column_names: vec![],
            });
        }

        // Get column names from schema first
        let column_names = match extract_column_names_from_schema(&schema) {
            Ok(cols) => cols,
            Err(e) => {
                eprintln!("DEBUG: Failed to extract column names: {}", e);
                vec![]
            }
        };
        eprintln!("DEBUG: Facet table has columns: {:?}", column_names);

        // Stream TSON data to get actual facet values
        // Request specific columns (not None) to ensure data is materialized
        let columns_to_fetch = if column_names.is_empty() {
            None
        } else {
            Some(column_names.clone())
        };

        let tson_data = streamer
            .stream_tson(table_id, columns_to_fetch, 0, n_rows as i64)
            .await?;

        // If no data, return placeholder labels
        if tson_data.is_empty() || tson_data.len() < 30 {
            eprintln!(
                "DEBUG: Facet table has no data ({} bytes), using index labels",
                tson_data.len()
            );
            let groups: Vec<FacetGroup> = (0..n_rows)
                .map(|index| FacetGroup {
                    index,
                    label: format!("{}", index),
                    values: Default::default(),
                })
                .collect();

            return Ok(FacetMetadata {
                groups,
                column_names: vec![],
            });
        }

        // Parse TSON to DataFrame
        let df = tson_to_dataframe(&tson_data)?;
        eprintln!(
            "DEBUG: Parsed facet table: {} rows × {} columns",
            df.nrow(),
            df.ncol()
        );

        let column_names: Vec<String> = df.columns().iter().map(|s| s.to_string()).collect();
        eprintln!("DEBUG: Facet columns: {:?}", column_names);

        // Create groups from parsed data
        let mut groups = Vec::new();
        for index in 0..df.nrow() {
            let mut values = HashMap::new();
            let mut label_parts = Vec::new();

            // Collect all column values for this row
            for col_name in &column_names {
                if let Ok(value) = df.get_value(index, col_name) {
                    let value_str = value.as_string();
                    values.insert(col_name.clone(), value_str.clone());
                    label_parts.push(value_str);
                }
            }

            // Join all values with ", " to create label
            let label = if label_parts.is_empty() {
                format!("{}", index)
            } else {
                label_parts.join(", ")
            };

            groups.push(FacetGroup {
                index,
                label,
                values,
            });
        }

        eprintln!("DEBUG: Created {} facet groups", groups.len());
        if !groups.is_empty() {
            eprintln!("DEBUG: First facet label: '{}'", groups[0].label);
        }

        Ok(FacetMetadata {
            groups,
            column_names,
        })
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
