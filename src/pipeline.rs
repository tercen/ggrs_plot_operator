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
use crate::ggrs_integration::{TercenStreamConfig, TercenStreamGenerator};
use crate::memprof;
use crate::tercen::{
    extract_page_values, new_schema_cache, ChartKind, ColorMapping, PlotResult, TercenContext,
};
use ggrs_core::scale::ContinuousScale;
use ggrs_core::stream::{DataCache, StreamGenerator};
use ggrs_core::theme::elements::Element;
use ggrs_core::{EnginePlotSpec, Geom, HeatmapLayout, PlotGenerator, PlotRenderer};

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
    let m0 = memprof::checkpoint_return("generate_plots START");
    let t0 = std::time::Instant::now();

    // Display context information
    print_context_info(ctx, config);

    // Display color information
    print_color_info(ctx);

    // Extract page information
    println!("\n[2/4] Extracting page information...");
    let page_values = extract_page_values(ctx.client(), ctx.row_hash(), ctx.page_factors()).await?;
    let _m1 = memprof::delta("After extract_page_values", m0);
    let _t1 = memprof::time_delta("After extract_page_values", t0, t0);

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

    // Create shared schema cache for multi-page plots
    // Schemas are reused across pages, reducing network requests
    let schema_cache = if page_values.len() > 1 {
        println!("  Created schema cache for multi-page plot");
        Some(new_schema_cache())
    } else {
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

        let m2 = memprof::checkpoint_return("Before TercenStreamGenerator::new()");
        let t2 = std::time::Instant::now();

        // Build configuration struct for stream generator
        let stream_config = TercenStreamConfig::new(
            ctx.qt_hash().to_string(),
            ctx.column_hash().to_string(),
            ctx.row_hash().to_string(),
            config.chunk_size,
        )
        .y_axis_table(ctx.y_axis_table_id().map(|s| s.to_string()))
        .x_axis_table(ctx.x_axis_table_id().map(|s| s.to_string()))
        .colors(ctx.color_infos().to_vec())
        .per_layer_colors(ctx.per_layer_colors().cloned())
        .page_factors(ctx.page_factors().to_vec())
        .schema_cache(schema_cache.clone())
        .heatmap_cell_aggregation(config.heatmap_cell_aggregation)
        .y_transform(ctx.y_transform().map(|s| s.to_string()))
        .x_transform(ctx.x_transform().map(|s| s.to_string()))
        .n_layers(ctx.n_layers())
        .layer_palette_name(ctx.layer_palette_name().map(|s| s.to_string()))
        .layer_y_factor_names(ctx.layer_y_factor_names().to_vec())
        .chart_kind(ctx.chart_kind());

        let mut stream_gen =
            TercenStreamGenerator::new(client_arc.clone(), stream_config, page_filter).await?;

        let _m3 = memprof::delta("After TercenStreamGenerator::new()", m2);
        let _t3 = memprof::time_delta("After TercenStreamGenerator::new()", t0, t2);

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
    // Priority: 1) crosstab dimensions from Tercen UI, 2) grid-based calculation
    let crosstab_dims = ctx.crosstab_dimensions();
    let (sizing_cols, sizing_rows) = stream_gen.sizing_dims();
    let (plot_width, plot_height) =
        config.resolve_dimensions_with_crosstab(crosstab_dims, sizing_cols, sizing_rows);

    if let Some((ct_w, ct_h)) = crosstab_dims {
        println!(
            "  Plot size: {}×{} pixels (from crosstab {}×{} + legend space)",
            plot_width, plot_height, ct_w, ct_h
        );
    } else {
        println!(
            "  Plot size: {}×{} pixels (from {}×{} grid + legend space)",
            plot_width, plot_height, sizing_cols, sizing_rows
        );
    }

    // Create theme from config (gray, bw, or minimal)
    let mut theme = config.to_theme();

    // Apply config overrides
    theme.legend_position = config.to_legend_position();
    theme.legend_justification = config.legend_justification;
    theme.plot_title_position = config.plot_title_position.clone();

    println!("  Theme: {}", config.theme);

    // Apply plot title justification if configured
    if let Some((just_x, just_y)) = config.plot_title_justification {
        if let Element::Text(ref mut text_elem) = theme.plot_title {
            text_elem.hjust = Some(just_x);
            text_elem.vjust = Some(just_y);
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

    // Grid disable
    if config.grid_major_disable {
        theme.disable_grid_major();
        println!("  Major grid: disabled");
    }
    if config.grid_minor_disable {
        theme.disable_grid_minor();
        println!("  Minor grid: disabled");
    }

    // Font size overrides
    if let Some(size) = config.title_font_size {
        theme.set_plot_title_size(size);
        println!("  Title font size: {}pt", size);
    }
    if let Some(size) = config.axis_label_font_size {
        theme.set_axis_title_size(size);
        println!("  Axis label font size: {}pt", size);
    }
    if let Some(size) = config.tick_label_font_size {
        theme.set_axis_text_size(size);
        println!("  Tick label font size: {}pt", size);
    }

    // Axis line width
    if let Some(width) = config.axis_line_width {
        theme.set_panel_border_linewidth(width);
        println!("  Axis line width: {}pt", width);
    }

    // Select geom based on chart kind
    let geom = match ctx.chart_kind() {
        ChartKind::Heatmap => {
            println!("  Chart kind: Heatmap (using Geom::tile())");
            Geom::tile()
        }
        ChartKind::Bar => {
            println!("  Chart kind: Bar (using Geom::bar())");
            Geom::bar()
        }
        ChartKind::Point => {
            println!(
                "  Chart kind: Point (using Geom::point_sized({}))",
                config.point_size
            );
            Geom::point_sized(config.point_size)
        }
        ChartKind::Line => {
            println!(
                "  Chart kind: Line (using Geom::line_width({}))",
                config.point_size
            );
            Geom::line_width(config.point_size)
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
    } else {
        // Non-heatmap charts: use default ContinuousScale
        // Transform handling will be implemented in GGRS via NumericAxisData.transform
        println!("  Chart layout: Default (ContinuousScale)");
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

    // Set point shapes per layer (cycles through layers based on .axisIndex)
    plot_spec = plot_spec.layer_shapes(config.layer_shapes.clone());

    // Set global opacity for data geoms
    plot_spec = plot_spec.opacity(config.opacity);

    // Create PlotGenerator
    let m4 = memprof::checkpoint_return("Before PlotGenerator::new()");
    let t4 = std::time::Instant::now();
    let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;
    let m5 = memprof::delta("After PlotGenerator::new()", m4);
    let t5 = memprof::time_delta("After PlotGenerator::new()", t4, t4);

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

    let (backend, output_format) = match config.output_format.as_str() {
        "svg" => (BackendChoice::Svg, OutputFormat::Svg),
        "hsvg" => (BackendChoice::HybridSvg, OutputFormat::HybridSvg),
        _ => match config.backend.as_str() {
            "gpu" => (BackendChoice::WebGPU, OutputFormat::Png),
            _ => (BackendChoice::Cairo, OutputFormat::Png),
        },
    };

    let ext = output_format.extension();

    println!(
        "  Rendering plot (backend: {}, format: {})...",
        config.backend, ext
    );

    // Render to temporary file (must use temp dir in production containers)
    let temp_dir = std::env::temp_dir();
    let temp_path = if total_pages > 1 {
        temp_dir.join(format!("temp_plot_page_{}.{}", page_idx, ext))
    } else {
        temp_dir.join(format!("temp_plot.{}", ext))
    };

    let _ = memprof::delta("Before render_to_file()", m5);
    let t6 = std::time::Instant::now();
    renderer.render_to_file(&temp_path.to_string_lossy(), backend, output_format)?;
    let _ = memprof::time_delta("After render_to_file()", t5, t6);

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
        output_ext: ext.to_string(),
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
    // Check for per-layer color configuration first
    if let Some(plc) = ctx.per_layer_colors() {
        use crate::tercen::LayerColorConfig;

        println!("  Per-layer color configuration:");
        println!(
            "    Layers: {}, has_explicit={}, is_mixed={}",
            plc.n_layers,
            plc.has_explicit_colors(),
            plc.is_mixed()
        );

        for (layer_idx, config) in plc.layer_configs.iter().enumerate() {
            match config {
                LayerColorConfig::Continuous {
                    palette,
                    factor_name,
                    ..
                } => {
                    println!(
                        "    Layer {}: continuous factor '{}'",
                        layer_idx, factor_name
                    );
                    if let Some((min, max)) = palette.range() {
                        println!(
                            "      Range: {} to {}, {} stops",
                            min,
                            max,
                            palette.stops.len()
                        );
                    }
                }
                LayerColorConfig::Categorical {
                    color_map,
                    factor_name,
                    ..
                } => {
                    println!(
                        "    Layer {}: categorical factor '{}' ({} categories)",
                        layer_idx,
                        factor_name,
                        color_map.mappings.len()
                    );
                }
                LayerColorConfig::Constant { color } => {
                    println!(
                        "    Layer {}: constant color RGB({},{},{})",
                        layer_idx, color[0], color[1], color[2]
                    );
                }
            }
        }
        return;
    }

    // Fallback to legacy color_infos
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
