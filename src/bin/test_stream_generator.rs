//! Test binary for TercenStreamGenerator
//!
//! This is a standalone test program to verify the GGRS integration works
//! using workflow and step IDs (like Python's OperatorContextDev).
//!
//! Usage:
//! ```bash
//! export TERCEN_URI=https://tercen.com:5400
//! export TERCEN_TOKEN=your_token_here
//! export WORKFLOW_ID=your_workflow_id
//! export STEP_ID=your_step_id
//! cargo run --bin test_stream_generator
//! ```

use ggrs_plot_operator::config::OperatorConfig;
use ggrs_plot_operator::ggrs_integration::TercenStreamGenerator;
use ggrs_plot_operator::tercen::{DevContext, TercenClient, TercenContext};
use std::sync::Arc;
use std::time::Instant;

fn log_phase(start: Instant, phase: &str) {
    let elapsed = start.elapsed();
    eprintln!("[PHASE @{:.3}s] {}", elapsed.as_secs_f64(), phase);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    log_phase(start, "START: Test initialization");
    println!("=== TercenStreamGenerator Test ===\n");

    // Read environment variables
    let uri = std::env::var("TERCEN_URI").unwrap_or_else(|_| "https://tercen.com:5400".to_string());
    let token =
        std::env::var("TERCEN_TOKEN").expect("TERCEN_TOKEN environment variable is required");
    let workflow_id =
        std::env::var("WORKFLOW_ID").expect("WORKFLOW_ID environment variable is required");
    let step_id = std::env::var("STEP_ID").expect("STEP_ID environment variable is required");

    // Load operator configuration from operator_config.json if it exists
    let config = load_test_config();

    println!("Configuration:");
    println!("  URI: {}", uri);
    println!("  Token: {}...", &token[..10.min(token.len())]);
    println!("  Workflow ID: {}", workflow_id);
    println!("  Step ID: {}", step_id);
    println!("  Chunk size: {}", config.chunk_size);
    println!();

    // Connect to Tercen
    log_phase(start, "PHASE 1: Connecting to Tercen");
    println!("Connecting to Tercen...");
    std::env::set_var("TERCEN_URI", &uri);
    std::env::set_var("TERCEN_TOKEN", &token);

    let client = TercenClient::from_env().await?;
    let client_arc = Arc::new(client);
    println!("✓ Connected successfully\n");

    // Create DevContext (fetches workflow, step, CubeQuery, colors, etc.)
    log_phase(start, "PHASE 2: Creating DevContext");
    println!("Creating DevContext from workflow/step...");
    let ctx = DevContext::from_workflow_step(client_arc.clone(), &workflow_id, &step_id).await?;

    println!("✓ Context created");
    println!("  Main table (qt_hash): {}", ctx.qt_hash());
    println!("  Column table (column_hash): {}", ctx.column_hash());
    println!("  Row table (row_hash): {}", ctx.row_hash());
    println!("  Schema IDs: {}", ctx.schema_ids().len());

    // Display color information
    println!("\n=== Color Information ===");
    if ctx.color_infos().is_empty() {
        println!("  No color factors defined");
    } else {
        for (i, info) in ctx.color_infos().iter().enumerate() {
            println!("  Color {} : '{}'", i + 1, info.factor_name);
            println!("    Type: {}", info.factor_type);
            match &info.mapping {
                ggrs_plot_operator::tercen::ColorMapping::Continuous(palette) => {
                    if let Some((min, max)) = palette.range() {
                        println!("    Range: {} to {}", min, max);
                        println!("    Stops: {}", palette.stops.len());
                    }
                }
                ggrs_plot_operator::tercen::ColorMapping::Categorical(color_map) => {
                    println!("    Categories: {}", color_map.mappings.len());
                }
            }
        }
    }

    // Extract page factors and values
    log_phase(start, "PHASE 2.5: Extracting page information");
    println!("\n=== Page Information ===");
    if ctx.page_factors().is_empty() {
        println!("  No page factors defined - will generate single plot");
    } else {
        println!("  Page factors: {:?}", ctx.page_factors());
    }

    // Extract unique page values from row facet table
    let page_values = ggrs_plot_operator::tercen::extract_page_values(
        ctx.client(),
        ctx.row_hash(),
        ctx.page_factors(),
    )
    .await?;

    println!("  Pages to generate: {}", page_values.len());
    for (i, page_value) in page_values.iter().enumerate() {
        println!("    Page {}: {}", i + 1, page_value.label);
    }

    // Create shared disk cache for all pages (only if multiple pages)
    use ggrs_core::stream::DataCache;
    let cache = if page_values.len() > 1 {
        let cache = DataCache::new(&workflow_id, &step_id)?;
        println!(
            "  Created disk cache at /tmp/ggrs_cache_{}_{}/",
            workflow_id, step_id
        );
        Some(cache)
    } else {
        println!("  Single page - cache disabled");
        None
    };

    // Loop through pages (or single iteration if no pages)
    for (page_idx, page_value) in page_values.iter().enumerate() {
        if page_values.len() > 1 {
            println!("\n============================================");
            println!(
                "=== Generating Page {}/{}: {} ===",
                page_idx + 1,
                page_values.len(),
                page_value.label
            );
            println!("============================================\n");
        }

        // Create stream generator
        log_phase(
            start,
            &format!(
                "PHASE 3: Creating StreamGenerator for page {}",
                page_idx + 1
            ),
        );
        println!("Creating TercenStreamGenerator...");

        let page_filter = if page_values.len() > 1 {
            Some(&page_value.values)
        } else {
            None
        };

        let stream_gen = TercenStreamGenerator::new(
            client_arc.clone(),
            ctx.qt_hash().to_string(),
            ctx.column_hash().to_string(),
            ctx.row_hash().to_string(),
            ctx.y_axis_table_id().map(|s| s.to_string()),
            config.chunk_size,
            ctx.color_infos().to_vec(),
            ctx.page_factors().to_vec(),
            page_filter,
        )
        .await?;

        log_phase(
            start,
            &format!(
                "PHASE 3 COMPLETE: StreamGenerator created for page {}",
                page_idx + 1
            ),
        );
        println!("✓ Stream generator created successfully\n");

        // Test facet metadata
        println!("=== Facet Information ===");
        println!("Column facets: {}", stream_gen.n_col_facets());
        println!("Row facets: {}", stream_gen.n_row_facets());
        println!(
            "Total facet cells: {}",
            stream_gen.n_col_facets() * stream_gen.n_row_facets()
        );
        println!();

        // Test axis ranges
        println!("=== Testing Axis Ranges ===");
        for col_idx in 0..stream_gen.n_col_facets().min(3) {
            for row_idx in 0..stream_gen.n_row_facets().min(3) {
                println!("Facet cell ({}, {}):", col_idx, row_idx);

                let x_axis = stream_gen.query_x_axis(col_idx, row_idx);
                let y_axis = stream_gen.query_y_axis(col_idx, row_idx);

                match x_axis {
                    ggrs_core::stream::AxisData::Numeric(data) => {
                        println!(
                            "  X-axis: [{:.2}, {:.2}] (data: [{:.2}, {:.2}])",
                            data.min_axis, data.max_axis, data.min_value, data.max_value
                        );
                    }
                    _ => println!("  X-axis: Categorical"),
                }

                match y_axis {
                    ggrs_core::stream::AxisData::Numeric(data) => {
                        println!(
                            "  Y-axis: [{:.2}, {:.2}] (data: [{:.2}, {:.2}])",
                            data.min_axis, data.max_axis, data.min_value, data.max_value
                        );
                    }
                    _ => println!("  Y-axis: Categorical"),
                }
                println!();
            }
        }

        // Test data querying
        log_phase(start, "PHASE 4: Testing data query (100 rows)");
        println!("=== Testing Data Query ===");
        use ggrs_core::stream::{Range, StreamGenerator};

        let test_range = Range::new(0, 100);

        println!("Querying bulk data, range 0-100...");
        let data = stream_gen.query_data_multi_facet(test_range);

        log_phase(start, "PHASE 4 COMPLETE: Data query finished");
        println!("✓ Received {} rows", data.nrow());
        println!("  Columns: {:?}", data.columns());

        if data.nrow() > 0 {
            println!("\nFirst 5 rows:");
            for i in 0..5.min(data.nrow()) {
                print!("  Row {}: ", i);

                // Print color RGB values if present
                if let Ok(r) = data.get_value(i, ".color_r") {
                    if let Ok(g) = data.get_value(i, ".color_g") {
                        if let Ok(b) = data.get_value(i, ".color_b") {
                            print!("RGB=({:?},{:?},{:?})", r, g, b);
                        }
                    }
                }
                println!();
            }
        }

        // Generate plot
        log_phase(start, "PHASE 5: Starting plot generation");
        println!("\n=== Generating Plot ===");
        use ggrs_core::renderer::{BackendChoice, OutputFormat};
        use ggrs_core::{EnginePlotSpec, Geom, PlotGenerator, PlotRenderer, Theme};

        println!("Creating plot specification...");
        println!("  Point size: {}", config.point_size);
        println!("  Legend position: {:?}", config.to_legend_position());
        println!("  Legend justification: {:?}", config.legend_justification);

        // Create theme with configured legend and title settings
        use ggrs_core::theme::elements::Element;

        let mut theme = Theme {
            legend_position: config.to_legend_position(),
            legend_justification: config.legend_justification,
            plot_title_position: config.plot_title_position.clone(),
            ..Default::default()
        };

        // Apply plot title justification if configured
        if let Some((just_x, just_y)) = config.plot_title_justification {
            if let Element::Text(ref mut text_elem) = theme.plot_title {
                text_elem.hjust = just_x;
                text_elem.vjust = just_y;
            }
        }

        // Create plot spec
        let mut plot_spec = EnginePlotSpec::new()
            .add_layer(Geom::point_sized(config.point_size as f64))
            .theme(theme);

        // Add text labels from configuration
        if let Some(ref title) = config.plot_title {
            plot_spec = plot_spec.title(title.clone());
        }
        if let Some(ref x_label) = config.x_axis_label {
            plot_spec = plot_spec.x_label(x_label.clone());
        }
        if let Some(ref y_label) = config.y_axis_label {
            plot_spec = plot_spec.y_label(y_label.clone());
        }

        log_phase(start, "PHASE 5.1: Creating PlotGenerator");
        println!("Creating plot generator...");
        let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;

        // Resolve plot dimensions
        let (plot_width, plot_height) = config.resolve_dimensions(
            plot_gen.generator().n_col_facets(),
            plot_gen.generator().n_row_facets(),
        );
        println!(
            "  Resolved plot size: {}×{} pixels",
            plot_width, plot_height
        );

        log_phase(
            start,
            "PHASE 5.2: Creating PlotRenderer (optimized streaming)",
        );
        println!("Creating plot renderer...");

        // Create PlotRenderer with cache (if enabled)
        let mut renderer = if let Some(ref cache_ref) = cache {
            PlotRenderer::new_with_cache(
                &plot_gen,
                plot_width as u32,
                plot_height as u32,
                cache_ref.clone(),
            )
        } else {
            PlotRenderer::new(&plot_gen, plot_width as u32, plot_height as u32)
        };

        // Set PNG compression level
        let png_compression = match config.png_compression.to_lowercase().as_str() {
            "fast" => ggrs_core::PngCompression::Fast,
            "best" => ggrs_core::PngCompression::Best,
            _ => ggrs_core::PngCompression::Default,
        };
        renderer.set_png_compression(png_compression);

        log_phase(start, "PHASE 5.3: Rendering plot (optimized streaming)");
        println!(
            "Rendering plot with optimized streaming (PNG compression: {})...",
            config.png_compression
        );

        // Use page-specific filename if we have multiple pages
        let plot_filename = if page_values.len() > 1 {
            format!("plot_{}.png", page_idx + 1)
        } else {
            "plot.png".to_string()
        };

        renderer.render_to_file(&plot_filename, BackendChoice::Cairo, OutputFormat::Png)?;

        log_phase(start, "PHASE 5.4: Checking PNG");
        let metadata = std::fs::metadata(&plot_filename)?;
        println!(
            "✓ Plot saved to {} ({} bytes)",
            plot_filename,
            metadata.len()
        );
    }

    // Clean up cache directory after all pages are rendered
    if let Some(ref cache_ref) = cache {
        println!("  Cleaning up disk cache...");
        cache_ref.clear()?;
    }

    log_phase(start, "PHASE 6: Test complete");
    println!("\n=== Test Complete ===");
    println!("All checks passed! The TercenStreamGenerator is working correctly.");

    Ok(())
}

/// Load test configuration from operator_config.json if it exists
fn load_test_config() -> OperatorConfig {
    use ggrs_plot_operator::tercen::client::proto::{OperatorRef, OperatorSettings, PropertyValue};
    use std::fs;

    let config_path = "operator_config.json";
    let config_json = match fs::read_to_string(config_path) {
        Ok(json) => json,
        Err(_) => {
            println!("  No operator_config.json found, using defaults");
            return OperatorConfig::from_properties(None);
        }
    };

    let config_map: serde_json::Map<String, serde_json::Value> =
        match serde_json::from_str(&config_json) {
            Ok(map) => map,
            Err(e) => {
                eprintln!("  Failed to parse operator_config.json: {}", e);
                eprintln!("  Using defaults");
                return OperatorConfig::from_properties(None);
            }
        };

    // Convert JSON map to PropertyValue list
    let mut property_values = Vec::new();
    for (key, value) in config_map {
        let value_str = match value {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => continue,
        };

        property_values.push(PropertyValue {
            name: key,
            value: value_str,
        });
    }

    let operator_settings = OperatorSettings {
        operator_ref: Some(OperatorRef {
            operator_id: "test".to_string(),
            name: String::new(),
            operator_kind: String::new(),
            operator_spec: None,
            version: String::new(),
            url: None,
            property_values,
        }),
        namespace: String::new(),
        environment: Vec::new(),
        operator_model: None,
    };

    println!("  Loaded configuration from operator_config.json");
    OperatorConfig::from_properties(Some(&operator_settings))
}
