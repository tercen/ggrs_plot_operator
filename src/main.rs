//! GGRS Plot Operator - Main entry point
//!
//! This operator receives tabular data from Tercen via gRPC, generates plots using GGRS,
//! and returns PNG images back to Tercen for visualization.
//!
//! Module organization:
//! - `tercen`: Tercen gRPC client library (future tercen-rust crate)
//! - `ggrs_integration`: GGRS-specific integration code
//! - `config`: Operator configuration

pub mod config;
pub mod ggrs_integration;
pub mod tercen;

#[cfg(feature = "jemalloc")]
use tikv_jemallocator::Jemalloc;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() {
    println!("GGRS Plot Operator v{}", env!("CARGO_PKG_VERSION"));
    println!("Ready to generate high-performance plots!\n");
    println!("Build timestamp: 2026-01-26 10:00:00"); // Force cache bust

    // Parse command-line arguments
    // Production: Tercen passes --taskId, --serviceUri, --token
    // Dev: Can pass --workflowId, --stepId (like Python OperatorContextDev)
    let args: Vec<String> = std::env::args().collect();
    parse_args(&args);

    // Print environment info
    print_env_info();

    // Connect to Tercen
    println!("Attempting to connect to Tercen...");
    match tercen::TercenClient::from_env().await {
        Ok(client) => {
            println!("✓ Successfully connected to Tercen!\n");

            // Create Arc for sharing client across async operations
            let client_arc = std::sync::Arc::new(client);

            // Process task if TERCEN_TASK_ID is set
            if let Ok(task_id) = std::env::var("TERCEN_TASK_ID") {
                match process_task(client_arc.clone(), &task_id).await {
                    Ok(()) => {
                        println!("\n✓ Task processed successfully!");
                    }
                    Err(e) => {
                        eprintln!("\n✗ Task processing failed: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                println!("No TERCEN_TASK_ID set, skipping task processing");
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to connect to Tercen: {}", e);
            eprintln!("\nNote: To run the operator, set environment variables:");
            eprintln!("  export TERCEN_URI=https://tercen.com:5400");
            eprintln!("  export TERCEN_TOKEN=your_token_here");
            eprintln!("  export TERCEN_TASK_ID=your_task_id_here");
            std::process::exit(1);
        }
    }

    println!("\nOperator completed!");
}

/// Parse command-line arguments and set environment variables
fn parse_args(args: &[String]) {
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--taskId" if i + 1 < args.len() => {
                std::env::set_var("TERCEN_TASK_ID", &args[i + 1]);
                i += 2;
            }
            "--workflowId" if i + 1 < args.len() => {
                std::env::set_var("WORKFLOW_ID", &args[i + 1]);
                i += 2;
            }
            "--stepId" if i + 1 < args.len() => {
                std::env::set_var("STEP_ID", &args[i + 1]);
                i += 2;
            }
            "--serviceUri" if i + 1 < args.len() => {
                std::env::set_var("TERCEN_URI", &args[i + 1]);
                i += 2;
            }
            "--token" if i + 1 < args.len() => {
                std::env::set_var("TERCEN_TOKEN", &args[i + 1]);
                i += 2;
            }
            _ => i += 1,
        }
    }
}

/// Print environment info for debugging
fn print_env_info() {
    if let Ok(task_id) = std::env::var("TERCEN_TASK_ID") {
        println!("TERCEN_TASK_ID: {}", task_id);
    } else {
        println!("TERCEN_TASK_ID not set");
    }

    if let Ok(uri) = std::env::var("TERCEN_URI") {
        println!("TERCEN_URI: {}", uri);
    } else {
        println!("TERCEN_URI not set");
    }

    if let Ok(token) = std::env::var("TERCEN_TOKEN") {
        println!(
            "TERCEN_TOKEN: {}...{}",
            &token[..8.min(token.len())],
            if token.len() > 8 { "***" } else { "" }
        );
    } else {
        println!("TERCEN_TOKEN not set");
    }
    println!();
}

/// Process a Tercen task: fetch data, generate plot, upload result
async fn process_task(
    client_arc: std::sync::Arc<tercen::TercenClient>,
    task_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use tercen::TercenContext;

    println!("=== Task Processing Started ===");
    println!("Task ID: {}\n", task_id);

    // Step 1: Create ProductionContext (fetches task, extracts all data)
    println!("[1/5] Creating context and extracting data...");
    let ctx = tercen::ProductionContext::from_task_id(client_arc.clone(), task_id).await?;

    println!("✓ Context created");
    println!("  Main table: {}", ctx.qt_hash());
    println!("  Column facets: {}", ctx.column_hash());
    println!("  Row facets: {}", ctx.row_hash());
    println!("  Project ID: {}", ctx.project_id());
    println!("  Namespace: {}", ctx.namespace());

    // Load operator configuration from properties
    let config = config::OperatorConfig::from_properties(ctx.operator_settings());

    println!("\n✓ Configuration loaded");
    println!("  Backend: {}", config.backend);
    println!("  Point size: {}", config.point_size);
    println!(
        "  Plot dimensions: {:?} × {:?}",
        config.plot_width, config.plot_height
    );

    if let Some(y_table) = ctx.y_axis_table_id() {
        println!("  Y-axis table: {}", y_table);
    } else {
        println!("  Y-axis table: None (will compute from data)");
    }

    // Step 2: Display color information
    println!("\n[2/5] Color information...");
    if ctx.color_infos().is_empty() {
        println!("  No color factors defined");
    } else {
        for (i, info) in ctx.color_infos().iter().enumerate() {
            println!("  Color {} : '{}'", i + 1, info.factor_name);
            println!("    Type: {}", info.factor_type);
            match &info.mapping {
                tercen::ColorMapping::Continuous(palette) => {
                    if let Some((min, max)) = palette.range() {
                        println!("    Range: {} to {}", min, max);
                        println!("    Stops: {}", palette.stops.len());
                    }
                }
                tercen::ColorMapping::Categorical(color_map) => {
                    println!("    Categories: {}", color_map.mappings.len());
                }
            }
        }
    }

    // Step 2.5: Page information
    println!("\n[2.5/5] Page information...");
    if ctx.page_factors().is_empty() {
        println!("  No page factors defined - will generate single plot");
    } else {
        println!("  Page factors: {:?}", ctx.page_factors());
    }

    // Extract unique page values from row facet table
    let page_values =
        tercen::extract_page_values(ctx.client(), ctx.row_hash(), ctx.page_factors()).await?;

    println!("  Pages to generate: {}", page_values.len());
    for (i, page_value) in page_values.iter().enumerate() {
        println!("    Page {}: {}", i + 1, page_value.label);
    }

    // Store all generated plot buffers for upload at the end
    let mut plot_buffers: Vec<(String, Vec<u8>, i32, i32)> = Vec::new();

    // Step 3 & 4: Loop through pages and generate plots
    println!(
        "\n[3/5] Generating plots for {} page(s)...",
        page_values.len()
    );

    // Create shared disk cache for all pages (only if multiple pages)
    use ggrs_core::stream::DataCache;
    let cache = if page_values.len() > 1 {
        let cache = DataCache::new(ctx.workflow_id(), ctx.step_id())?;
        println!(
            "  Created disk cache at /tmp/ggrs_cache_{}_{}/",
            ctx.workflow_id(),
            ctx.step_id()
        );
        Some(cache)
    } else {
        println!("  Single page - cache disabled");
        None
    };

    for (page_idx, page_value) in page_values.iter().enumerate() {
        if page_values.len() > 1 {
            println!(
                "\n=== Page {}/{}: {} ===",
                page_idx + 1,
                page_values.len(),
                page_value.label
            );
        }

        use ggrs_core::stream::StreamGenerator;

        // Create StreamGenerator for this page with facet filtering
        println!("  Creating StreamGenerator for page {}...", page_idx + 1);

        let page_filter = if page_values.len() > 1 {
            Some(&page_value.values)
        } else {
            None
        };

        let page_stream_gen = ggrs_integration::TercenStreamGenerator::new(
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

        println!(
            "  Facets: {} columns × {} rows = {} cells",
            page_stream_gen.n_col_facets(),
            page_stream_gen.n_row_facets(),
            page_stream_gen.n_col_facets() * page_stream_gen.n_row_facets()
        );

        // Resolve "auto" plot dimensions now that we know facet counts
        let (plot_width, plot_height) = config.resolve_dimensions(
            page_stream_gen.n_col_facets(),
            page_stream_gen.n_row_facets(),
        );
        println!(
            "  Resolved plot size: {}×{} pixels",
            plot_width, plot_height
        );

        // Step 4: Generate plot
        println!("\n[4/5] Generating plot...");
        use ggrs_core::renderer::{BackendChoice, OutputFormat};
        use ggrs_core::{EnginePlotSpec, Geom, PlotGenerator, PlotRenderer, Theme};
        use std::fs;

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

        // Create PlotSpec
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

        // Create PlotGenerator with the StreamGenerator
        let plot_gen = PlotGenerator::new(Box::new(page_stream_gen), plot_spec)?;

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

        println!(
            "  Rendering plot (backend: {}, PNG compression: {})...",
            config.backend, config.png_compression
        );
        let backend = match config.backend.as_str() {
            "gpu" => BackendChoice::WebGPU,
            _ => BackendChoice::Cairo,
        };

        // Render to temporary file with page-specific name
        let temp_path = if page_values.len() > 1 {
            format!("temp_plot_page_{}.png", page_idx)
        } else {
            "temp_plot.png".to_string()
        };
        renderer.render_to_file(&temp_path, backend, OutputFormat::Png)?;

        // Read PNG into memory
        let png_buffer = fs::read(&temp_path)?;
        fs::remove_file(&temp_path)?;

        println!("✓ Plot generated ({} bytes)", png_buffer.len());

        // Store for later upload
        plot_buffers.push((
            page_value.label.clone(),
            png_buffer,
            plot_width,
            plot_height,
        ));
    }

    // Clean up cache directory after all pages are rendered
    if let Some(ref cache_ref) = cache {
        println!("  Cleaning up disk cache...");
        cache_ref.clear()?;
    }

    // Step 5: Upload results and update task
    // We need to fetch the task again for mutable access
    println!("\n[5/5] Uploading result(s) to Tercen...");

    let mut task_service = client_arc.task_service()?;
    let request = tonic::Request::new(tercen::client::proto::GetRequest {
        id: task_id.to_string(),
        ..Default::default()
    });
    let response = task_service.get(request).await?;
    let mut task = response.into_inner();

    if plot_buffers.len() == 1 {
        // Single plot - use existing save_result
        let (_, png_buffer, plot_width, plot_height) = plot_buffers.into_iter().next().unwrap();
        tercen::result::save_result(
            client_arc.clone(),
            ctx.project_id(),
            ctx.namespace(),
            png_buffer,
            plot_width,
            plot_height,
            &mut task,
        )
        .await?;
        println!("✓ Result uploaded and linked successfully");
    } else {
        // Multiple plots - TODO: need to handle multiple file uploads
        println!("  Multiple plots generated:");
        for (label, png_buffer, width, height) in &plot_buffers {
            println!(
                "    - {}: {} bytes ({}×{})",
                label,
                png_buffer.len(),
                width,
                height
            );
        }
        println!("  WARNING: Multiple plot upload not yet implemented!");
        println!("  Using first plot for now...");

        // For now, just upload the first plot
        let (_, png_buffer, plot_width, plot_height) = plot_buffers.into_iter().next().unwrap();
        tercen::result::save_result(
            client_arc.clone(),
            ctx.project_id(),
            ctx.namespace(),
            png_buffer,
            plot_width,
            plot_height,
            &mut task,
        )
        .await?;
        println!("✓ First plot uploaded (multi-plot support coming soon)");
    }

    println!("\n=== Task Processing Complete ===");
    Ok(())
}
