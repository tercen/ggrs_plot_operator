//! Operator property definitions with defaults from operator.json
//!
//! This module parses operator.json at compile time to extract property definitions
//! and their default values. This ensures defaults are defined in ONE place (operator.json)
//! and eliminates hardcoded fallback values scattered throughout the codebase.

use crate::tercen::client::proto::OperatorSettings;
use std::collections::HashMap;

/// Operator.json embedded at compile time
const OPERATOR_JSON: &str = include_str!("../../operator.json");

/// Property definition from operator.json
#[derive(Debug, Clone)]
pub struct PropertyDef {
    pub name: String,
    pub kind: PropertyKind,
    pub default_value: String,
    pub description: String,
    /// For EnumeratedProperty, the valid values
    pub valid_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PropertyKind {
    String,
    Enumerated,
    Boolean,
}

/// Registry of all operator properties with their defaults from operator.json
pub struct PropertyRegistry {
    properties: HashMap<String, PropertyDef>,
}

impl PropertyRegistry {
    /// Parse operator.json and build the registry
    ///
    /// This is called once at startup. Panics if operator.json is malformed
    /// (which should never happen since it's compile-time embedded).
    pub fn from_operator_json() -> Self {
        let json: serde_json::Value =
            serde_json::from_str(OPERATOR_JSON).expect("operator.json is invalid JSON");

        let properties_array = json["properties"]
            .as_array()
            .expect("operator.json missing 'properties' array");

        let mut properties = HashMap::new();

        for prop in properties_array {
            let name = prop["name"]
                .as_str()
                .expect("property missing 'name'")
                .to_string();

            let kind_str = prop["kind"].as_str().expect("property missing 'kind'");
            let kind = match kind_str {
                "StringProperty" => PropertyKind::String,
                "EnumeratedProperty" => PropertyKind::Enumerated,
                "BooleanProperty" => PropertyKind::Boolean,
                other => panic!("Unknown property kind: {}", other),
            };

            // BooleanProperty uses a JSON bool for defaultValue, others use string
            let default_value = if kind == PropertyKind::Boolean {
                match &prop["defaultValue"] {
                    v if v.is_boolean() => v.as_bool().unwrap().to_string(),
                    v if v.is_string() => v.as_str().unwrap().to_string(),
                    _ => "false".to_string(),
                }
            } else {
                prop["defaultValue"].as_str().unwrap_or("").to_string()
            };

            let description = prop["description"].as_str().unwrap_or("").to_string();

            let valid_values = if kind == PropertyKind::Enumerated {
                prop["values"].as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
            } else {
                None
            };

            properties.insert(
                name.clone(),
                PropertyDef {
                    name,
                    kind,
                    default_value,
                    description,
                    valid_values,
                },
            );
        }

        Self { properties }
    }

    /// Get the default value for a property
    pub fn get_default(&self, name: &str) -> Option<&str> {
        self.properties.get(name).map(|p| p.default_value.as_str())
    }

    /// Get the property definition
    pub fn get_property(&self, name: &str) -> Option<&PropertyDef> {
        self.properties.get(name)
    }

    /// Check if a value is valid for an enumerated property
    pub fn is_valid_enum_value(&self, name: &str, value: &str) -> bool {
        self.properties
            .get(name)
            .and_then(|p| p.valid_values.as_ref())
            .map(|values| values.iter().any(|v| v.eq_ignore_ascii_case(value)))
            .unwrap_or(true) // Non-enumerated properties accept any value
    }
}

/// Global registry instance (initialized lazily)
static REGISTRY: std::sync::OnceLock<PropertyRegistry> = std::sync::OnceLock::new();

/// Get the global property registry
pub fn registry() -> &'static PropertyRegistry {
    REGISTRY.get_or_init(PropertyRegistry::from_operator_json)
}

/// Typed operator property reader
///
/// Reads operator properties from Tercen settings, using defaults from operator.json.
/// All defaults come from operator.json - no hardcoded values in this code.
pub struct OperatorPropertyReader {
    /// Properties from Tercen (user-set values)
    user_values: HashMap<String, String>,
}

impl OperatorPropertyReader {
    /// Create from OperatorSettings
    pub fn new(settings: Option<&OperatorSettings>) -> Self {
        let user_values = settings
            .and_then(|s| s.operator_ref.as_ref())
            .map(|op_ref| {
                op_ref
                    .property_values
                    .iter()
                    .filter(|p| !p.value.is_empty()) // Empty = not set
                    .map(|p| (p.name.clone(), p.value.clone()))
                    .collect()
            })
            .unwrap_or_default();

        Self { user_values }
    }

    /// Get string property (user value or default from operator.json)
    ///
    /// Returns the user-set value if present and non-empty,
    /// otherwise returns the default from operator.json.
    pub fn get_string(&self, name: &str) -> String {
        // User value takes precedence
        if let Some(value) = self.user_values.get(name) {
            return value.clone();
        }

        // Fall back to operator.json default
        registry().get_default(name).unwrap_or("").to_string()
    }

    /// Get enumerated property with validation
    ///
    /// Returns the user-set value if valid. Errors if user value is invalid.
    /// Returns the default from operator.json if no user value is set.
    pub fn get_enum(&self, name: &str) -> Result<String, String> {
        let reg = registry();
        let default = reg.get_default(name).unwrap_or("");

        if let Some(value) = self.user_values.get(name) {
            if reg.is_valid_enum_value(name, value) {
                return Ok(value.clone());
            } else {
                let valid_values = reg
                    .get_property(name)
                    .and_then(|p| p.valid_values.as_ref())
                    .map(|v| v.join(", "))
                    .unwrap_or_default();
                return Err(format!(
                    "Invalid value '{}' for property '{}'. Valid values: [{}]",
                    value, name, valid_values
                ));
            }
        }

        Ok(default.to_string())
    }

    /// Get optional string property (None if empty)
    pub fn get_optional_string(&self, name: &str) -> Option<String> {
        let value = self.get_string(name);
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    }

    /// Get f64 property with validation
    ///
    /// Parses the string value as f64. Errors if the user-provided value
    /// cannot be parsed as a number.
    pub fn get_f64(&self, name: &str) -> Result<f64, String> {
        let value = self.get_string(name);
        let default_str = registry().get_default(name).unwrap_or("0");
        let default = default_str.parse::<f64>().unwrap_or(0.0);

        if value.is_empty() {
            return Ok(default);
        }

        value.parse::<f64>().map_err(|_| {
            format!(
                "Invalid numeric value '{}' for property '{}'. Expected a number.",
                value, name
            )
        })
    }

    /// Get f64 property with range validation
    ///
    /// Errors if the value cannot be parsed or is outside [min, max].
    pub fn get_f64_in_range(&self, name: &str, min: f64, max: f64) -> Result<f64, String> {
        let value = self.get_f64(name)?;

        if value >= min && value <= max {
            Ok(value)
        } else {
            Err(format!(
                "Value {} for property '{}' out of range [{}, {}].",
                value, name, min, max
            ))
        }
    }

    /// Get i32 property with validation
    ///
    /// Errors if the user-provided value cannot be parsed as an integer.
    pub fn get_i32(&self, name: &str) -> Result<i32, String> {
        let value = self.get_string(name);
        let default_str = registry().get_default(name).unwrap_or("0");
        let default = default_str.parse::<i32>().unwrap_or(0);

        if value.is_empty() {
            return Ok(default);
        }

        value.parse::<i32>().map_err(|_| {
            format!(
                "Invalid integer value '{}' for property '{}'. Expected an integer.",
                value, name
            )
        })
    }

    /// Parse coordinate string "x,y" into (f64, f64)
    ///
    /// Format: "x,y" where x,y ∈ [0,1]
    /// Returns Ok(None) if empty, Err if format is invalid.
    pub fn get_coords(&self, name: &str) -> Result<Option<(f64, f64)>, String> {
        let value = self.get_string(name);
        if value.is_empty() {
            return Ok(None);
        }

        let parts: Vec<&str> = value.split(',').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid coordinate format '{}' for property '{}'. Expected 'x,y'.",
                value, name
            ));
        }

        let x = parts[0].trim().parse::<f64>().map_err(|_| {
            format!(
                "Invalid x coordinate in '{}' for property '{}'.",
                value, name
            )
        })?;

        let y = parts[1].trim().parse::<f64>().map_err(|_| {
            format!(
                "Invalid y coordinate in '{}' for property '{}'.",
                value, name
            )
        })?;

        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return Err(format!(
                "Coordinates '{}' for property '{}' out of range [0,1].",
                value, name
            ));
        }

        Ok(Some((x, y)))
    }

    /// Get boolean property with validation
    ///
    /// Accepts "true"/"1"/"yes" → true, "false"/"0"/"no"/"" → false.
    /// Errors if the value is not a recognized boolean.
    pub fn get_bool(&self, name: &str) -> Result<bool, String> {
        let value = self.get_string(name);
        match value.to_lowercase().as_str() {
            "true" | "1" | "yes" => Ok(true),
            "false" | "0" | "no" | "" => Ok(false),
            _ => Err(format!(
                "Invalid boolean value '{}' for property '{}'. Expected 'true' or 'false'.",
                value, name
            )),
        }
    }

    /// Get optional f64 property (None if empty)
    ///
    /// Returns None if the value is empty, Some(f64) if valid, or Err if invalid.
    pub fn get_optional_f64(&self, name: &str) -> Result<Option<f64>, String> {
        let value = self.get_string(name);
        if value.is_empty() {
            return Ok(None);
        }
        value.parse::<f64>().map(Some).map_err(|_| {
            format!(
                "Invalid numeric value '{}' for property '{}'. Expected a number.",
                value, name
            )
        })
    }

    /// Parse semicolon-separated list of shape codes
    ///
    /// Format: "19;15;17" -> vec![19, 15, 17]
    /// Errors if any entry is invalid or out of range (0-25).
    /// Returns default [19] (filled circle) if empty.
    pub fn get_shape_list(&self, name: &str) -> Result<Vec<i32>, String> {
        let value = self.get_string(name);
        if value.is_empty() {
            return Ok(vec![19]); // Default: filled circle
        }

        let mut shapes = Vec::new();
        for s in value.split(';') {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                continue;
            }
            let n = trimmed.parse::<i32>().map_err(|_| {
                format!(
                    "Invalid shape '{}' in property '{}'. Expected integer 0-25.",
                    trimmed, name
                )
            })?;
            if !(0..=25).contains(&n) {
                return Err(format!(
                    "Shape {} in property '{}' out of range. Must be 0-25.",
                    n, name
                ));
            }
            shapes.push(n);
        }

        if shapes.is_empty() {
            Ok(vec![19])
        } else {
            Ok(shapes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_loads() {
        let reg = registry();
        // Check some known properties exist
        assert!(reg.get_property("backend").is_some());
        assert!(reg.get_property("legend.position").is_some());
        assert!(reg.get_property("plot.width").is_some());
    }

    #[test]
    fn test_registry_defaults() {
        let reg = registry();
        assert_eq!(reg.get_default("backend"), Some("cpu"));
        assert_eq!(reg.get_default("legend.position"), Some("right"));
        assert_eq!(reg.get_default("png.compression"), Some("fast"));
    }

    #[test]
    fn test_enum_validation() {
        let reg = registry();
        assert!(reg.is_valid_enum_value("backend", "cpu"));
        assert!(reg.is_valid_enum_value("backend", "gpu"));
        assert!(!reg.is_valid_enum_value("backend", "invalid"));
    }

    #[test]
    fn test_property_reader_defaults() {
        let reader = OperatorPropertyReader::new(None);
        assert_eq!(reader.get_enum("backend").unwrap(), "cpu");
        assert_eq!(reader.get_enum("legend.position").unwrap(), "right");
        assert_eq!(reader.get_f64("point.size.multiplier").unwrap(), 1.0);
    }

    #[test]
    fn test_shape_list_default() {
        let reader = OperatorPropertyReader::new(None);
        let shapes = reader.get_shape_list("point.shapes").unwrap();
        assert_eq!(shapes, vec![19]); // Default from operator.json
    }

    #[test]
    fn test_bool_defaults() {
        let reader = OperatorPropertyReader::new(None);
        assert!(!reader.get_bool("grid.major.disable").unwrap());
        assert!(!reader.get_bool("grid.minor.disable").unwrap());
        assert!(!reader.get_bool("axis.lines.disable").unwrap());
        assert!(!reader.get_bool("text.disable").unwrap());
    }

    #[test]
    fn test_optional_f64_defaults() {
        let reader = OperatorPropertyReader::new(None);
        assert_eq!(
            reader.get_optional_f64("plot.title.font.size").unwrap(),
            None
        );
        assert_eq!(
            reader.get_optional_f64("axis.label.font.size").unwrap(),
            None
        );
        assert_eq!(
            reader.get_optional_f64("axis.tick.font.size").unwrap(),
            None
        );
        assert_eq!(reader.get_optional_f64("axis.line.width").unwrap(), None);
    }
}
