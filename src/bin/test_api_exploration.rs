use ggrs_plot_operator::tercen::TercenClient;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Tercen API Exploration ===\n");

    // Connect to Tercen
    let client = TercenClient::from_env().await?;
    println!("✓ Connected to Tercen\n");

    // Get the workflow and step IDs from environment
    let workflow_id = std::env::var("WORKFLOW_ID").expect("WORKFLOW_ID must be set");
    let step_id = std::env::var("STEP_ID").expect("STEP_ID must be set");

    println!("Workflow ID: {}", workflow_id);
    println!("Step ID: {}\n", step_id);

    // 1. Get the task for this step
    println!("=== 1. Exploring Task Structure ===");
    let mut task_service = client.task_service()?;

    // The taskId from workflow.json is the computation task ID
    let task_id = "28e3c9888e9935f667aed6f07c029cb5";
    println!("Getting task: {}", task_id);

    let request = ggrs_plot_operator::tercen::client::proto::GetRequest {
        id: task_id.to_string(),
        ..Default::default()
    };
    let task_result = task_service.get(request).await;
    match task_result {
        Ok(task) => {
            println!("✓ Got task successfully");
            println!("Task kind: {:?}", task.object);

            if let Some(obj) = &task.object {
                use ggrs_plot_operator::tercen::client::proto::e_task::Object;
                match obj {
                    Object::Computationtask(ct) => {
                        println!("\n  ComputationTask details:");
                        println!("    Query table: {:?}", ct.query);
                        println!("    Relation: {:?}", ct.relation);

                        // Check if there's cube query information
                        if let Some(query) = &ct.query {
                            println!("\n  === Exploring CubeQuery ===");
                            println!("    CubeQuery ID: {}", query.id);

                            // Try to get the cube query table schema
                            use ggrs_plot_operator::tercen::table::TableStreamer;
                            let table_service = TableStreamer::new(&client);
                            match table_service.get_schema(&query.id).await {
                                Ok(schema) => {
                                    println!("    ✓ Got CubeQuery schema");
                                    explore_schema("CubeQuery", &schema);
                                }
                                Err(e) => println!("    ✗ Failed to get CubeQuery schema: {}", e),
                            }
                        }

                        // Check relation
                        if let Some(relation) = &ct.relation {
                            println!("\n  === Exploring Relation ===");
                            println!("    Relation ID: {}", relation.id);

                            let table_service = TableStreamer::new(&client);
                            match table_service.get_schema(&relation.id).await {
                                Ok(schema) => {
                                    println!("    ✓ Got Relation schema");
                                    explore_schema("Relation", &schema);
                                }
                                Err(e) => println!("    ✗ Failed to get Relation schema: {}", e),
                            }
                        }
                    }
                    _ => println!("  Task is not a ComputationTask"),
                }
            }
        }
        Err(e) => println!("✗ Failed to get task: {}", e),
    }

    // 2. Explore the workflow document itself
    println!("\n=== 2. Exploring Workflow Document ===");
    let mut document_service = client.document_service()?;

    let doc_request = ggrs_plot_operator::tercen::client::proto::GetRequest {
        id: workflow_id.clone(),
        ..Default::default()
    };
    match document_service.get(doc_request).await {
        Ok(workflow_doc) => {
            println!("✓ Got workflow document");

            if let Some(obj) = &workflow_doc.object {
                use ggrs_plot_operator::tercen::client::proto::e_document::Object;
                match obj {
                    Object::Workflow(wf) => {
                        println!("\n  Workflow details:");
                        println!("    Name: {}", wf.name);
                        println!("    Number of steps: {}", wf.steps.len());

                        // Find our data step
                        for step in &wf.steps {
                            use ggrs_plot_operator::tercen::client::proto::e_step::Object;
                            if let Some(step_obj) = &step.object {
                                if let Object::Datastep(ds) = step_obj {
                                    if ds.id == step_id {
                                        println!("\n  === Found our DataStep ===");
                                        println!("    Step ID: {}", ds.id);
                                        println!("    Step name: {}", ds.name);

                                        // Explore the crosstab model
                                        if let Some(model_obj) = &ds.model {
                                            use ggrs_plot_operator::tercen::client::proto::e_step_model::Object;
                                            if let Object::Crosstab(ct) = &model_obj.object {
                                                println!("\n  === Crosstab Model ===");
                                                println!("    Task ID: {}", ct.task_id);

                                                // Column table
                                                if let Some(col_table) = &ct.column_table {
                                                    println!("\n    Column Table:");
                                                    println!("      nRows: {}", col_table.n_rows);
                                                    println!(
                                                        "      cellSize: {}",
                                                        col_table.cell_size
                                                    );
                                                    println!("      offset: {}", col_table.offset);
                                                    println!(
                                                        "      Graphical factors: {}",
                                                        col_table.graphical_factors.len()
                                                    );
                                                    for (i, gf) in col_table
                                                        .graphical_factors
                                                        .iter()
                                                        .enumerate()
                                                    {
                                                        if let Some(factor) = &gf.factor {
                                                            println!(
                                                                "        [{}] {} ({})",
                                                                i, factor.name, factor.r#type
                                                            );
                                                        }
                                                    }
                                                }

                                                // Row table
                                                if let Some(row_table) = &ct.row_table {
                                                    println!("\n    Row Table:");
                                                    println!("      nRows: {}", row_table.n_rows);
                                                    println!(
                                                        "      cellSize: {}",
                                                        row_table.cell_size
                                                    );
                                                    println!("      offset: {}", row_table.offset);
                                                    println!(
                                                        "      Graphical factors: {}",
                                                        row_table.graphical_factors.len()
                                                    );
                                                    for (i, gf) in row_table
                                                        .graphical_factors
                                                        .iter()
                                                        .enumerate()
                                                    {
                                                        if let Some(factor) = &gf.factor {
                                                            println!(
                                                                "        [{}] {} ({})",
                                                                i, factor.name, factor.r#type
                                                            );
                                                        }
                                                    }
                                                }

                                                // Axis list
                                                if let Some(axis) = &ct.axis {
                                                    println!("\n    Axis List:");
                                                    println!(
                                                        "      Number of XY axes: {}",
                                                        axis.xy_axis.len()
                                                    );
                                                    for (i, xy) in axis.xy_axis.iter().enumerate() {
                                                        println!(
                                                            "      [{}] Task ID: {}",
                                                            i, xy.task_id
                                                        );

                                                        // Check X axis
                                                        if let Some(x_axis) = &xy.x_axis {
                                                            if let Some(gf) =
                                                                &x_axis.graphical_factor
                                                            {
                                                                if let Some(factor) = &gf.factor {
                                                                    println!(
                                                                        "          X: {} ({})",
                                                                        factor.name, factor.r#type
                                                                    );
                                                                }
                                                            }
                                                        }

                                                        // Check Y axis
                                                        if let Some(y_axis) = &xy.y_axis {
                                                            if let Some(gf) =
                                                                &y_axis.graphical_factor
                                                            {
                                                                if let Some(factor) = &gf.factor {
                                                                    println!(
                                                                        "          Y: {} ({})",
                                                                        factor.name, factor.r#type
                                                                    );
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // Check computed relation
                                        if let Some(computed_rel) = &ds.computed_relation {
                                            println!("\n  === Computed Relation ===");
                                            println!("    Relation ID: {}", computed_rel.id);

                                            // Try to get its schema
                                            let table_service = TableStreamer::new(&client);
                                            match table_service.get_schema(&computed_rel.id).await {
                                                Ok(schema) => {
                                                    println!("    ✓ Got computed relation schema");
                                                    explore_schema("ComputedRelation", &schema);
                                                }
                                                Err(e) => println!("    ✗ Failed to get computed relation schema: {}", e),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => println!("  Document is not a Workflow"),
                }
            }
        }
        Err(e) => println!("✗ Failed to get workflow document: {}", e),
    }

    // 3. Try to explore the task's generated tables
    println!("\n=== 3. Exploring Generated Tables ===");

    // These are common table suffixes that Tercen generates
    let table_suffixes = vec![
        "",         // Main table
        "_column",  // Column facet table
        "_row",     // Row facet table
        "_x",       // X axis table
        "_y",       // Y axis table
        "_stats",   // Statistics table (if exists)
        "_summary", // Summary table (if exists)
    ];

    use ggrs_plot_operator::tercen::table::TableStreamer;
    let table_service = TableStreamer::new(&client);

    for suffix in table_suffixes {
        let table_id = format!("{}{}", task_id, suffix);
        println!("\nTrying table: {}", table_id);

        match table_service.get_schema(&table_id).await {
            Ok(schema) => {
                println!("  ✓ Found table!");
                explore_schema(&format!("Table{}", suffix), &schema);

                // Try to get first few rows with Select
                println!("\n  Sampling data with Select...");
                let select = ggrs_plot_operator::tercen::client::proto::Select {
                    offset: Some(0),
                    limit: Some(5),
                    columns: None,
                    ..Default::default()
                };

                match table_service.select(&table_id, &select).await {
                    Ok(data) => {
                        println!("    ✓ Got {} bytes of data", data.len());

                        // Try to parse as TSON
                        use ggrs_plot_operator::tercen::tson_convert::tson_to_dataframe;
                        match tson_to_dataframe(&data) {
                            Ok(df) => {
                                println!("    ✓ Parsed as TSON DataFrame");
                                println!("      Rows: {}", df.nrow());
                                println!("      Columns: {}", df.ncol());
                                println!("      Column names: {:?}", df.get_column_names());
                            }
                            Err(e) => println!("    ✗ Failed to parse TSON: {}", e),
                        }
                    }
                    Err(e) => println!("    ✗ Failed to select data: {}", e),
                }
            }
            Err(_) => {
                // Table doesn't exist, that's ok
            }
        }
    }

    println!("\n=== Exploration Complete ===");
    Ok(())
}

fn explore_schema(name: &str, schema: &ggrs_plot_operator::tercen::client::proto::ESchema) {
    use ggrs_plot_operator::tercen::client::proto::e_schema::Object;

    println!("    {} Schema:", name);

    match &schema.object {
        Some(Object::Tableschema(ts)) => {
            println!("      Type: TableSchema");
            println!("      Columns: {}", ts.columns.len());
            println!("      nRows: {}", ts.n_rows);

            println!("      Column details:");
            for col in &ts.columns {
                println!("        - {} (type: {})", col.name, col.col_type);
            }
        }
        Some(Object::Computedtableschema(cts)) => {
            println!("      Type: ComputedTableSchema");
            println!("      Columns: {}", cts.columns.len());
            println!("      nRows: {}", cts.n_rows);

            println!("      Column details:");
            for col in &cts.columns {
                println!("        - {} (type: {})", col.name, col.col_type);
            }
        }
        Some(Object::Cubequerytableschema(cqts)) => {
            println!("      Type: CubeQueryTableSchema");
            println!("      Columns: {}", cqts.columns.len());
            println!("      nRows: {}", cqts.n_rows);

            println!("      Column details:");
            for col in &cqts.columns {
                println!("        - {} (type: {})", col.name, col.col_type);
            }
        }
        _ => println!("      Type: Unknown"),
    }
}
