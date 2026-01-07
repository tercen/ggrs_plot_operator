//! Minimal test to explore CubeQueryTask schemas and find per-facet row counts
//!
//! Usage:
//! ```bash
//! TERCEN_URI="http://127.0.0.1:50051" \
//! TERCEN_TOKEN="your_token" \
//! cargo run --bin explore_schemas
//! ```

use ggrs_plot_operator::tercen::TercenClient;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Schema Exploration for Row Counts ===\n");

    // Connect
    let client = TercenClient::from_env().await?;
    println!("✓ Connected\n");

    // Get the task
    let task_id = "28e3c9888e9935f667aed6f07c029cb5";
    println!("Getting task: {}\n", task_id);

    let mut task_service = client.task_service()?;
    let request = ggrs_plot_operator::tercen::client::proto::GetRequest {
        id: task_id.to_string(),
        ..Default::default()
    };

    let response = task_service.get(request).await?;
    let e_task = response.into_inner();

    use ggrs_plot_operator::tercen::client::proto::e_task;
    let task_object = e_task.object.ok_or("No task object")?;

    match task_object {
        e_task::Object::Cubequerytask(cqt) => {
            println!("✓ CubeQueryTask found\n");

            // Print generated tables
            println!("Generated tables ({}):", cqt.schema_ids.len());
            for (i, id) in cqt.schema_ids.iter().enumerate() {
                println!("  [{}] {}", i, id);
            }
            println!();

            // Explore each table's schema
            use ggrs_plot_operator::tercen::table::TableStreamer;
            let table_service = TableStreamer::new(&client);

            for (i, schema_id) in cqt.schema_ids.iter().enumerate() {
                println!("=== Table {}: {} ===", i, schema_id);
                match table_service.get_schema(schema_id).await {
                    Ok(schema) => {
                        print_schema(&schema);
                    }
                    Err(e) => println!("  Error: {}", e),
                }
                println!();
            }

            // Also check the cube query's table hashes
            if let Some(query) = &cqt.query {
                println!("=== CubeQuery Table Hashes ===");
                println!("Main (qt_hash): {}", query.qt_hash);
                println!("Column (column_hash): {}", query.column_hash);
                println!("Row (row_hash): {}", query.row_hash);
                println!();

                for (name, hash) in &[
                    ("Main", &query.qt_hash),
                    ("Column", &query.column_hash),
                    ("Row", &query.row_hash),
                ] {
                    println!("=== {} Table: {} ===", name, hash);
                    match table_service.get_schema(hash).await {
                        Ok(schema) => {
                            print_schema(&schema);
                        }
                        Err(e) => println!("  Error: {}", e),
                    }
                    println!();
                }
            }
        }
        _ => println!("✗ Not a CubeQueryTask"),
    }

    Ok(())
}

fn print_schema(schema: &ggrs_plot_operator::tercen::client::proto::ESchema) {
    use ggrs_plot_operator::tercen::client::proto::e_schema::Object;

    match &schema.object {
        Some(Object::Tableschema(ts)) => {
            println!("  Type: TableSchema");
            println!("  nRows: {}", ts.n_rows);
            println!("  Columns: {}", ts.columns.len());
            for col in &ts.columns {
                if let Some(ggrs_plot_operator::tercen::client::proto::e_column_schema::Object::Columnschema(cs)) = &col.object {
                    print!("    - {}({})", cs.name, cs.r#type);
                    // Check if this could be a row count column
                    if cs.name.contains("count") || cs.name.contains("n_") || cs.name.contains("size") {
                        print!(" 🔍 POTENTIAL ROW COUNT!");
                    }
                    println!();
                }
            }
        }
        Some(Object::Computedtableschema(cts)) => {
            println!("  Type: ComputedTableSchema");
            println!("  nRows: {}", cts.n_rows);
            println!("  Columns: {}", cts.columns.len());
            for col in &cts.columns {
                if let Some(ggrs_plot_operator::tercen::client::proto::e_column_schema::Object::Columnschema(cs)) = &col.object {
                    print!("    - {}({})", cs.name, cs.r#type);
                    if cs.name.contains("count") || cs.name.contains("n_") || cs.name.contains("size") {
                        print!(" 🔍 POTENTIAL ROW COUNT!");
                    }
                    println!();
                }
            }
        }
        Some(Object::Cubequerytableschema(cqts)) => {
            println!("  Type: CubeQueryTableSchema");
            println!("  nRows: {}", cqts.n_rows);
            println!("  Columns: {}", cqts.columns.len());
            for col in &cqts.columns {
                if let Some(ggrs_plot_operator::tercen::client::proto::e_column_schema::Object::Columnschema(cs)) = &col.object {
                    print!("    - {}({})", cs.name, cs.r#type);
                    if cs.name.contains("count") || cs.name.contains("n_") || cs.name.contains("size") {
                        print!(" 🔍 POTENTIAL ROW COUNT!");
                    }
                    println!();
                }
            }
        }
        _ => println!("  Unknown schema type"),
    }
}
