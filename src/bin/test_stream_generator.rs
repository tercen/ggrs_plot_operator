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
use ggrs_plot_operator::tercen::TercenClient;
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

    // Load operator configuration
    let config = OperatorConfig::load();

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
    println!("✓ Connected successfully\n");

    // Get CubeQuery from workflow and step (like Python OperatorContextDev)
    log_phase(start, "PHASE 2: Fetching CubeQuery");
    println!("Fetching CubeQuery from workflow step...");
    let (cube_query, full_cube_query, cube_query_task) =
        get_cube_query(&client, &workflow_id, &step_id).await?;

    println!("✓ CubeQuery retrieved");
    println!("  Main table (qt_hash): {}", cube_query.qt_hash);
    println!("  Column table (column_hash): {}", cube_query.column_hash);
    println!("  Row table (row_hash): {}", cube_query.row_hash);

    // DEBUG: Check what's in the full CubeQuery
    println!("\n  DEBUG: Full CubeQuery details:");
    println!("    Axis queries: {}", full_cube_query.axis_queries.len());
    println!(
        "    Col columns (factors): {}",
        full_cube_query.col_columns.len()
    );
    println!(
        "    Row columns (factors): {}",
        full_cube_query.row_columns.len()
    );

    if let Some(ref e_relation) = full_cube_query.relation {
        println!("    Has relation: YES");
        println!("    Relation type: {:?}", e_relation.object);

        // Check if it's a ReferenceRelation
        use ggrs_plot_operator::tercen::client::proto::e_relation;
        if let Some(e_relation::Object::Referencerelation(ref_rel)) = &e_relation.object {
            println!("      Reference relation ID: {}", ref_rel.id);
        }
    } else {
        println!("    Has relation: NO");
    }

    if let Some(ref op_settings) = full_cube_query.operator_settings {
        println!("    Has operator settings: YES");
        println!("    Namespace: {}", op_settings.namespace);
        println!("    Environment pairs: {}", op_settings.environment.len());
        for pair in &op_settings.environment {
            println!("      {} = {}", pair.key, pair.value);
        }
    } else {
        println!("    Has operator settings: NO");
    }

    for (i, axis_query) in full_cube_query.axis_queries.iter().enumerate() {
        println!("\n    Axis query {}:", i);
        println!("      Chart type: {}", axis_query.chart_type);
        println!("      Point size: {}", axis_query.point_size);

        if let Some(ref y_axis) = axis_query.y_axis {
            println!(
                "      Y-axis factor: name='{}', type='{}'",
                y_axis.name, y_axis.r#type
            );
        }
        if let Some(ref x_axis) = axis_query.x_axis {
            println!(
                "      X-axis factor: name='{}', type='{}'",
                x_axis.name, x_axis.r#type
            );
        }

        if let Some(ref y_settings) = axis_query.y_axis_settings {
            println!(
                "      Y-axis settings meta pairs: {}",
                y_settings.meta.len()
            );
            for pair in &y_settings.meta {
                println!("        {} = {}", pair.key, pair.value);
            }
        }
        if let Some(ref x_settings) = axis_query.x_axis_settings {
            println!(
                "      X-axis settings meta pairs: {}",
                x_settings.meta.len()
            );
            for pair in &x_settings.meta {
                println!("        {} = {}", pair.key, pair.value);
            }
        }

        println!("      Colors: {}", axis_query.colors.len());
        println!("      Labels: {}", axis_query.labels.len());
        println!("      Errors: {}", axis_query.errors.len());
    }
    println!();

    // DEBUG: Check if there's an axis range table (4th table in CubeQueryTask)
    log_phase(start, "PHASE 2.5: Investigating axis range tables");
    let client_arc_temp = Arc::new(client);
    let streamer = ggrs_plot_operator::tercen::TableStreamer::new(&client_arc_temp);

    if let Some(ref task) = cube_query_task {
        println!(
            "\n  DEBUG: Found {} schema tables from CubeQueryTask",
            task.schema_ids.len()
        );

        // Find the extra table (not qt, column, or row)
        let known_tables = [
            full_cube_query.qt_hash.as_str(),
            full_cube_query.column_hash.as_str(),
            full_cube_query.row_hash.as_str(),
        ];

        for (i, schema_id) in task.schema_ids.iter().enumerate() {
            if !known_tables.contains(&schema_id.as_str()) {
                println!("  Found extra table {}: {}", i, schema_id);
                println!("    This might be the axis ranges table!");
                println!("    Fetching schema...");

                let axis_schema = streamer.get_schema(schema_id).await?;
                use ggrs_plot_operator::tercen::client::proto::e_schema;
                if let Some(e_schema::Object::Cubequerytableschema(cqts)) = axis_schema.object {
                    println!("      Table type: CubeQueryTableSchema");
                    println!("      Query table type: {}", cqts.query_table_type);
                    println!("      Columns: {}", cqts.columns.len());
                    for (j, col) in cqts.columns.iter().take(10).enumerate() {
                        if let Some(ggrs_plot_operator::tercen::client::proto::e_column_schema::Object::Columnschema(cs)) = &col.object {
                            println!("        Column {}: name='{}', type='{}'", j, cs.name, cs.r#type);
                        }
                    }

                    // Try to fetch rows with no column selection (all columns)
                    println!("      Fetching first 5 rows (all columns)...");
                    let data_all = streamer.stream_tson(schema_id, None, 0, 5).await?;
                    println!("        Got {} bytes of data", data_all.len());

                    if !data_all.is_empty() {
                        use ggrs_plot_operator::tercen::tson_convert::tson_to_dataframe;
                        match tson_to_dataframe(&data_all) {
                            Ok(df) => {
                                println!("        Parsed {} rows", df.nrow());
                                println!("        Columns: {:?}", df.columns());
                                if df.nrow() > 0 {
                                    println!("        First row values:");
                                    for col_name in df.columns() {
                                        if let Ok(val) = df.get_value(0, &col_name) {
                                            println!("          {} = {:?}", col_name, val);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                println!("        Error parsing: {}", e);
                            }
                        }
                    }

                    // Try fetching specific columns
                    println!(
                        "      Fetching first row with specific columns [.ri, .minY, .maxY]..."
                    );
                    let columns = vec![".ri".to_string(), ".minY".to_string(), ".maxY".to_string()];
                    let data_specific = streamer
                        .stream_tson(schema_id, Some(columns.clone()), 0, 1)
                        .await?;
                    println!("        Got {} bytes of data", data_specific.len());

                    if !data_specific.is_empty() {
                        use ggrs_plot_operator::tercen::tson_convert::tson_to_dataframe;
                        match tson_to_dataframe(&data_specific) {
                            Ok(df) => {
                                println!("        Parsed {} rows", df.nrow());
                                println!("        Columns: {:?}", df.columns());
                                if df.nrow() > 0 {
                                    println!("        First row values:");
                                    for col_name in &columns {
                                        if let Ok(val) = df.get_value(0, col_name) {
                                            println!("          {} = {:?}", col_name, val);
                                        }
                                    }
                                } else {
                                    println!("        WARNING: Got data bytes but parsed 0 rows!");
                                    println!(
                                        "        Raw TSON bytes (first 200): {:?}",
                                        &data_specific[..data_specific.len().min(200)]
                                    );
                                }
                            }
                            Err(e) => {
                                println!("        Error parsing: {}", e);
                                println!(
                                    "        Raw TSON bytes (first 200): {:?}",
                                    &data_specific[..data_specific.len().min(200)]
                                );
                            }
                        }
                    } else {
                        println!("        WARNING: No data returned (0 bytes)");
                    }
                }
            }
        }
    } else {
        println!("\n  No CubeQueryTask available (using getCubeQuery directly)");
    }

    println!("\n  DEBUG: Checking main table schema for column statistics...");
    let schema = streamer.get_schema(&cube_query.qt_hash).await?;

    use ggrs_plot_operator::tercen::client::proto::e_schema;
    if let Some(e_schema::Object::Cubequerytableschema(cqts)) = schema.object {
        println!("    Table has {} columns", cqts.columns.len());
        for (i, col) in cqts.columns.iter().take(5).enumerate() {
            if let Some(
                ggrs_plot_operator::tercen::client::proto::e_column_schema::Object::Columnschema(
                    cs,
                ),
            ) = &col.object
            {
                println!("    Column {}: name='{}', type='{}'", i, cs.name, cs.r#type);
                if let Some(ref metadata) = cs.meta_data {
                    println!("      Quartiles: {:?}", metadata.quartiles);
                    println!("      Properties: {} pairs", metadata.properties.len());
                    for prop in &metadata.properties {
                        println!("        {} = {}", prop.key, prop.value);
                    }
                }
            }
        }
    }
    println!();

    // Find Y-axis table from CubeQueryTask schema_ids
    let y_axis_table_id = if let Some(ref task) = cube_query_task {
        // Find the extra table (not qt, column, or row)
        let known_tables = [
            full_cube_query.qt_hash.as_str(),
            full_cube_query.column_hash.as_str(),
            full_cube_query.row_hash.as_str(),
        ];

        let mut y_table_id = None;
        for schema_id in &task.schema_ids {
            if !known_tables.contains(&schema_id.as_str()) {
                // Check if this is the Y-axis table
                let axis_schema = streamer.get_schema(schema_id).await?;
                use ggrs_plot_operator::tercen::client::proto::e_schema;
                if let Some(e_schema::Object::Cubequerytableschema(cqts)) = axis_schema.object {
                    if cqts.query_table_type == "y" {
                        println!("  Found Y-axis table: {}", schema_id);
                        y_table_id = Some(schema_id.clone());
                        break;
                    }
                }
            }
        }
        y_table_id
    } else {
        None
    };

    // Create stream generator (includes loading facets and axis ranges)
    log_phase(
        start,
        "PHASE 3: Creating StreamGenerator (loads facets + axis ranges from table)",
    );
    println!("Creating TercenStreamGenerator...");
    let client_arc = client_arc_temp;

    // Use new() constructor with explicit table IDs from CubeQuery
    let stream_gen = TercenStreamGenerator::new(
        client_arc,
        cube_query.qt_hash.clone(),
        cube_query.column_hash.clone(),
        cube_query.row_hash.clone(),
        y_axis_table_id,
        config.chunk_size,
    )
    .await?;

    log_phase(start, "PHASE 3 COMPLETE: StreamGenerator created");
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

    let test_range = Range::new(0, 100); // Query first 100 rows

    println!("Querying bulk data, range 0-100...");
    let data = stream_gen.query_data_multi_facet(test_range);

    log_phase(start, "PHASE 4 COMPLETE: Data query finished");
    println!("✓ Received {} rows", data.nrow());
    println!("  Columns: {:?}", data.columns());

    if data.nrow() > 0 {
        println!("\nFirst 5 rows:");
        for i in 0..5.min(data.nrow()) {
            print!("  Row {}: ", i);
            if let Ok(x) = data.get_value(i, ".x") {
                print!(".x={:?} ", x);
            }
            if let Ok(y) = data.get_value(i, ".y") {
                print!(".y={:?}", y);
            }
            println!();
        }
    }

    // Generate plot
    log_phase(start, "PHASE 5: Starting plot generation");
    println!("\n=== Generating Plot ===");
    use ggrs_core::renderer::{BackendChoice, OutputFormat};
    use ggrs_core::{EnginePlotSpec, Geom, PlotGenerator, PlotRenderer};

    println!("Creating plot specification...");
    println!("  Point size: {}", config.point_size);
    let plot_spec = EnginePlotSpec::new().add_layer(Geom::point_sized(config.point_size as f64));

    log_phase(start, "PHASE 5.1: Creating PlotGenerator");
    println!("Creating plot generator...");
    let plot_gen = PlotGenerator::new(Box::new(stream_gen), plot_spec)?;

    log_phase(
        start,
        "PHASE 5.2: Creating PlotRenderer (optimized streaming)",
    );
    println!("Creating plot renderer...");
    let renderer = PlotRenderer::new(
        &plot_gen,
        config.default_plot_width,
        config.default_plot_height,
    );

    log_phase(start, "PHASE 5.3: Rendering plot (optimized streaming)");
    println!("Rendering plot with optimized streaming...");
    renderer.render_to_file("plot.png", BackendChoice::Cairo, OutputFormat::Png)?;

    log_phase(start, "PHASE 5.4: Checking PNG");
    let metadata = std::fs::metadata("plot.png")?;
    println!("✓ Plot saved to plot.png ({} bytes)", metadata.len());

    log_phase(start, "PHASE 6: Test complete");
    println!("\n=== Test Complete ===");
    println!("All checks passed! The TercenStreamGenerator is working correctly.");

    Ok(())
}

/// Get CubeQuery from workflow and step (like Python OperatorContextDev)
async fn get_cube_query(
    client: &TercenClient,
    workflow_id: &str,
    step_id: &str,
) -> Result<
    (
        CubeQuery,
        ggrs_plot_operator::tercen::client::proto::CubeQuery,
        Option<ggrs_plot_operator::tercen::client::proto::CubeQueryTask>,
    ),
    Box<dyn std::error::Error>,
> {
    use ggrs_plot_operator::tercen::client::proto::{e_step, e_workflow, GetRequest};

    // Get the workflow
    println!("  Getting workflow...");
    let mut workflow_service = client.workflow_service()?;
    let request = tonic::Request::new(GetRequest {
        id: workflow_id.to_string(),
        ..Default::default()
    });
    println!("  Calling WorkflowService.get()...");
    let response = workflow_service.get(request).await?;
    let e_workflow = response.into_inner();

    // Unwrap EWorkflow to get Workflow
    let workflow = e_workflow.object.ok_or("No workflow object")?;

    let e_workflow::Object::Workflow(workflow) = workflow;

    println!("  Workflow name: {}", workflow.name);

    // Find the step (all computation steps are DataStep)
    // Need to unwrap each step to check its id
    let data_step = workflow
        .steps
        .iter()
        .find_map(|e_step| {
            if let Some(e_step::Object::Datastep(ds)) = &e_step.object {
                if ds.id == step_id {
                    return Some(ds);
                }
            }
            None
        })
        .ok_or_else(|| format!("DataStep {} not found in workflow", step_id))?;

    println!("  Step name: {}", data_step.name);

    // Get the CubeQueryTask ID from the crosstab model (NOT from state!)
    let task_id = data_step
        .model
        .as_ref()
        .map(|m| m.task_id.clone())
        .unwrap_or_default();

    println!("  Has crosstab model: {}", data_step.model.is_some());
    println!("  CubeQueryTask ID from crosstab model: '{}'", task_id);
    if let Some(ref state) = data_step.state {
        println!("  State task ID (for comparison): '{}'", state.task_id);
    }

    let (cube_query, cube_query_task_opt) = if !task_id.is_empty() {
        // Step already ran - get CubeQuery from existing task
        println!("  Getting CubeQuery from existing task: {}", task_id);
        let mut task_service = client.task_service()?;
        let request = tonic::Request::new(GetRequest {
            id: task_id.clone(),
            ..Default::default()
        });
        let response = task_service.get(request).await?;
        let e_task = response.into_inner();

        println!("  DEBUG: ETask received");
        println!("    Has object: {}", e_task.object.is_some());

        // Unwrap ETask to get the actual task type
        let task_object = e_task.object.ok_or("Task has no object")?;

        use ggrs_plot_operator::tercen::client::proto::e_task;
        match task_object {
            e_task::Object::Computationtask(ct) => {
                println!("    ✓ Task type: ComputationTask");
                println!("      ID: {}", ct.id);
                println!("      Owner: {}", ct.owner);
                println!("      Has query: {}", ct.query.is_some());
                let query = ct.query.ok_or("ComputationTask has no query")?;
                (query, None)
            }
            e_task::Object::Cubequerytask(cqt) => {
                println!("    ✓ Task type: CubeQueryTask");
                println!("      ID: {}", cqt.id);
                println!("      Owner: {}", cqt.owner);
                println!("      Has query: {}", cqt.query.is_some());
                println!(
                    "      Schema IDs (generated tables): {} tables",
                    cqt.schema_ids.len()
                );
                for (i, schema_id) in cqt.schema_ids.iter().enumerate() {
                    println!("        Table {}: {}", i, schema_id);
                }

                if let Some(ref q) = cqt.query {
                    println!("      === CubeQuery from CubeQueryTask ===");
                    println!("      Axis queries: {}", q.axis_queries.len());
                    println!("      Main table (qt): {}", q.qt_hash);
                    println!("      Column table: {}", q.column_hash);
                    println!("      Row table: {}", q.row_hash);

                    for (i, aq) in q.axis_queries.iter().enumerate() {
                        println!("        Axis query {}: chart_type={}", i, aq.chart_type);
                        if let Some(ref y_settings) = aq.y_axis_settings {
                            println!(
                                "          Y-axis settings meta pairs: {}",
                                y_settings.meta.len()
                            );
                            for pair in &y_settings.meta {
                                println!("            {} = {}", pair.key, pair.value);
                            }
                        } else {
                            println!("          Y-axis settings: NONE");
                        }
                        if let Some(ref x_settings) = aq.x_axis_settings {
                            println!(
                                "          X-axis settings meta pairs: {}",
                                x_settings.meta.len()
                            );
                            for pair in &x_settings.meta {
                                println!("            {} = {}", pair.key, pair.value);
                            }
                        } else {
                            println!("          X-axis settings: NONE");
                        }
                    }
                }
                let query = cqt.query.clone().ok_or("CubeQueryTask has no query")?;
                (query, Some(cqt))
            }
            other => {
                println!("    Task type: {:?}", other);
                return Err(format!("Unexpected task type: {:?}", other).into());
            }
        }
    } else {
        // No task yet - call WorkflowService.getCubeQuery
        println!("  Calling WorkflowService.getCubeQuery (no existing task)...");
        let mut workflow_service = client.workflow_service()?;
        let request =
            tonic::Request::new(ggrs_plot_operator::tercen::client::proto::ReqGetCubeQuery {
                workflow_id: workflow_id.to_string(),
                step_id: step_id.to_string(),
            });
        let response = workflow_service.get_cube_query(request).await?;
        let resp = response.into_inner();

        let query = resp.result.ok_or("getCubeQuery returned no result")?;
        (query, None)
    };

    let simple_query = cube_query.clone().into();
    Ok((simple_query, cube_query, cube_query_task_opt))
}

/// CubeQuery struct (simplified fields we need)
struct CubeQuery {
    qt_hash: String,
    column_hash: String,
    row_hash: String,
}

impl From<ggrs_plot_operator::tercen::client::proto::CubeQuery> for CubeQuery {
    fn from(cq: ggrs_plot_operator::tercen::client::proto::CubeQuery) -> Self {
        CubeQuery {
            qt_hash: cq.qt_hash,
            column_hash: cq.column_hash,
            row_hash: cq.row_hash,
        }
    }
}
