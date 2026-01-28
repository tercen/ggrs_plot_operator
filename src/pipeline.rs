//! Shared plot generation pipeline
//!
//! This module contains the core plot generation logic that is shared between
//! production (main.rs) and development (dev.rs) entry points.
//!
//! The pipeline:
//! 1. Extracts page information from context
//! 2. Creates StreamGenerator for each page
//! 3. Renders plots using GGRS
//! 4. Returns plot results for output handling

use crate::config::OperatorConfig;
use crate::ggrs_integration::TercenStreamGenerator;
use crate::tercen::{extract_page_values, ChartKind, ColorMapping, PlotResult, TercenContext};
use ggrs_core::scale::ContinuousScale;
use ggrs_core::stream::{DataCache, StreamGenerator};
use ggrs_core::theme::elements::Element;
use ggrs_core::{EnginePlotSpec, Geom, HeatmapLayout, PlotGenerator, PlotRenderer, Theme};

/// Error type for pipeline operations
pub type PipelineError = Box<dyn std::error::Error>;

/// Generate plots from a TercenContext
///
/// This is the main entry point for the shared pipeline. It takes any type
/// implementing TercenContext and generates plots based on the configuration.
///
/// # Arguments
/// * `ctx` - The Tercen context (ProductionContext or DevContext)
/// * `config` - Operator configuration
///
/// # Returns
/// A vector of PlotResult containing the rendered PNG images
pub async fn generate_plots<C: TercenContext>(
    ctx: &C,
    config: &OperatorConfig,
) -> Result<Vec<PlotResult>, PipelineError> {
    // Display context information
    print_context_info(ctx, config);

    // Display color information
    print_color_info(ctx);

    // Extract page information
    println!("\n[2/4] Extracting page information...");
    let page_values = extract_page_values(ctx.client(), ctx.row_hash(), ctx.page_factors()).await?;

    if page_values.is_empty() {
        return Err("No pages to generate".into());
    }

    println!("  Pages to generate: {}", page_values.len());
    for (i, page_value) in page_values.iter().enumerate() {
        println!("    Page {}: {}", i + 1, page_value.label);
    }

    // Create shared disk cache for all pages (only if multiple pages)
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

    // Generate plots for each page
    println!(
        "\n[3/4] Generating plots for {} page(s)...",
        page_values.len()
    );

    let mut plot_results: Vec<PlotResult> = Vec::new();
    let client_arc = ctx.client().clone();

    for (page_idx, page_value) in page_values.iter().enumerate() {
        if page_values.len() > 1 {
            println!(
                "\n=== Page {}/{}: {} ===",
                page_idx + 1,
                page_values.len(),
                page_value.label
            );
        }

        // Create StreamGenerator for this page
        let page_filter = if page_values.len() > 1 {
            Some(&page_value.values)
        } else {
            None
        };

        let mut stream_gen = TercenStreamGenerator::new(
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

        // For heatmaps: enable heatmap mode which sets 1x1 facets and grid-based axis ranges
        // The original facet dimensions become the heatmap grid dimensions
        if matches!(ctx.chart_kind(), ChartKind::Heatmap) {
            let (n_cols, n_rows) = stream_gen.original_grid_dims();
            println!(
                "  Heatmap mode: using grid {}×{} as tile positions",
                n_cols, n_rows
            );
            stream_gen.set_heatmap_mode(n_cols, n_rows);
        }

        println!(
            "  Facets: {} columns × {} rows = {} cells",
            stream_gen.n_col_facets(),
            stream_gen.n_row_facets(),
            stream_gen.n_col_facets() * stream_gen.n_row_facets()
        );

        // Render the plot
        let plot_result = render_page(
            ctx,
            config,
            stream_gen,
            page_value,
            page_idx,
            page_values.len(),
            cache.as_ref(),
        )?;

        plot_results.push(plot_result);
    }

    // Clean up cache
    if let Some(ref cache_ref) = cache {
        println!("  Cleaning up disk cache...");
        cache_ref.clear()?;
    }

    println!("\n[4/4] Plot generation complete");
    Ok(plot_results)
}

/// Render a single page/plot
fn render_page<C: TercenContext>(
    ctx: &C,
    config: &OperatorConfig,
    stream_gen: TercenStreamGenerator,
    page_value: &crate::tercen::PageValue,
    page_idx: usize,
    total_pages: usize,
    cache: Option<&DataCache>,
) -> Result<PlotResult, PipelineError> {
    use ggrs_core::renderer::{BackendChoice, OutputFormat};

    // Resolve plot dimensions
    let (plot_width, plot_height) =
        config.resolve_dimensions(stream_gen.n_col_facets(), stream_gen.n_row_facets());
    println!(
        "  Resolved plot size: {}×{} pixels",
        plot_width, plot_height
    );

    // Create theme
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

    // Apply tick label rotation if configured
    if config.x_tick_rotation != 0.0 {
        theme.set_x_tick_rotation(config.x_tick_rotation);
        println!("  X-axis tick rotation: {}°", config.x_tick_rotation);
    }
    if config.y_tick_rotation != 0.0 {
        theme.set_y_tick_rotation(config.y_tick_rotation);
        println!("  Y-axis tick rotation: {}°", config.y_tick_rotation);
    }

    // Select geom based on chart kind
    let geom = match ctx.chart_kind() {
        ChartKind::Heatmap => {
            println!("  Chart kind: Heatmap (using Geom::tile())");
            Geom::tile()
        }
        ChartKind::Point | ChartKind::Line | ChartKind::Bar => {
            println!(
                "  Chart kind: {:?} (using Geom::point_sized({}))",
                ctx.chart_kind(),
                config.point_size
            );
            Geom::point_sized(config.point_size)
        }
    };

    // Get aes, facet_spec, and legend_scale from StreamGenerator
    let aes = stream_gen.aes().clone();
    let legend_scale = stream_gen.query_legend_scale();

    // For heatmaps: no faceting - the grid IS the heatmap
    // .ci = X position, .ri = Y position (following legacy R operator)
    let facet_spec = match ctx.chart_kind() {
        ChartKind::Heatmap => {
            println!("  Heatmap mode: using FacetSpec::none() (grid is the heatmap)");
            ggrs_core::stream::FacetSpec::none()
        }
        _ => stream_gen.facet_spec().clone(),
    };

    // Create PlotSpec with chart-specific layout
    let mut plot_spec = EnginePlotSpec::new()
        .aes(aes)
        .facet(facet_spec)
        .legend_scale(legend_scale)
        .add_layer(geom)
        .theme(theme);

    // Set chart layout based on chart kind
    // HeatmapLayout: uses .ci/.ri for positions, discrete axes, single panel
    // DefaultLayout (default): uses .xs/.ys, continuous axes, faceted panels
    if let ChartKind::Heatmap = ctx.chart_kind() {
        let (n_cols, n_rows) = stream_gen.original_grid_dims();
        plot_spec = plot_spec.chart_layout(Box::new(HeatmapLayout::new(n_cols, n_rows)));

        // For heatmaps, use scales with NO expansion (discrete grid positions)
        // Default ContinuousScale has 5% expansion which distorts tile placement
        let x_scale = ContinuousScale::new().with_expand(0.0, 0.0);
        let y_scale = ContinuousScale::new().with_expand(0.0, 0.0);
        plot_spec = plot_spec
            .scale_x(Box::new(x_scale))
            .scale_y(Box::new(y_scale));

        println!(
            "  Chart layout: HeatmapLayout (grid {}×{}, no expansion)",
            n_cols, n_rows
        );
    }

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

    // Create PlotGenerator
    let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;

    // Create PlotRenderer with cache (if enabled)
    let mut renderer = if let Some(cache_ref) = cache {
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

    // Render to temporary file (must use temp dir in production containers)
    let temp_dir = std::env::temp_dir();
    let temp_path = if total_pages > 1 {
        temp_dir.join(format!("temp_plot_page_{}.png", page_idx))
    } else {
        temp_dir.join("temp_plot.png")
    };
    renderer.render_to_file(&temp_path.to_string_lossy(), backend, OutputFormat::Png)?;

    // Read PNG into memory
    let png_buffer = std::fs::read(&temp_path)?;
    std::fs::remove_file(&temp_path)?;

    println!("✓ Plot generated ({} bytes)", png_buffer.len());

    // Build page factors for result
    let page_factors: Vec<(String, String)> = ctx
        .page_factors()
        .iter()
        .filter_map(|name| {
            page_value
                .values
                .get(name)
                .map(|value| (name.clone(), value.clone()))
        })
        .collect();

    Ok(PlotResult {
        label: page_value.label.clone(),
        png_buffer,
        width: plot_width,
        height: plot_height,
        page_factors,
    })
}

/// Print context information
fn print_context_info<C: TercenContext>(ctx: &C, config: &OperatorConfig) {
    println!("\n[1/4] Context information...");
    println!("  Main table: {}", ctx.qt_hash());
    println!("  Column facets: {}", ctx.column_hash());
    println!("  Row facets: {}", ctx.row_hash());
    println!("  Workflow: {}", ctx.workflow_id());
    println!("  Step: {}", ctx.step_id());

    println!("\n  Configuration:");
    println!("    Backend: {}", config.backend);
    println!("    Point size: {}", config.point_size);
    println!(
        "    Plot dimensions: {:?} × {:?}",
        config.plot_width, config.plot_height
    );

    if let Some(y_table) = ctx.y_axis_table_id() {
        println!("    Y-axis table: {}", y_table);
    }
}

/// Print color information
fn print_color_info<C: TercenContext>(ctx: &C) {
    if ctx.color_infos().is_empty() {
        println!("  No color factors defined");
    } else {
        for (i, info) in ctx.color_infos().iter().enumerate() {
            println!("  Color {} : '{}'", i + 1, info.factor_name);
            println!("    Type: {}", info.factor_type);
            match &info.mapping {
                ColorMapping::Continuous(palette) => {
                    if let Some((min, max)) = palette.range() {
                        println!("    Range: {} to {}", min, max);
                        println!("    Stops: {}", palette.stops.len());
                    }
                }
                ColorMapping::Categorical(color_map) => {
                    println!("    Categories: {}", color_map.mappings.len());
                }
            }
        }
    }
}
