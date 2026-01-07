//! Test to explore where per-facet row counts are stored in Tercen API
//!
//! This test systematically explores the Tercen API to find efficient ways
//! to get per-facet row counts without streaming through all data.
//!
//! Usage:
//! ```bash
//! TERCEN_URI="http://127.0.0.1:50051" \
//! TERCEN_TOKEN="your_token" \
//! WORKFLOW_ID="2076952ae523bb4d472e283b9e0022a4" \
//! STEP_ID="b9659735-27db-4480-b398-4e391431480f" \
//! cargo run --bin test_facet_counts
//! ```

use ggrs_plot_operator::tercen::TercenClient;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Facet Row Count Exploration ===\n");

    // Connect to Tercen
    let client = TercenClient::from_env().await?;
    println!("✓ Connected to Tercen\n");

    // Get environment variables
    let workflow_id = std::env::var("WORKFLOW_ID").expect("WORKFLOW_ID must be set");
    let step_id = std::env::var("STEP_ID").expect("STEP_ID must be set");

    println!("Workflow ID: {}", workflow_id);
    println!("Step ID: {}\n", step_id);

    // Get the CubeQueryTask
    println!("=== 1. Getting CubeQueryTask ===");
    let task_id = "28e3c9888e9935f667aed6f07c029cb5"; // From workflow.json
    println!("Task ID: {}\n", task_id);

    let mut task_service = client.task_service()?;
    let request = ggrs_plot_operator::tercen::client::proto::GetRequest {
        id: task_id.to_string(),
        ..Default::default()
    };

    let response = task_service.get(request).await?;
    let e_task = response.into_inner();

    // Unwrap the task
    use ggrs_plot_operator::tercen::client::proto::e_task;
    let task_object = e_task.object.ok_or("Task has no object")?;

    match task_object {
        e_task::Object::Cubequerytask(cqt) => {
            println!("✓ Found CubeQueryTask");
            println!("  ID: {}", cqt.id);
            println!("  Owner: {}", cqt.owner);
            println!("  State: {:?}", cqt.state);
            println!();

            // Check the schema_ids - these are the generated tables
            println!("  Generated tables (schema_ids): {}", cqt.schema_ids.len());
            for (i, schema_id) in cqt.schema_ids.iter().enumerate() {
                println!("    [{}] {}", i, schema_id);
            }
            println!();

            // Explore each generated table
            use ggrs_plot_operator::tercen::table::TableStreamer;
            let table_service = TableStreamer::new(&client);

            println!("=== 2. Exploring Generated Table Schemas ===\n");
            for (i, schema_id) in cqt.schema_ids.iter().enumerate() {
                println!("Table {}: {}", i, schema_id);

                match table_service.get_schema(schema_id).await {
                    Ok(schema) => {
                        print_schema(&schema);

                        // Try to get a small sample to see what's in the data
                        println!("  Sampling first 5 rows...");
                        match table_service.stream_tson(schema_id, None, 0, 5).await {
                            Ok(data) => {
                                use ggrs_plot_operator::tercen::tson_convert::tson_to_dataframe;
                                match tson_to_dataframe(&data) {
                                    Ok(df) => {
                                        println!("    ✓ Got {} rows", df.nrow());

                                        // Print first few rows
                                        if df.nrow() > 0 {
                                            println!("    Sample data:");
                                            for row_idx in 0..df.nrow().min(5) {
                                                print!("      Row {}: ", row_idx);
                                                for col_name in df.columns.keys() {
                                                    if let Ok(val) = df.get_value(row_idx, col_name)
                                                    {
                                                        print!("{} = {:?}, ", col_name, val);
                                                    }
                                                }
                                                println!();
                                            }
                                        }
                                    }
                                    Err(e) => println!("    ✗ Failed to parse TSON: {}", e),
                                }
                            }
                            Err(e) => println!("    ✗ Failed to select: {}", e),
                        }
                    }
                    Err(e) => {
                        println!("  ✗ Failed to get schema: {}", e);
                    }
                }
                println!();
            }

            // Explore the CubeQuery itself
            if let Some(query) = &cqt.query {
                println!("=== 3. Exploring CubeQuery ===\n");
                println!("  Main table (qt_hash): {}", query.qt_hash);
                println!("  Column table (column_hash): {}", query.column_hash);
                println!("  Row table (row_hash): {}", query.row_hash);
                println!("  Axis queries: {}", query.axis_queries.len());
                println!();

                // Check axis queries - these might have per-cell information
                println!("  === Axis Queries ===");
                for (i, axis_query) in query.axis_queries.iter().enumerate() {
                    println!("  Axis {}: hash = {}", i, axis_query.hash);
                    println!("    Type: {:?}", axis_query.r#type);

                    // The hash might be a table ID
                    println!("    Trying to get schema for hash {}...", axis_query.hash);
                    match table_service.get_schema(&axis_query.hash).await {
                        Ok(schema) => {
                            println!("      ✓ Found table!");
                            print_schema_brief(&schema);

                            // Get all rows from axis tables (they're small)
                            match table_service
                                .stream_tson(&axis_query.hash, None, 0, 100)
                                .await
                            {
                                Ok(data) => {
                                    use ggrs_plot_operator::tercen::tson_convert::tson_to_dataframe;
                                    match tson_to_dataframe(&data) {
                                        Ok(df) => {
                                            println!("      ✓ Got {} rows", df.nrow());
                                            println!(
                                                "      Columns: {:?}",
                                                df.columns.keys().collect::<Vec<_>>()
                                            );

                                            // Look for row count columns
                                            for col_name in df.columns.keys() {
                                                if col_name.contains("count")
                                                    || col_name.contains("n")
                                                    || col_name.contains("size")
                                                {
                                                    println!(
                                                        "      🔍 POTENTIAL ROW COUNT COLUMN: {}",
                                                        col_name
                                                    );
                                                    if df.nrow() > 0 {
                                                        println!("        Sample values:");
                                                        for row_idx in 0..df.nrow().min(10) {
                                                            if let Ok(val) =
                                                                df.get_value(row_idx, col_name)
                                                            {
                                                                println!(
                                                                    "          Row {}: {:?}",
                                                                    row_idx, val
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => println!("      ✗ Failed to parse: {}", e),
                                    }
                                }
                                Err(e) => println!("      ✗ Failed to select: {}", e),
                            }
                        }
                        Err(e) => {
                            println!("      ✗ Not a table: {}", e);
                        }
                    }
                    println!();
                }

                // Check column and row tables
                println!("  === Column Facet Table ===");
                match table_service.get_schema(&query.column_hash).await {
                    Ok(schema) => {
                        print_schema_brief(&schema);
                        explore_facet_table(&table_service, &query.column_hash, "Column").await?;
                    }
                    Err(e) => println!("    ✗ Failed: {}", e),
                }
                println!();

                println!("  === Row Facet Table ===");
                match table_service.get_schema(&query.row_hash).await {
                    Ok(schema) => {
                        print_schema_brief(&schema);
                        explore_facet_table(&table_service, &query.row_hash, "Row").await?;
                    }
                    Err(e) => println!("    ✗ Failed: {}", e),
                }
            }
        }
        _ => {
            println!("✗ Task is not a CubeQueryTask");
        }
    }

    println!("\n=== Exploration Complete ===");
    Ok(())
}

fn print_schema(schema: &ggrs_plot_operator::tercen::client::proto::ESchema) {
    use ggrs_plot_operator::tercen::client::proto::e_schema::Object;

    match &schema.object {
        Some(Object::Tableschema(ts)) => {
            println!("  Type: TableSchema");
            println!("  nRows: {}", ts.n_rows);
            println!(
                "  Columns ({}): {:?}",
                ts.columns.len(),
                ts.columns
                    .iter()
                    .map(|c| format_column(c))
                    .collect::<Vec<_>>()
            );
        }
        Some(Object::Computedtableschema(cts)) => {
            println!("  Type: ComputedTableSchema");
            println!("  nRows: {}", cts.n_rows);
            println!(
                "  Columns ({}): {:?}",
                cts.columns.len(),
                cts.columns
                    .iter()
                    .map(|c| format_column(c))
                    .collect::<Vec<_>>()
            );
        }
        Some(Object::Cubequerytableschema(cqts)) => {
            println!("  Type: CubeQueryTableSchema");
            println!("  nRows: {}", cqts.n_rows);
            println!(
                "  Columns ({}): {:?}",
                cqts.columns.len(),
                cqts.columns
                    .iter()
                    .map(|c| format_column(c))
                    .collect::<Vec<_>>()
            );
        }
        _ => println!("  Type: Unknown"),
    }
}

fn print_schema_brief(schema: &ggrs_plot_operator::tercen::client::proto::ESchema) {
    use ggrs_plot_operator::tercen::client::proto::e_schema::Object;

    match &schema.object {
        Some(Object::Tableschema(ts)) => {
            println!(
                "    nRows: {}, columns: {:?}",
                ts.n_rows,
                ts.columns
                    .iter()
                    .map(|c| format_column(c))
                    .collect::<Vec<_>>()
            );
        }
        Some(Object::Computedtableschema(cts)) => {
            println!(
                "    nRows: {}, columns: {:?}",
                cts.n_rows,
                cts.columns
                    .iter()
                    .map(|c| format_column(c))
                    .collect::<Vec<_>>()
            );
        }
        Some(Object::Cubequerytableschema(cqts)) => {
            println!(
                "    nRows: {}, columns: {:?}",
                cqts.n_rows,
                cqts.columns
                    .iter()
                    .map(|c| format_column(c))
                    .collect::<Vec<_>>()
            );
        }
        _ => println!("    Unknown schema type"),
    }
}

fn format_column(col: &ggrs_plot_operator::tercen::client::proto::EColumnSchema) -> String {
    use ggrs_plot_operator::tercen::client::proto::e_column_schema::Object;

    match &col.object {
        Some(Object::Columnschema(cs)) => format!("{}({})", cs.name, cs.r#type),
        _ => "?".to_string(),
    }
}

async fn explore_facet_table(
    table_service: &ggrs_plot_operator::tercen::table::TableStreamer<'_>,
    table_id: &str,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    use ggrs_plot_operator::tercen::tson_convert::tson_to_dataframe;

    println!("    Getting all {} facet rows...", name);
    match table_service.stream_tson(table_id, None, 0, 100).await {
        Ok(data) => {
            match tson_to_dataframe(&data) {
                Ok(df) => {
                    println!("      ✓ Got {} facet groups", df.nrow());
                    println!("      Columns: {:?}", df.columns.keys().collect::<Vec<_>>());

                    // Look for row count columns
                    let has_count_col = df.columns.keys().any(|k| {
                        k.contains("count")
                            || k.contains("n")
                            || k.contains("size")
                            || k.contains("rows")
                    });

                    if has_count_col {
                        println!("      🎯 FOUND COUNT COLUMN(S) in {} facet table!", name);
                    }

                    // Print all data (it's small)
                    for row_idx in 0..df.nrow() {
                        print!("      Facet {}: ", row_idx);
                        for col_name in df.columns.keys() {
                            if let Ok(val) = df.get_value(row_idx, col_name) {
                                print!("{} = {:?}, ", col_name, val);
                            }
                        }
                        println!();
                    }
                }
                Err(e) => println!("      ✗ Failed to parse: {}", e),
            }
        }
        Err(e) => println!("      ✗ Failed to select: {}", e),
    }

    Ok(())
}
