//! prepare - Prepares a DataStep for local rendering by creating and running a CubeQueryTask.
//!
//! This binary uses gRPC to:
//! 1. Fetch the workflow and DataStep
//! 2. Build a CubeQuery from the step's Crosstab model
//! 3. Create and run a CubeQueryTask (populates data tables)
//! 4. Set model.taskId on the step so the dev binary can use it
//!
//! Also supports `--delete-project` to clean up via gRPC.
//!
//! Usage:
//! ```bash
//! export TERCEN_URI=http://127.0.0.1:50051
//! export TERCEN_TOKEN=your_token_here
//! cargo run --bin prepare -- --workflow-id WF_ID --step-id STEP_ID
//! cargo run --bin prepare -- --delete-project PROJECT_ID
//! ```

use ggrs_plot_operator::tercen::client::proto;
use ggrs_plot_operator::tercen::TercenClient;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let mut workflow_id = String::new();
    let mut step_id = String::new();
    let mut delete_project_id = String::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--workflow-id" | "-w" if i + 1 < args.len() => {
                workflow_id = args[i + 1].clone();
                i += 2;
            }
            "--step-id" | "-s" if i + 1 < args.len() => {
                step_id = args[i + 1].clone();
                i += 2;
            }
            "--delete-project" if i + 1 < args.len() => {
                delete_project_id = args[i + 1].clone();
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    // Handle --delete-project mode
    if !delete_project_id.is_empty() {
        let client = TercenClient::from_env().await?;
        delete_project(&client, &delete_project_id).await?;
        return Ok(());
    }

    if workflow_id.is_empty() || step_id.is_empty() {
        eprintln!("Usage: prepare --workflow-id WF_ID --step-id STEP_ID");
        eprintln!("       prepare --delete-project PROJECT_ID");
        eprintln!("  Environment: TERCEN_URI, TERCEN_TOKEN");
        std::process::exit(1);
    }

    println!("[prepare] workflow_id={}, step_id={}", workflow_id, step_id);

    // Connect to Tercen
    let client = TercenClient::from_env().await?;
    let client = Arc::new(client);

    // Step 1: Fetch workflow
    println!("[prepare] Fetching workflow...");
    let workflow = fetch_workflow(&client, &workflow_id).await?;
    println!("[prepare] Workflow: {}", workflow.name);

    // Step 2: Find the DataStep and extract Crosstab model
    let data_step = workflow
        .steps
        .iter()
        .find_map(|s| match &s.object {
            Some(proto::e_step::Object::Datastep(ds)) if ds.id == step_id => Some(ds.clone()),
            _ => None,
        })
        .ok_or_else(|| format!("DataStep {} not found in workflow", step_id))?;

    let model = data_step
        .model
        .as_ref()
        .ok_or("DataStep has no Crosstab model")?;

    // Check if model.taskId is already set and valid
    if !model.task_id.is_empty() {
        println!(
            "[prepare] model.taskId already set: {}. Verifying...",
            model.task_id
        );
        match verify_cube_query_task(&client, &model.task_id).await {
            Ok(true) => {
                println!("[prepare] CubeQueryTask is valid and done. Nothing to do.");
                return Ok(());
            }
            _ => {
                println!("[prepare] CubeQueryTask invalid or not done. Creating new one.");
            }
        }
    }

    // Step 3: Find the parent TableStep relation
    let parent_relation = find_parent_relation(&workflow, &data_step)?;
    println!("[prepare] Parent relation found");

    // Step 4: Build CubeQuery from Crosstab model
    let cube_query = build_cube_query(model, parent_relation)?;
    println!(
        "[prepare] CubeQuery built: {} axis queries",
        cube_query.axis_queries.len()
    );

    // Step 5: Create CubeQueryTask
    println!("[prepare] Creating CubeQueryTask...");
    let cqt = create_cube_query_task(&client, &cube_query, &workflow).await?;
    let cqt_id = cqt.id.clone();
    println!("[prepare] CubeQueryTask created: {}", cqt_id);

    // Step 6: Run the task
    println!("[prepare] Running CubeQueryTask...");
    match run_task(&client, &cqt_id).await {
        Ok(()) => println!("[prepare] runTask succeeded"),
        Err(e) => {
            // Some task types auto-run on create; runTask may fail with state transition error
            println!("[prepare] runTask returned error (may auto-run): {}", e);
        }
    }

    // Step 7: Wait for completion
    println!("[prepare] Waiting for completion...");
    let finished = wait_done(&client, &cqt_id).await?;

    // Check state
    let state_kind = extract_task_state(&finished);
    if state_kind != "DoneState" {
        return Err(format!("CubeQueryTask finished with state: {}", state_kind).into());
    }

    let schema_ids = extract_schema_ids(&finished);
    println!(
        "[prepare] CubeQueryTask completed: {} schema_ids",
        schema_ids.len()
    );

    // Step 8: Update model.taskId on the step
    println!("[prepare] Updating model.taskId on step...");
    update_model_task_id(&client, &workflow_id, &step_id, &cqt_id).await?;
    println!("[prepare] Done! model.taskId = {}", cqt_id);

    Ok(())
}

async fn fetch_workflow(
    client: &TercenClient,
    workflow_id: &str,
) -> Result<proto::Workflow, Box<dyn std::error::Error>> {
    let mut svc = client.workflow_service()?;
    let resp = svc
        .get(tonic::Request::new(proto::GetRequest {
            id: workflow_id.to_string(),
            ..Default::default()
        }))
        .await?;
    let e_wf = resp.into_inner();
    match e_wf.object {
        Some(proto::e_workflow::Object::Workflow(wf)) => Ok(wf),
        _ => Err("No workflow object".into()),
    }
}

/// Navigate workflow links to find the parent TableStep's relation for a DataStep
fn find_parent_relation(
    workflow: &proto::Workflow,
    data_step: &proto::DataStep,
) -> Result<proto::ERelation, Box<dyn std::error::Error>> {
    // DataStep has inputs; find the first input port
    let input_port = data_step
        .inputs
        .first()
        .ok_or("DataStep has no input ports")?;

    // Find the Link that connects to this input port
    let link = workflow
        .links
        .iter()
        .find(|l| l.input_id == input_port.id)
        .ok_or_else(|| format!("No link found for input port {}", input_port.id))?;

    // Find the step that has the matching output port
    for step in &workflow.steps {
        if let Some(proto::e_step::Object::Tablestep(ts)) = &step.object {
            if ts.outputs.iter().any(|o| o.id == link.output_id) {
                let model = ts.model.as_ref().ok_or("TableStep has no model")?;
                return model
                    .relation
                    .clone()
                    .ok_or_else(|| "TableStep model has no relation".into());
            }
        }
    }

    Err(format!("No parent step found for output port {}", link.output_id).into())
}

/// Build a CubeQuery from the Crosstab model
fn build_cube_query(
    model: &proto::Crosstab,
    relation: proto::ERelation,
) -> Result<proto::CubeQuery, Box<dyn std::error::Error>> {
    // Extract col/row columns from CrosstabTable graphicalFactors.
    // Filter out empty-name factors — default CrosstabTable has a placeholder
    // graphicalFactor with name="" that must not be included in the CubeQuery.
    let col_columns: Vec<proto::Factor> = model
        .column_table
        .as_ref()
        .map(|ct| {
            ct.graphical_factors
                .iter()
                .filter_map(|gf| gf.factor.clone())
                .filter(|f| !f.name.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let row_columns: Vec<proto::Factor> = model
        .row_table
        .as_ref()
        .map(|rt| {
            rt.graphical_factors
                .iter()
                .filter_map(|gf| gf.factor.clone())
                .filter(|f| !f.name.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Build axis queries from XYAxis list
    let axis_queries: Vec<proto::CubeAxisQuery> = model
        .axis
        .as_ref()
        .map(|axis_list| axis_list.xy_axis.iter().map(build_axis_query).collect())
        .unwrap_or_default();

    let filters = model.filters.clone();

    let operator_settings = model.operator_settings.clone();

    Ok(proto::CubeQuery {
        relation: Some(relation),
        col_columns,
        row_columns,
        axis_queries,
        filters,
        operator_settings,
        // These are OUTPUT fields — populated by CubeQueryTask runner
        qt_hash: String::new(),
        column_hash: String::new(),
        row_hash: String::new(),
    })
}

/// Build a CubeAxisQuery from an XYAxis
fn build_axis_query(xy: &proto::XyAxis) -> proto::CubeAxisQuery {
    // Extract chart type and point size from EChart
    let (chart_type, point_size) = match &xy.chart {
        Some(chart) => extract_chart_info(chart),
        None => ("point".to_string(), 4),
    };

    let y_axis = xy
        .y_axis
        .as_ref()
        .and_then(|a| a.graphical_factor.as_ref())
        .and_then(|gf| gf.factor.clone());

    let y_axis_settings = xy.y_axis.as_ref().and_then(|a| a.axis_settings.clone());

    let x_axis = xy
        .x_axis
        .as_ref()
        .and_then(|a| a.graphical_factor.as_ref())
        .and_then(|gf| gf.factor.clone());

    let x_axis_settings = xy.x_axis.as_ref().and_then(|a| a.axis_settings.clone());

    let colors: Vec<proto::Factor> = xy
        .colors
        .as_ref()
        .map(|c| c.factors.clone())
        .unwrap_or_default();

    let labels: Vec<proto::Factor> = xy
        .labels
        .as_ref()
        .map(|l| l.factors.clone())
        .unwrap_or_default();

    let errors: Vec<proto::Factor> = xy
        .errors
        .as_ref()
        .map(|e| e.factors.clone())
        .unwrap_or_default();

    proto::CubeAxisQuery {
        point_size,
        chart_type,
        y_axis,
        y_axis_settings,
        x_axis,
        x_axis_settings,
        colors,
        labels,
        errors,
        preprocessors: xy.preprocessors.clone(),
    }
}

/// Extract chart type string and point size from EChart
fn extract_chart_info(chart: &proto::EChart) -> (String, i32) {
    match &chart.object {
        Some(proto::e_chart::Object::Chartpoint(cp)) => ("point".to_string(), cp.point_size),
        Some(proto::e_chart::Object::Chartsize(cs)) => ("point".to_string(), cs.point_size),
        Some(proto::e_chart::Object::Chartline(cl)) => ("line".to_string(), cl.point_size),
        Some(proto::e_chart::Object::Chartbar(_)) => ("bar".to_string(), 4),
        Some(proto::e_chart::Object::Chartheatmap(_)) => ("heatmap".to_string(), 4),
        Some(proto::e_chart::Object::Chart(_)) => ("point".to_string(), 4),
        None => ("point".to_string(), 4),
    }
}

/// Create a CubeQueryTask via gRPC
async fn create_cube_query_task(
    client: &TercenClient,
    query: &proto::CubeQuery,
    workflow: &proto::Workflow,
) -> Result<proto::CubeQueryTask, Box<dyn std::error::Error>> {
    let cqt = proto::CubeQueryTask {
        owner: workflow
            .acl
            .as_ref()
            .map(|a| a.owner.clone())
            .unwrap_or_default(),
        project_id: workflow.project_id.clone(),
        query: Some(query.clone()),
        remove_on_gc: true,
        state: Some(proto::EState {
            object: Some(proto::e_state::Object::Initstate(proto::InitState {})),
        }),
        ..Default::default()
    };

    let e_task = proto::ETask {
        object: Some(proto::e_task::Object::Cubequerytask(cqt)),
    };

    let mut svc = client.task_service()?;
    let resp = svc.create(tonic::Request::new(e_task)).await?;
    let created = resp.into_inner();

    match created.object {
        Some(proto::e_task::Object::Cubequerytask(task)) => Ok(task),
        _ => Err("Created task is not a CubeQueryTask".into()),
    }
}

/// Run a task via gRPC
async fn run_task(client: &TercenClient, task_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut svc = client.task_service()?;
    svc.run_task(tonic::Request::new(proto::ReqRunTask {
        task_id: task_id.to_string(),
    }))
    .await?;
    Ok(())
}

/// Wait for a task to complete via gRPC
async fn wait_done(
    client: &TercenClient,
    task_id: &str,
) -> Result<proto::ETask, Box<dyn std::error::Error>> {
    let mut svc = client.task_service()?;
    let resp = svc
        .wait_done(tonic::Request::new(proto::ReqWaitDone {
            task_id: task_id.to_string(),
        }))
        .await?;
    resp.into_inner()
        .result
        .ok_or_else(|| "waitDone returned no result".into())
}

/// Extract state kind from an ETask
fn extract_task_state(task: &proto::ETask) -> String {
    match &task.object {
        Some(proto::e_task::Object::Cubequerytask(cqt)) => cqt
            .state
            .as_ref()
            .and_then(|s| s.object.as_ref())
            .map(|obj| match obj {
                proto::e_state::Object::Donestate(_) => "DoneState".to_string(),
                proto::e_state::Object::Failedstate(fs) => {
                    format!("FailedState: {}", fs.reason)
                }
                proto::e_state::Object::Initstate(_) => "InitState".to_string(),
                proto::e_state::Object::Runningstate(_) => "RunningState".to_string(),
                _ => "Unknown".to_string(),
            })
            .unwrap_or_else(|| "NoState".to_string()),
        _ => "NotCubeQueryTask".to_string(),
    }
}

/// Extract schema_ids from a completed CubeQueryTask
fn extract_schema_ids(task: &proto::ETask) -> Vec<String> {
    match &task.object {
        Some(proto::e_task::Object::Cubequerytask(cqt)) => cqt.schema_ids.clone(),
        _ => Vec::new(),
    }
}

/// Verify that an existing CubeQueryTask is valid and in DoneState
async fn verify_cube_query_task(
    client: &TercenClient,
    task_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut svc = client.task_service()?;
    let resp = svc
        .get(tonic::Request::new(proto::GetRequest {
            id: task_id.to_string(),
            ..Default::default()
        }))
        .await?;
    let task = resp.into_inner();
    let state = extract_task_state(&task);
    Ok(state == "DoneState")
}

/// Update model.taskId on the step by fetching, mutating, and saving the workflow
async fn update_model_task_id(
    client: &TercenClient,
    workflow_id: &str,
    step_id: &str,
    cube_query_task_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Re-fetch the workflow (it may have changed since we last read it)
    let mut svc = client.workflow_service()?;
    let resp = svc
        .get(tonic::Request::new(proto::GetRequest {
            id: workflow_id.to_string(),
            ..Default::default()
        }))
        .await?;
    let mut e_wf = resp.into_inner();

    // Find and mutate the step's model.taskId
    let wf = match &mut e_wf.object {
        Some(proto::e_workflow::Object::Workflow(wf)) => wf,
        _ => return Err("No workflow object".into()),
    };

    let mut found = false;
    for step in &mut wf.steps {
        match &mut step.object {
            Some(proto::e_step::Object::Datastep(ds)) if ds.id == step_id => {
                if let Some(model) = &mut ds.model {
                    model.task_id = cube_query_task_id.to_string();
                    found = true;
                }
            }
            _ => {}
        }
    }

    if !found {
        return Err(format!("DataStep {} not found for taskId update", step_id).into());
    }

    // Save the workflow
    let mut svc = client.workflow_service()?;
    svc.update(tonic::Request::new(e_wf)).await?;

    Ok(())
}

/// Delete a project by setting isDeleted=true via gRPC ProjectService
async fn delete_project(
    client: &TercenClient,
    project_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use proto::project_service_client::ProjectServiceClient;

    println!("[prepare] Deleting project {}...", project_id);

    let interceptor = client.auth_interceptor()?;
    let mut svc = ProjectServiceClient::with_interceptor(client.channel().clone(), interceptor);

    // Fetch the project
    let resp = svc
        .get(tonic::Request::new(proto::GetRequest {
            id: project_id.to_string(),
            ..Default::default()
        }))
        .await?;
    let mut e_project = resp.into_inner();

    // Set isDeleted = true
    match &mut e_project.object {
        Some(proto::e_project::Object::Project(p)) => {
            p.is_deleted = true;
        }
        _ => return Err("Not a Project object".into()),
    }

    // Update
    svc.update(tonic::Request::new(e_project)).await?;
    println!("[prepare] Project deleted");

    Ok(())
}
