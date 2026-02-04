//! Color column processing for DataFrames
//!
//! Transforms color factor values into packed RGB colors for rendering.
//! This module handles both continuous (palette interpolation) and categorical
//! (level-based) color mapping.

use crate::tercen::{
    categorical_color_from_level, interpolate_color, ColorInfo, ColorMapping, ColorPalette,
};
use ggrs_core::data::DataFrame;
use polars::prelude::*;
use std::borrow::Cow;

/// Add packed RGB color column to DataFrame based on color factors
///
/// For continuous mapping: interpolates values using the palette
/// For categorical mapping: maps levels to default palette colors
///
/// # Arguments
/// * `df` - DataFrame with color factor column(s)
/// * `color_infos` - Color configuration (factor name, mapping, quartiles)
///
/// # Returns
/// DataFrame with `.color` column added (packed RGB as i64)
///
/// # Errors
/// Returns error if:
/// - Color column is missing from DataFrame
/// - Color column has wrong type for the mapping
/// - `.colorLevels` column missing for categorical mapping
pub fn add_color_columns(
    mut df: DataFrame,
    color_infos: &[ColorInfo],
) -> Result<DataFrame, Box<dyn std::error::Error>> {
    // For now, only use the first color factor
    // TODO: Handle multiple color factors (blend? choose first? user option?)
    let color_info = &color_infos[0];

    // Get mutable reference to inner Polars DataFrame (no cloning)
    let polars_df = df.inner_mut();

    // Generate RGB values based on mapping type
    let nrows = polars_df.height();
    let mut r_values = Vec::with_capacity(nrows);
    let mut g_values = Vec::with_capacity(nrows);
    let mut b_values = Vec::with_capacity(nrows);

    match &color_info.mapping {
        ColorMapping::Continuous(palette) => {
            add_continuous_colors(
                polars_df,
                color_info,
                palette,
                &mut r_values,
                &mut g_values,
                &mut b_values,
            )?;
        }

        ColorMapping::Categorical(color_map) => {
            add_categorical_colors(
                polars_df,
                color_info,
                color_map,
                &mut r_values,
                &mut g_values,
                &mut b_values,
            )?;
        }
    }

    // Pack RGB values directly as u32 (stored as i64 in Polars)
    // This avoids String allocation per point and hex parsing at render time
    // Memory saving: ~24MB for 475K points (Option<String> vs i64)
    let packed_colors: Vec<i64> = (0..r_values.len())
        .map(|i| ggrs_core::PackedRgba::rgb(r_values[i], g_values[i], b_values[i]).to_u32() as i64)
        .collect();

    // Add color column as packed integers
    polars_df.with_column(Series::new(".color".into(), packed_colors))?;

    // Debug: Print first color values
    if polars_df.height() > 0 {
        if let Ok(color_col) = polars_df.column(".color") {
            let int_col = color_col.i64().unwrap();
            let first_colors: Vec<String> = int_col
                .into_iter()
                .take(3)
                .map(|opt| {
                    opt.map(|v| {
                        let packed = ggrs_core::PackedRgba::from_u32(v as u32);
                        format!("RGB({},{},{})", packed.red(), packed.green(), packed.blue())
                    })
                    .unwrap_or_else(|| "NULL".to_string())
                })
                .collect();
            eprintln!("DEBUG: First 3 .color packed values: {:?}", first_colors);
        }
    }

    Ok(df)
}

/// Add continuous colors using palette interpolation
fn add_continuous_colors(
    polars_df: &polars::frame::DataFrame,
    color_info: &ColorInfo,
    palette: &ColorPalette,
    r_values: &mut Vec<u8>,
    g_values: &mut Vec<u8>,
    b_values: &mut Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    let color_col_name = &color_info.factor_name;
    eprintln!(
        "DEBUG add_color_columns: Using continuous color mapping for '{}', is_user_defined={}",
        color_col_name, palette.is_user_defined
    );

    // Rescale palette if is_user_defined=false and quartiles are available
    let effective_palette: Cow<'_, ColorPalette> = if !palette.is_user_defined {
        if let Some(ref quartiles) = color_info.quartiles {
            eprintln!(
                "DEBUG add_color_columns: Rescaling palette using quartiles: {:?}",
                quartiles
            );
            let rescaled = palette.rescale_from_quartiles(quartiles);
            eprintln!(
                "DEBUG add_color_columns: Original range: {:?}, Rescaled range: {:?}",
                palette.range(),
                rescaled.range()
            );
            Cow::Owned(rescaled)
        } else {
            eprintln!(
                "WARN add_color_columns: is_user_defined=false but no quartiles available, using original palette"
            );
            Cow::Borrowed(palette)
        }
    } else {
        Cow::Borrowed(palette)
    };

    // Get the color factor column
    let color_series = polars_df
        .column(color_col_name)
        .map_err(|e| format!("Color column '{}' not found: {}", color_col_name, e))?;

    // Extract f64 values
    let color_values = color_series.f64().map_err(|e| {
        format!(
            "Color column '{}' is not f64 for continuous mapping: {}",
            color_col_name, e
        )
    })?;

    // Debug: Print first few color factor values to verify we're getting expected data
    let sample_values: Vec<f64> = color_values.iter().take(5).flatten().collect();
    if !sample_values.is_empty() {
        let min_val = color_values.min().unwrap_or(0.0);
        let max_val = color_values.max().unwrap_or(0.0);
        eprintln!(
            "DEBUG add_color_columns: {} values range [{:.2}, {:.2}], first 5: {:?}",
            color_col_name, min_val, max_val, sample_values
        );
    }

    // Map each value to RGB using palette interpolation
    for opt_value in color_values.iter() {
        if let Some(value) = opt_value {
            let rgb = interpolate_color(value, &effective_palette);
            r_values.push(rgb[0]);
            g_values.push(rgb[1]);
            b_values.push(rgb[2]);
        } else {
            // Handle null values with a default color (gray)
            r_values.push(128);
            g_values.push(128);
            b_values.push(128);
        }
    }

    Ok(())
}

/// Add categorical colors using level mapping or explicit category mappings
fn add_categorical_colors(
    polars_df: &polars::frame::DataFrame,
    color_info: &ColorInfo,
    color_map: &crate::tercen::CategoryColorMap,
    r_values: &mut Vec<u8>,
    g_values: &mut Vec<u8>,
    b_values: &mut Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("DEBUG add_color_columns: Using categorical color mapping");
    eprintln!(
        "DEBUG add_color_columns: Category map has {} entries",
        color_map.mappings.len()
    );

    // For categorical colors, Tercen uses .colorLevels column (int32) with level indices
    // If color_map has explicit mappings, use them; otherwise generate from levels
    let use_levels = color_map.mappings.is_empty();

    if use_levels {
        add_categorical_colors_from_levels(polars_df, r_values, g_values, b_values)?;
    } else {
        add_categorical_colors_from_mappings(
            polars_df, color_info, color_map, r_values, g_values, b_values,
        )?;
    }

    Ok(())
}

/// Map .colorLevels column to colors using default categorical palette
fn add_categorical_colors_from_levels(
    polars_df: &polars::frame::DataFrame,
    r_values: &mut Vec<u8>,
    g_values: &mut Vec<u8>,
    b_values: &mut Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("DEBUG add_color_columns: Using .colorLevels column for categorical colors");

    // Get .colorLevels column instead of the factor column
    let levels_series = polars_df
        .column(".colorLevels")
        .map_err(|e| format!("Categorical colors require .colorLevels column: {}", e))?;

    // Schema says int32 but it comes back as i64, so accept both
    let levels = levels_series
        .i64()
        .map_err(|e| format!(".colorLevels column is not i64: {}", e))?;

    // Map each level to RGB using default categorical palette
    for opt_level in levels.iter() {
        if let Some(level) = opt_level {
            let rgb = categorical_color_from_level(level as i32);
            r_values.push(rgb[0]);
            g_values.push(rgb[1]);
            b_values.push(rgb[2]);
        } else {
            // Handle null values with a default color (gray)
            r_values.push(128);
            g_values.push(128);
            b_values.push(128);
        }
    }

    Ok(())
}

/// Map categorical values using explicit category→color mappings
fn add_categorical_colors_from_mappings(
    polars_df: &polars::frame::DataFrame,
    color_info: &ColorInfo,
    color_map: &crate::tercen::CategoryColorMap,
    r_values: &mut Vec<u8>,
    g_values: &mut Vec<u8>,
    b_values: &mut Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    let color_col_name = &color_info.factor_name;
    eprintln!(
        "DEBUG add_color_columns: Using explicit category mappings for '{}'",
        color_col_name
    );

    // Get the color factor column
    let color_series = polars_df
        .column(color_col_name)
        .map_err(|e| format!("Color column '{}' not found: {}", color_col_name, e))?;

    let color_values = color_series.str().map_err(|e| {
        format!(
            "Color column '{}' is not string for categorical mapping: {}",
            color_col_name, e
        )
    })?;

    for opt_value in color_values.iter() {
        if let Some(category) = opt_value {
            let rgb = color_map
                .mappings
                .get(category)
                .unwrap_or(&color_map.default_color);
            r_values.push(rgb[0]);
            g_values.push(rgb[1]);
            b_values.push(rgb[2]);
        } else {
            r_values.push(128);
            g_values.push(128);
            b_values.push(128);
        }
    }

    Ok(())
}
