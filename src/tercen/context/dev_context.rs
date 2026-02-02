//! DevContext - TercenContext implementation for development/testing mode
//!
//! Initialized from workflow_id + step_id, fetches data from workflow structure.
//! This mirrors Python's OperatorContextDev.

use super::TercenContext;
use crate::tercen::client::proto::{CubeQuery, OperatorSettings};
use crate::tercen::colors::{ChartKind, ColorInfo};
use crate::tercen::TercenClient;
use std::sync::Arc;

/// Development context initialized from workflow_id + step_id
///
/// This is used for local testing when we don't have a task_id.
pub struct DevContext {
    client: Arc<TercenClient>,
    cube_query: CubeQuery,
    schema_ids: Vec<String>,
    workflow_id: String,
    step_id: String,
    project_id: String,
    namespace: String,
    operator_settings: Option<OperatorSettings>,
    color_infos: Vec<ColorInfo>,
    page_factors: Vec<String>,
    y_axis_table_id: Option<String>,
    x_axis_table_id: Option<String>,
    point_size: Option<i32>,
    chart_kind: ChartKind,
}

impl DevContext {
    /// Create a new DevContext from workflow_id and step_id
    ///
    /// This fetches the workflow, finds the step, and extracts the CubeQuery
    /// either from the step's model.task_id or by calling getCubeQuery.
    pub async fn from_workflow_step(
        client: Arc<TercenClient>,
        workflow_id: &str,
        step_id: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        use crate::tercen::client::proto::{e_step, e_task, e_workflow, GetRequest};

        println!("[DevContext] Fetching workflow {}...", workflow_id);

        // Fetch workflow
        let mut workflow_service = client.workflow_service()?;
        let request = tonic::Request::new(GetRequest {
            id: workflow_id.to_string(),
            ..Default::default()
        });
        let response = workflow_service.get(request).await?;
        let e_workflow = response.into_inner();

        let workflow = match e_workflow.object {
            Some(e_workflow::Object::Workflow(wf)) => wf,
            _ => return Err("No workflow object".into()),
        };

        println!("[DevContext] Workflow name: {}", workflow.name);

        // Find the DataStep
        let data_step = workflow
            .steps
            .iter()
            .find_map(|e_step| {
                if let Some(e_step::Object::Datastep(ds)) = &e_step.object {
                    if ds.id == step_id {
                        return Some(ds.clone());
                    }
                }
                None
            })
            .ok_or_else(|| format!("DataStep {} not found in workflow", step_id))?;

        println!("[DevContext] Step name: {}", data_step.name);

        // Get task_id from model (if exists)
        let task_id = data_step
            .model
            .as_ref()
            .map(|m| m.task_id.clone())
            .unwrap_or_default();

        println!("[DevContext] Model task_id: '{}'", task_id);

        // Get CubeQuery and schema_ids
        let (cube_query, schema_ids, project_id) = if task_id.is_empty() {
            // No task_id - call getCubeQuery
            println!("[DevContext] Calling getCubeQuery...");
            let mut workflow_service = client.workflow_service()?;
            let request = tonic::Request::new(crate::tercen::client::proto::ReqGetCubeQuery {
                workflow_id: workflow_id.to_string(),
                step_id: step_id.to_string(),
            });
            let response = workflow_service.get_cube_query(request).await?;
            let resp = response.into_inner();
            let query = resp.result.ok_or("getCubeQuery returned no result")?;

            // getCubeQuery doesn't return schema_ids, so we can't get Y-axis/color tables this way
            // We'll have to leave schema_ids empty
            (query, Vec::new(), String::new())
        } else {
            // Retrieve task to get CubeQuery and schema_ids
            println!("[DevContext] Retrieving task {}...", task_id);
            let mut task_service = client.task_service()?;
            let request = tonic::Request::new(GetRequest {
                id: task_id.clone(),
                ..Default::default()
            });
            let response = task_service.get(request).await?;
            let task = response.into_inner();

            match task.object.as_ref() {
                Some(e_task::Object::Cubequerytask(cqt)) => {
                    let query = cqt.query.as_ref().ok_or("CubeQueryTask has no query")?;
                    (
                        query.clone(),
                        cqt.schema_ids.clone(),
                        cqt.project_id.clone(),
                    )
                }
                Some(e_task::Object::Computationtask(ct)) => {
                    let query = ct.query.as_ref().ok_or("ComputationTask has no query")?;
                    (query.clone(), ct.schema_ids.clone(), ct.project_id.clone())
                }
                Some(e_task::Object::Runcomputationtask(rct)) => {
                    let query = rct
                        .query
                        .as_ref()
                        .ok_or("RunComputationTask has no query")?;
                    (
                        query.clone(),
                        rct.schema_ids.clone(),
                        rct.project_id.clone(),
                    )
                }
                _ => return Err("Task is not a query task".into()),
            }
        };

        println!("[DevContext] CubeQuery retrieved");
        println!("[DevContext]   qt_hash: {}", cube_query.qt_hash);
        println!("[DevContext]   column_hash: {}", cube_query.column_hash);
        println!("[DevContext]   row_hash: {}", cube_query.row_hash);

        // Extract operator settings and namespace from cube_query
        let operator_settings = cube_query.operator_settings.clone();
        let namespace = operator_settings
            .as_ref()
            .map(|os| os.namespace.clone())
            .unwrap_or_default();

        // Find Y-axis table
        let y_axis_table_id = if !schema_ids.is_empty() {
            Self::find_y_axis_table(&client, &schema_ids, &cube_query).await?
        } else {
            None
        };

        // Find X-axis table
        let x_axis_table_id = if !schema_ids.is_empty() {
            Self::find_x_axis_table(&client, &schema_ids, &cube_query).await?
        } else {
            None
        };

        // Extract color information
        let color_infos =
            Self::extract_color_info(&client, &schema_ids, &cube_query, &workflow, step_id).await?;

        // Extract page factors
        let page_factors = crate::tercen::extract_page_factors(operator_settings.as_ref());

        // Extract point size from workflow step
        let point_size = match crate::tercen::extract_point_size_from_step(&workflow, step_id) {
            Ok(ps) => ps,
            Err(e) => {
                eprintln!("[DevContext] Failed to extract point_size: {}", e);
                None
            }
        };

        // Extract chart kind from workflow step
        let chart_kind = match crate::tercen::extract_chart_kind_from_step(&workflow, step_id) {
            Ok(ck) => {
                println!("[DevContext] Chart kind: {:?}", ck);
                ck
            }
            Err(e) => {
                eprintln!("[DevContext] Failed to extract chart_kind: {}", e);
                ChartKind::Point
            }
        };

        Ok(Self {
            client,
            cube_query,
            schema_ids,
            workflow_id: workflow_id.to_string(),
            step_id: step_id.to_string(),
            project_id,
            namespace,
            operator_settings,
            color_infos,
            page_factors,
            y_axis_table_id,
            x_axis_table_id,
            point_size,
            chart_kind,
        })
    }

    /// Find Y-axis table from schema_ids
    async fn find_y_axis_table(
        client: &TercenClient,
        schema_ids: &[String],
        cube_query: &CubeQuery,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        use crate::tercen::client::proto::e_schema;
        use crate::tercen::TableStreamer;

        let streamer = TableStreamer::new(client);

        let known_tables = [
            cube_query.qt_hash.as_str(),
            cube_query.column_hash.as_str(),
            cube_query.row_hash.as_str(),
        ];

        eprintln!(
            "DEBUG find_y_axis_table: schema_ids={:?}, known_tables={:?}",
            schema_ids, known_tables
        );

        for schema_id in schema_ids {
            if !known_tables.contains(&schema_id.as_str()) {
                let schema = streamer.get_schema(schema_id).await?;
                if let Some(e_schema::Object::Cubequerytableschema(cqts)) = schema.object {
                    eprintln!(
                        "DEBUG find_y_axis_table: schema {} has query_table_type='{}'",
                        schema_id, cqts.query_table_type
                    );
                    if cqts.query_table_type == "y" {
                        println!("[DevContext] Found Y-axis table: {}", schema_id);
                        return Ok(Some(schema_id.clone()));
                    }
                }
            } else {
                eprintln!(
                    "DEBUG find_y_axis_table: skipping known table {}",
                    schema_id
                );
            }
        }

        eprintln!("DEBUG find_y_axis_table: No Y-axis table found");
        Ok(None)
    }

    /// Find X-axis table from schema_ids
    async fn find_x_axis_table(
        client: &TercenClient,
        schema_ids: &[String],
        cube_query: &CubeQuery,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        use crate::tercen::client::proto::e_schema;
        use crate::tercen::TableStreamer;

        let streamer = TableStreamer::new(client);

        let known_tables = [
            cube_query.qt_hash.as_str(),
            cube_query.column_hash.as_str(),
            cube_query.row_hash.as_str(),
        ];

        for schema_id in schema_ids {
            if !known_tables.contains(&schema_id.as_str()) {
                let schema = streamer.get_schema(schema_id).await?;
                if let Some(e_schema::Object::Cubequerytableschema(cqts)) = schema.object {
                    if cqts.query_table_type == "x" {
                        println!("[DevContext] Found X-axis table: {}", schema_id);
                        return Ok(Some(schema_id.clone()));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Extract color information from workflow
    async fn extract_color_info(
        client: &TercenClient,
        schema_ids: &[String],
        _cube_query: &CubeQuery,
        workflow: &crate::tercen::client::proto::Workflow,
        step_id: &str,
    ) -> Result<Vec<ColorInfo>, Box<dyn std::error::Error>> {
        use crate::tercen::client::proto::{e_column_schema, e_schema};
        use crate::tercen::TableStreamer;

        if schema_ids.is_empty() {
            println!("[DevContext] No schema_ids available - skipping color extraction");
            return Ok(Vec::new());
        }

        // Find color tables and cache their schemas
        // Note: A color factor may be the same as a facet factor (column/row),
        // so we check ALL schema_ids, not just "unknown" ones.
        let streamer = TableStreamer::new(client);
        let mut color_table_ids: Vec<Option<String>> = Vec::new();
        let mut color_table_schemas: std::collections::HashMap<
            String,
            crate::tercen::client::proto::CubeQueryTableSchema,
        > = std::collections::HashMap::new();

        for schema_id in schema_ids {
            let schema = streamer.get_schema(schema_id).await?;
            if let Some(e_schema::Object::Cubequerytableschema(cqts)) = schema.object {
                if cqts.query_table_type.starts_with("color_") {
                    if let Some(idx_str) = cqts.query_table_type.strip_prefix("color_") {
                        if let Ok(idx) = idx_str.parse::<usize>() {
                            while color_table_ids.len() <= idx {
                                color_table_ids.push(None);
                            }
                            color_table_ids[idx] = Some(schema_id.clone());
                            color_table_schemas.insert(schema_id.clone(), cqts);
                            println!("[DevContext] Found color table {}: {}", idx, schema_id);
                        }
                    }
                }
            }
        }

        // Extract color info from step
        let mut color_infos =
            crate::tercen::extract_color_info_from_step(workflow, step_id, &color_table_ids)?;

        // All color factors share the same color table (color_0)
        // Assign the color table ID to ALL factors, not just the first
        let shared_color_table_id = color_table_ids.first().and_then(|opt| opt.clone());
        if let Some(ref table_id) = shared_color_table_id {
            for color_info in &mut color_infos {
                if color_info.color_table_id.is_none() {
                    eprintln!(
                        "DEBUG extract_color_info: assigning shared color table {} to factor '{}'",
                        table_id, color_info.factor_name
                    );
                    color_info.color_table_id = Some(table_id.clone());
                }
            }
        }

        // Fetch actual color labels from color table for categorical colors
        // With multiple factors, the color table has the cross-product of all factors
        // Each row is a unique combination (e.g., "F, BD") that maps to a .colorLevels index
        // We only need to populate labels for the FIRST categorical color factor
        // (since .colorLevels is a single column representing the combined index)
        if let Some(first_categorical_idx) = color_infos
            .iter()
            .position(|ci| matches!(ci.mapping, crate::tercen::ColorMapping::Categorical(_)))
        {
            let color_info = &color_infos[first_categorical_idx];

            if let Some(ref table_id) = color_info.color_table_id {
                // Get the schema to find all factor columns and row count
                if let Some(cqts) = color_table_schemas.get(table_id) {
                    let n_rows = cqts.n_rows as usize;

                    // Get all factor column names from the schema
                    let factor_columns: Vec<String> = cqts
                        .columns
                        .iter()
                        .filter_map(|c| {
                            if let Some(e_column_schema::Object::Columnschema(cs)) = &c.object {
                                Some(cs.name.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    if n_rows > 0 && !factor_columns.is_empty() {
                        eprintln!(
                            "DEBUG extract_color_info: fetching combined color labels from table {} ({} rows, columns: {:?})",
                            table_id, n_rows, factor_columns
                        );

                        // Stream all factor columns
                        match streamer
                            .stream_tson(table_id, Some(factor_columns.clone()), 0, n_rows as i64)
                            .await
                        {
                            Ok(tson_data) => {
                                if !tson_data.is_empty() {
                                    match crate::tercen::tson_to_dataframe(&tson_data) {
                                        Ok(df) => {
                                            // Create combined labels by joining all factor values
                                            let mut combined_labels = Vec::with_capacity(n_rows);
                                            for i in 0..df.nrow() {
                                                let parts: Vec<String> = factor_columns
                                                    .iter()
                                                    .filter_map(|col| {
                                                        df.get_value(i, col)
                                                            .ok()
                                                            .map(|v| v.as_string())
                                                    })
                                                    .collect();
                                                combined_labels.push(parts.join(", "));
                                            }
                                            eprintln!(
                                                "DEBUG extract_color_info: got {} combined color labels: {:?}",
                                                combined_labels.len(),
                                                combined_labels
                                            );

                                            // Set labels on the first categorical color factor
                                            color_infos[first_categorical_idx].n_levels =
                                                Some(combined_labels.len());
                                            color_infos[first_categorical_idx].color_labels =
                                                Some(combined_labels);
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "WARN extract_color_info: failed to parse color table TSON: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "WARN extract_color_info: failed to stream color table {}: {}",
                                    table_id, e
                                );
                            }
                        }
                    }
                }
            }
        }

        // Fetch quartiles for continuous color mappings that are not user-defined
        for color_info in &mut color_infos {
            // Only process continuous mappings
            let is_user_defined = match &color_info.mapping {
                crate::tercen::ColorMapping::Continuous(palette) => palette.is_user_defined,
                _ => true, // Categorical is always "user defined" in our context
            };

            eprintln!(
                "DEBUG extract_color_info: factor='{}' is_user_defined={}",
                color_info.factor_name, is_user_defined
            );

            if !is_user_defined {
                // Need to fetch quartiles from the color table schema
                if let Some(ref table_id) = color_info.color_table_id {
                    if let Some(cqts) = color_table_schemas.get(table_id) {
                        // Find the column that matches the color factor
                        for col_schema in &cqts.columns {
                            if let Some(e_column_schema::Object::Columnschema(cs)) =
                                &col_schema.object
                            {
                                if cs.name == color_info.factor_name {
                                    if let Some(ref meta) = cs.meta_data {
                                        if !meta.quartiles.is_empty() {
                                            eprintln!(
                                                "DEBUG extract_color_info: Found quartiles for '{}': {:?}",
                                                color_info.factor_name, meta.quartiles
                                            );
                                            color_info.quartiles = Some(meta.quartiles.clone());
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }

                // If we still don't have quartiles, warn
                if color_info.quartiles.is_none() {
                    eprintln!(
                        "WARN extract_color_info: is_user_defined=false for '{}' but no quartiles found",
                        color_info.factor_name
                    );
                }
            }
        }

        Ok(color_infos)
    }
}

impl TercenContext for DevContext {
    fn cube_query(&self) -> &CubeQuery {
        &self.cube_query
    }

    fn schema_ids(&self) -> &[String] {
        &self.schema_ids
    }

    fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    fn step_id(&self) -> &str {
        &self.step_id
    }

    fn project_id(&self) -> &str {
        &self.project_id
    }

    fn namespace(&self) -> &str {
        &self.namespace
    }

    fn operator_settings(&self) -> Option<&OperatorSettings> {
        self.operator_settings.as_ref()
    }

    fn color_infos(&self) -> &[ColorInfo] {
        &self.color_infos
    }

    fn page_factors(&self) -> &[String] {
        &self.page_factors
    }

    fn y_axis_table_id(&self) -> Option<&str> {
        self.y_axis_table_id.as_deref()
    }

    fn x_axis_table_id(&self) -> Option<&str> {
        self.x_axis_table_id.as_deref()
    }

    fn point_size(&self) -> Option<i32> {
        self.point_size
    }

    fn chart_kind(&self) -> ChartKind {
        self.chart_kind
    }

    fn client(&self) -> &Arc<TercenClient> {
        &self.client
    }
}
