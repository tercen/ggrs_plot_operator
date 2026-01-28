//! Color palette handling and RGB interpolation for continuous and categorical color scales
//!
//! This module provides functionality to:
//! - Parse Tercen color palettes (JetPalette, RampPalette, CategoryPalette)
//! - Interpolate color values to RGB (continuous)
//! - Map category strings to RGB (categorical)
//! - Extract color information from workflow steps

use super::client::proto;
use super::error::{Result, TercenError};
use std::collections::HashMap;

/// Information about a color factor and its associated palette
#[derive(Debug, Clone)]
pub struct ColorInfo {
    /// Name of the column containing color values (e.g., "Age", "Country")
    pub factor_name: String,
    /// Type of the factor (e.g., "double", "int32", "string")
    pub factor_type: String,
    /// The color mapping for this factor
    pub mapping: ColorMapping,
    /// Optional color table ID (for categorical colors with .colorLevels)
    /// This table contains the mapping from level index to category name
    pub color_table_id: Option<String>,
}

/// Color mapping - either continuous interpolation or categorical lookup
#[derive(Debug, Clone)]
pub enum ColorMapping {
    /// Continuous color scale: numeric value → RGB via interpolation
    Continuous(ColorPalette),
    /// Categorical color scale: string value → RGB via lookup
    Categorical(CategoryColorMap),
}

/// A color palette with sorted color stops for interpolation
#[derive(Debug, Clone)]
pub struct ColorPalette {
    /// Sorted list of color stops (by value, ascending)
    pub stops: Vec<ColorStop>,
}

/// A single color stop in a palette
#[derive(Debug, Clone, PartialEq)]
pub struct ColorStop {
    /// Numeric value at this stop
    pub value: f64,
    /// RGB color at this stop
    pub color: [u8; 3], // [r, g, b]
}

/// Categorical color mapping: string → RGB
#[derive(Debug, Clone)]
pub struct CategoryColorMap {
    /// Map from category string to RGB color
    pub mappings: HashMap<String, [u8; 3]>,
    /// Default color for unknown categories
    pub default_color: [u8; 3],
}

impl ColorPalette {
    /// Create a new empty palette
    pub fn new() -> Self {
        ColorPalette { stops: Vec::new() }
    }

    /// Add a color stop and maintain sorted order
    pub fn add_stop(&mut self, value: f64, color: [u8; 3]) {
        let stop = ColorStop { value, color };
        // Insert in sorted position
        match self
            .stops
            .binary_search_by(|s| s.value.partial_cmp(&value).unwrap())
        {
            Ok(pos) => self.stops[pos] = stop, // Replace if exists
            Err(pos) => self.stops.insert(pos, stop),
        }
    }

    /// Get the value range of this palette
    pub fn range(&self) -> Option<(f64, f64)> {
        if self.stops.is_empty() {
            None
        } else {
            Some((
                self.stops.first().unwrap().value,
                self.stops.last().unwrap().value,
            ))
        }
    }
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a Tercen EPalette proto into a ColorMapping
pub fn parse_palette(e_palette: &proto::EPalette) -> Result<ColorMapping> {
    let palette_obj = e_palette
        .object
        .as_ref()
        .ok_or_else(|| TercenError::Data("EPalette has no object".to_string()))?;

    match palette_obj {
        proto::e_palette::Object::Jetpalette(jet) => {
            Ok(ColorMapping::Continuous(parse_jet_palette(jet)?))
        }
        proto::e_palette::Object::Ramppalette(ramp) => {
            Ok(ColorMapping::Continuous(parse_ramp_palette(ramp)?))
        }
        proto::e_palette::Object::Categorypalette(cat) => {
            Ok(ColorMapping::Categorical(parse_category_palette(cat)?))
        }
        proto::e_palette::Object::Palette(_) => Err(TercenError::Data(
            "Base Palette type not supported".to_string(),
        )),
    }
}

/// Parse a JetPalette into a ColorPalette
fn parse_jet_palette(jet: &proto::JetPalette) -> Result<ColorPalette> {
    parse_double_color_elements(&jet.double_color_elements)
}

/// Parse a RampPalette into a ColorPalette
fn parse_ramp_palette(ramp: &proto::RampPalette) -> Result<ColorPalette> {
    parse_double_color_elements(&ramp.double_color_elements)
}

/// Parse a CategoryPalette into a CategoryColorMap
///
/// For categorical colors, Tercen stores color levels (indices) in the `.colorLevels` column
/// of the main data table. The actual category strings are in a separate color table.
///
/// If the palette has `stringColorElements`, use those explicit mappings.
/// Otherwise, we'll create mappings later from the data (using `.colorLevels`).
fn parse_category_palette(cat: &proto::CategoryPalette) -> Result<CategoryColorMap> {
    let mut mappings = HashMap::new();

    eprintln!(
        "DEBUG parse_category_palette: Processing {} string color elements",
        cat.string_color_elements.len()
    );

    // If we have explicit string→color mappings, use them
    if !cat.string_color_elements.is_empty() {
        for (i, element) in cat.string_color_elements.iter().enumerate() {
            let category = element.string_value.clone();
            let rgb = int_to_rgb(element.color);

            eprintln!(
                "DEBUG parse_category_palette: [{}] '{}' → RGB({}, {}, {})",
                i, category, rgb[0], rgb[1], rgb[2]
            );

            mappings.insert(category, rgb);
        }
    } else {
        // No explicit mappings - colors will be generated from .colorLevels in the data
        // The actual mapping happens in the stream generator when we see the data
        eprintln!(
            "DEBUG parse_category_palette: No string_color_elements, will use .colorLevels from data"
        );
        if let Some(ref color_list) = cat.color_list {
            eprintln!(
                "DEBUG parse_category_palette: ColorList name: '{}'",
                color_list.name
            );
        }
    }

    Ok(CategoryColorMap {
        mappings,
        default_color: [128, 128, 128], // Gray for unknown categories
    })
}

/// Parse DoubleColorElement array into ColorPalette
fn parse_double_color_elements(elements: &[proto::DoubleColorElement]) -> Result<ColorPalette> {
    let mut palette = ColorPalette::new();

    for element in elements {
        let value = element.string_value.parse::<f64>().map_err(|e| {
            TercenError::Data(format!(
                "Invalid color value '{}': {}",
                element.string_value, e
            ))
        })?;

        eprintln!(
            "DEBUG parse_palette: element color_int={}, stringValue={}",
            element.color, element.string_value
        );

        // Tercen uses -1 as a sentinel for "no color defined" - use default gradient
        let color = if element.color == -1 {
            // Use viridis-like default gradient based on position
            let t = if elements.len() > 1 {
                (palette.stops.len() as f64) / ((elements.len() - 1) as f64)
            } else {
                0.5
            };
            // Simple blue to yellow gradient
            let r = (t * 255.0) as u8;
            let g = (t * 255.0) as u8;
            let b = ((1.0 - t) * 255.0) as u8;
            eprintln!(
                "DEBUG parse_palette: Using default gradient at t={:.2}, RGB({}, {}, {})",
                t, r, g, b
            );
            [r, g, b]
        } else {
            let rgb = int_to_rgb(element.color);
            eprintln!(
                "DEBUG parse_palette: Parsed to RGB({}, {}, {})",
                rgb[0], rgb[1], rgb[2]
            );
            rgb
        };

        palette.add_stop(value, color);
    }

    if palette.stops.is_empty() {
        return Err(TercenError::Data("Palette has no color stops".to_string()));
    }

    Ok(palette)
}

/// Convert Tercen color integer (AARRGGBB) to RGB array
///
/// Tercen stores colors as 32-bit integers with the format:
/// - Bits 24-31: Alpha (ignored for now)
/// - Bits 16-23: Red
/// - Bits 8-15: Green
/// - Bits 0-7: Blue
fn int_to_rgb(color_int: i32) -> [u8; 3] {
    let color = color_int as u32;
    [
        ((color >> 16) & 0xFF) as u8, // Red
        ((color >> 8) & 0xFF) as u8,  // Green
        (color & 0xFF) as u8,         // Blue
    ]
}

/// Extract color information from a workflow step
///
/// Navigates to step.model.axis.xyAxis[0].colors and extracts:
/// - Color factors (column names and types)
/// - Associated palettes
/// - Optional color table IDs (indexed by factor position, e.g., color_0, color_1)
///
/// Returns a Vec<ColorInfo> (can be empty if no colors defined)
pub fn extract_color_info_from_step(
    workflow: &proto::Workflow,
    step_id: &str,
    color_table_ids: &[Option<String>],
) -> Result<Vec<ColorInfo>> {
    // Find the step by ID
    let step = workflow
        .steps
        .iter()
        .find(|s| {
            if let Some(proto::e_step::Object::Datastep(ds)) = &s.object {
                ds.id == step_id
            } else {
                false
            }
        })
        .ok_or_else(|| TercenError::Data(format!("Step '{}' not found in workflow", step_id)))?;

    // Extract DataStep
    let data_step = match &step.object {
        Some(proto::e_step::Object::Datastep(ds)) => ds,
        _ => return Err(TercenError::Data("Step is not a DataStep".to_string())),
    };

    // Navigate to model.axis.xyAxis
    let model = data_step
        .model
        .as_ref()
        .ok_or_else(|| TercenError::Data("DataStep has no model".to_string()))?;

    let axis = model
        .axis
        .as_ref()
        .ok_or_else(|| TercenError::Data("Model has no axis".to_string()))?;

    // Get first xyAxis (usually there's only one for plot operators)
    let xy_axis = axis
        .xy_axis
        .first()
        .ok_or_else(|| TercenError::Data("Axis has no xyAxis array".to_string()))?;

    // Extract colors object
    let colors = match &xy_axis.colors {
        Some(c) => c,
        None => {
            eprintln!("DEBUG extract_color_info: No colors object in xyAxis");
            return Ok(Vec::new()); // No colors defined - this is OK
        }
    };

    eprintln!(
        "DEBUG extract_color_info: Found colors object with {} factors",
        colors.factors.len()
    );
    eprintln!(
        "DEBUG extract_color_info: Palette present: {}",
        colors.palette.is_some()
    );

    // Parse each color factor
    let mut color_infos = Vec::new();
    for (i, factor) in colors.factors.iter().enumerate() {
        eprintln!(
            "DEBUG extract_color_info: Processing factor {}: name='{}', type='{}'",
            i, factor.name, factor.r#type
        );

        // Parse the palette/mapping
        let mapping = match &colors.palette {
            Some(p) => {
                eprintln!("DEBUG extract_color_info: Calling parse_palette...");
                let parsed = parse_palette(p)?;
                match &parsed {
                    ColorMapping::Continuous(palette) => {
                        eprintln!(
                            "DEBUG extract_color_info: Continuous palette with {} stops",
                            palette.stops.len()
                        );
                    }
                    ColorMapping::Categorical(color_map) => {
                        eprintln!(
                            "DEBUG extract_color_info: Categorical palette with {} categories",
                            color_map.mappings.len()
                        );
                    }
                }
                parsed
            }
            None => {
                return Err(TercenError::Data(
                    "Color factors defined but no palette provided".to_string(),
                ))
            }
        };

        // Get the color table ID for this factor (if available)
        let color_table_id = color_table_ids.get(i).and_then(|opt| opt.clone());

        color_infos.push(ColorInfo {
            factor_name: factor.name.clone(),
            factor_type: factor.r#type.clone(),
            mapping,
            color_table_id,
        });
    }

    eprintln!(
        "DEBUG extract_color_info: Returning {} ColorInfo objects",
        color_infos.len()
    );
    Ok(color_infos)
}

/// Generate a categorical color from a level index using a default palette
///
/// Uses a qualitative color scheme similar to R's default categorical colors.
/// Colors repeat after 12 levels.
pub fn categorical_color_from_level(level: i32) -> [u8; 3] {
    // Default categorical palette (similar to R's default colors)
    // Based on a qualitative color scheme with good distinguishability
    const CATEGORICAL_COLORS: [[u8; 3]; 12] = [
        [228, 26, 28],   // Red
        [55, 126, 184],  // Blue
        [77, 175, 74],   // Green
        [152, 78, 163],  // Purple
        [255, 127, 0],   // Orange
        [255, 255, 51],  // Yellow
        [166, 86, 40],   // Brown
        [247, 129, 191], // Pink
        [153, 153, 153], // Gray
        [102, 194, 165], // Teal
        [252, 141, 98],  // Coral
        [141, 160, 203], // Lavender
    ];

    let index = (level as usize) % CATEGORICAL_COLORS.len();
    CATEGORICAL_COLORS[index]
}

/// Interpolate a color value using the palette
///
/// Uses linear interpolation between the surrounding color stops.
/// Values outside the palette range clamp to the min/max colors.
pub fn interpolate_color(value: f64, palette: &ColorPalette) -> [u8; 3] {
    if palette.stops.is_empty() {
        return [128, 128, 128]; // Gray default
    }

    let stops = &palette.stops;

    // Clamp to min
    if value <= stops.first().unwrap().value {
        return stops.first().unwrap().color;
    }

    // Clamp to max
    if value >= stops.last().unwrap().value {
        return stops.last().unwrap().color;
    }

    // Find surrounding stops using binary search
    let idx = stops.partition_point(|stop| stop.value < value);
    let lower = &stops[idx - 1];
    let upper = &stops[idx];

    // Linear interpolation
    let t = (value - lower.value) / (upper.value - lower.value);
    [
        (lower.color[0] as f64 * (1.0 - t) + upper.color[0] as f64 * t) as u8,
        (lower.color[1] as f64 * (1.0 - t) + upper.color[1] as f64 * t) as u8,
        (lower.color[2] as f64 * (1.0 - t) + upper.color[2] as f64 * t) as u8,
    ]
}

/// Extract point size from workflow step
///
/// Returns the pointSize from the chart configuration (1-10 scale from UI).
/// Returns None if not found, in which case the caller should use a default.
pub fn extract_point_size_from_step(
    workflow: &proto::Workflow,
    step_id: &str,
) -> Result<Option<i32>> {
    // Find the step
    let step = workflow
        .steps
        .iter()
        .find(|s| {
            if let Some(proto::e_step::Object::Datastep(ds)) = &s.object {
                ds.id == step_id
            } else {
                false
            }
        })
        .ok_or_else(|| TercenError::Data(format!("Step '{}' not found in workflow", step_id)))?;

    // Extract DataStep
    let data_step = match &step.object {
        Some(proto::e_step::Object::Datastep(ds)) => ds,
        _ => return Err(TercenError::Data("Step is not a DataStep".to_string())),
    };

    // Navigate to model.axis.xyAxis
    let model = match data_step.model.as_ref() {
        Some(m) => m,
        None => return Ok(None), // No model, use default
    };

    let axis = match model.axis.as_ref() {
        Some(a) => a,
        None => return Ok(None), // No axis, use default
    };

    // Get first xyAxis
    let xy_axis = match axis.xy_axis.first() {
        Some(xy) => xy,
        None => return Ok(None), // No xyAxis, use default
    };

    // Extract pointSize from chart
    let chart = match xy_axis.chart.as_ref() {
        Some(c) => c,
        None => return Ok(None), // No chart, use default
    };

    // Check the chart type and extract pointSize
    let point_size = match &chart.object {
        Some(proto::e_chart::Object::Chartpoint(cp)) => Some(cp.point_size),
        Some(proto::e_chart::Object::Chartline(cl)) => Some(cl.point_size),
        _ => None, // Other chart types don't have pointSize
    };

    eprintln!(
        "DEBUG extract_point_size: Found pointSize = {:?}",
        point_size
    );

    Ok(point_size)
}

/// Chart type variants supported by Tercen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartKind {
    /// Scatter plot (default)
    #[default]
    Point,
    /// Heatmap (tile-based visualization)
    Heatmap,
    /// Line plot
    Line,
    /// Bar chart
    Bar,
}

/// Extract chart type from workflow step
///
/// Navigates: workflow.steps[step_id].model.axis.xyAxis[0].chart.object
/// Returns ChartKind based on the EChart variant.
pub fn extract_chart_kind_from_step(
    workflow: &proto::Workflow,
    step_id: &str,
) -> Result<ChartKind> {
    // Find the step - check both DataStep and CrossTabStep
    let step = workflow
        .steps
        .iter()
        .find(|s| match &s.object {
            Some(proto::e_step::Object::Datastep(ds)) => ds.id == step_id,
            Some(proto::e_step::Object::Crosstabstep(cs)) => cs.id == step_id,
            _ => false,
        })
        .ok_or_else(|| TercenError::Data(format!("Step '{}' not found in workflow", step_id)))?;

    // Extract the Crosstab model (both DataStep and CrossTabStep have it)
    let model = match &step.object {
        Some(proto::e_step::Object::Datastep(ds)) => ds.model.as_ref(),
        Some(proto::e_step::Object::Crosstabstep(cs)) => cs.model.as_ref(),
        _ => {
            return Err(TercenError::Data(
                "Step type does not have a model".to_string(),
            ))
        }
    }
    .ok_or_else(|| TercenError::Data("Step has no model".to_string()))?;

    // Navigate to model.axis.xyAxis
    let axis = match model.axis.as_ref() {
        Some(a) => a,
        None => {
            eprintln!("DEBUG extract_chart_kind: No axis in model, defaulting to Point");
            return Ok(ChartKind::Point);
        }
    };

    // Get first xyAxis
    let xy_axis = match axis.xy_axis.first() {
        Some(xy) => xy,
        None => {
            eprintln!("DEBUG extract_chart_kind: No xyAxis, defaulting to Point");
            return Ok(ChartKind::Point);
        }
    };

    // Extract chart type from EChart
    let chart = match xy_axis.chart.as_ref() {
        Some(c) => c,
        None => {
            eprintln!("DEBUG extract_chart_kind: No chart in xyAxis, defaulting to Point");
            return Ok(ChartKind::Point);
        }
    };

    // Map EChart variant to ChartKind
    let chart_kind = match &chart.object {
        Some(proto::e_chart::Object::Chartpoint(_)) => ChartKind::Point,
        Some(proto::e_chart::Object::Chartheatmap(_)) => ChartKind::Heatmap,
        Some(proto::e_chart::Object::Chartline(_)) => ChartKind::Line,
        Some(proto::e_chart::Object::Chartbar(_)) => ChartKind::Bar,
        Some(proto::e_chart::Object::Chart(_)) => ChartKind::Point, // Generic chart defaults to point
        Some(proto::e_chart::Object::Chartsize(_)) => ChartKind::Point, // Size chart treated as point
        None => ChartKind::Point,
    };

    eprintln!(
        "DEBUG extract_chart_kind: Found chart type = {:?}",
        chart_kind
    );

    Ok(chart_kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int_to_rgb() {
        // White: 0xFFFFFFFF
        assert_eq!(int_to_rgb(-1), [255, 255, 255]);

        // Red: 0x00FF0000
        assert_eq!(int_to_rgb(0x00FF0000u32 as i32), [255, 0, 0]);

        // Green: 0x0000FF00
        assert_eq!(int_to_rgb(0x0000FF00u32 as i32), [0, 255, 0]);

        // Blue: 0x000000FF
        assert_eq!(int_to_rgb(0x000000FFu32 as i32), [0, 0, 255]);

        // Gray: 0x00808080
        assert_eq!(int_to_rgb(0x00808080u32 as i32), [128, 128, 128]);
    }

    #[test]
    fn test_palette_add_stop() {
        let mut palette = ColorPalette::new();
        palette.add_stop(0.0, [0, 0, 0]);
        palette.add_stop(100.0, [255, 255, 255]);
        palette.add_stop(50.0, [128, 128, 128]);

        assert_eq!(palette.stops.len(), 3);
        assert_eq!(palette.stops[0].value, 0.0);
        assert_eq!(palette.stops[1].value, 50.0);
        assert_eq!(palette.stops[2].value, 100.0);
    }

    #[test]
    fn test_interpolate_color_edge_cases() {
        let mut palette = ColorPalette::new();
        palette.add_stop(0.0, [0, 0, 0]);
        palette.add_stop(100.0, [255, 255, 255]);

        // Below min - clamps to first color
        assert_eq!(interpolate_color(-10.0, &palette), [0, 0, 0]);

        // At min
        assert_eq!(interpolate_color(0.0, &palette), [0, 0, 0]);

        // At max
        assert_eq!(interpolate_color(100.0, &palette), [255, 255, 255]);

        // Above max - clamps to last color
        assert_eq!(interpolate_color(110.0, &palette), [255, 255, 255]);
    }

    #[test]
    fn test_interpolate_color_midpoint() {
        let mut palette = ColorPalette::new();
        palette.add_stop(0.0, [0, 0, 0]);
        palette.add_stop(100.0, [100, 200, 255]);

        // Midpoint
        let mid = interpolate_color(50.0, &palette);
        assert_eq!(mid, [50, 100, 127]); // (0+100)/2, (0+200)/2, (0+255)/2 rounded
    }

    #[test]
    fn test_palette_range() {
        let mut palette = ColorPalette::new();
        assert_eq!(palette.range(), None);

        palette.add_stop(10.0, [0, 0, 0]);
        palette.add_stop(50.0, [255, 255, 255]);

        assert_eq!(palette.range(), Some((10.0, 50.0)));
    }
}
