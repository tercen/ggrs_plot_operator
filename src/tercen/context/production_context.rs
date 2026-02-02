//! ProductionContext - TercenContext implementation for production mode
//!
//! Initialized from a task_id, extracts all necessary data from the task object.

use super::TercenContext;
use crate::tercen::client::proto::{CubeQuery, OperatorSettings};
use crate::tercen::colors::{ChartKind, ColorInfo};
use crate::tercen::TercenClient;
use std::sync::Arc;

/// Production context initialized from task_id
///
/// This is used when the operator is run by Tercen with --taskId argument.
pub struct ProductionContext {
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
    /// Crosstab dimensions from model (width, height) in pixels
    /// Calculated as cellSize × nRows for each axis
    crosstab_dimensions: Option<(i32, i32)>,
}

impl ProductionContext {
    /// Create a new ProductionContext from a task_id
    ///
    /// This fetches the task, extracts the CubeQuery, schema_ids, and other
    /// necessary data for plot generation.
    pub async fn from_task_id(
        client: Arc<TercenClient>,
        task_id: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        println!("[ProductionContext] Fetching task {}...", task_id);

        // Fetch task with retry logic for schema_ids
        // Rust operators start faster than R/Python, sometimes before backend is ready
        let (cube_query, schema_ids, project_id, operator_settings, workflow_id, step_id) =
            Self::fetch_task_with_retry(&client, task_id).await?;

        // Extract namespace from operator settings
        let namespace = operator_settings
            .as_ref()
            .map(|os| os.namespace.clone())
            .unwrap_or_default();

        println!(
            "[ProductionContext] workflow_id={}, step_id={}",
            workflow_id, step_id
        );

        if schema_ids.is_empty() {
            println!("[ProductionContext] schema_ids is empty");
        } else {
            println!(
                "[ProductionContext] Found {} schema_ids: {:?}",
                schema_ids.len(),
                schema_ids
            );
        }

        // Find Y-axis table
        let y_axis_table_id = Self::find_y_axis_table(&client, &schema_ids, &cube_query).await?;

        // Find X-axis table
        let x_axis_table_id = Self::find_x_axis_table(&client, &schema_ids, &cube_query).await?;

        // Extract color information
        let color_infos =
            Self::extract_color_info(&client, &schema_ids, &cube_query, &workflow_id, &step_id)
                .await?;

        // Extract page factors from operator settings
        let page_factors = crate::tercen::extract_page_factors(operator_settings.as_ref());

        // Extract point size from workflow step
        let point_size = Self::extract_point_size(&client, &workflow_id, &step_id).await?;

        // Extract chart kind from workflow step
        let chart_kind = Self::extract_chart_kind(&client, &workflow_id, &step_id).await?;

        // Extract crosstab dimensions from workflow step model
        let crosstab_dimensions =
            Self::extract_crosstab_dimensions(&client, &workflow_id, &step_id).await?;

        Ok(Self {
            client,
            cube_query,
            schema_ids,
            workflow_id,
            step_id,
            project_id,
            namespace,
            operator_settings,
            color_infos,
            page_factors,
            y_axis_table_id,
            x_axis_table_id,
            point_size,
            chart_kind,
            crosstab_dimensions,
        })
    }

    /// Fetch task with retry logic when schema_ids is empty
    ///
    /// Rust operators start faster than R/Python, sometimes before the backend
    /// has finished populating schema_ids. Retries with backoff: 500ms, 1000ms, 2000ms.
    async fn fetch_task_with_retry(
        client: &TercenClient,
        task_id: &str,
    ) -> Result<
        (
            CubeQuery,
            Vec<String>,
            String,
            Option<OperatorSettings>,
            String,
            String,
        ),
        Box<dyn std::error::Error>,
    > {
        use tokio::time::{sleep, Duration};

        let retry_delays_ms = [500, 1000, 2000];

        // First attempt
        let result = Self::fetch_task_once(client, task_id).await?;
        if !result.1.is_empty() {
            return Ok(result);
        }

        eprintln!(
            "DEBUG: schema_ids empty on first attempt, will retry (task_id={})",
            task_id
        );

        // Retry with backoff
        for (attempt, delay_ms) in retry_delays_ms.iter().enumerate() {
            eprintln!(
                "DEBUG: Retrying task fetch (attempt {}/{}), waiting {}ms...",
                attempt + 2,
                retry_delays_ms.len() + 1,
                delay_ms
            );
            sleep(Duration::from_millis(*delay_ms)).await;

            let result = Self::fetch_task_once(client, task_id).await?;
            if !result.1.is_empty() {
                eprintln!(
                    "DEBUG: schema_ids fetch succeeded on attempt {} with {} ids",
                    attempt + 2,
                    result.1.len()
                );
                return Ok(result);
            }
            eprintln!("DEBUG: schema_ids still empty on attempt {}", attempt + 2);
        }

        // All retries exhausted - fail with clear error
        Err(format!(
            "Task {} schema_ids is empty after {} retries (total wait: 3.5s). \
             The backend may not be ready yet.",
            task_id,
            retry_delays_ms.len()
        )
        .into())
    }

    /// Fetch task once and extract required data
    async fn fetch_task_once(
        client: &TercenClient,
        task_id: &str,
    ) -> Result<
        (
            CubeQuery,
            Vec<String>,
            String,
            Option<OperatorSettings>,
            String,
            String,
        ),
        Box<dyn std::error::Error>,
    > {
        use crate::tercen::client::proto::{e_task, GetRequest};

        let mut task_service = client.task_service()?;
        let request = tonic::Request::new(GetRequest {
            id: task_id.to_string(),
            ..Default::default()
        });
        let response = task_service.get(request).await?;
        let task = response.into_inner();

        // Extract data from task based on task type
        let (cube_query, schema_ids, project_id, operator_settings, task_environment) =
            match task.object.as_ref() {
                Some(e_task::Object::Computationtask(ct)) => (
                    ct.query
                        .as_ref()
                        .ok_or("ComputationTask has no query")?
                        .clone(),
                    ct.schema_ids.clone(),
                    ct.project_id.clone(),
                    ct.query.as_ref().and_then(|q| q.operator_settings.clone()),
                    &ct.environment,
                ),
                Some(e_task::Object::Runcomputationtask(rct)) => (
                    rct.query
                        .as_ref()
                        .ok_or("RunComputationTask has no query")?
                        .clone(),
                    rct.schema_ids.clone(),
                    rct.project_id.clone(),
                    rct.query.as_ref().and_then(|q| q.operator_settings.clone()),
                    &rct.environment,
                ),
                Some(e_task::Object::Cubequerytask(cqt)) => (
                    cqt.query
                        .as_ref()
                        .ok_or("CubeQueryTask has no query")?
                        .clone(),
                    cqt.schema_ids.clone(),
                    cqt.project_id.clone(),
                    cqt.query.as_ref().and_then(|q| q.operator_settings.clone()),
                    &cqt.environment,
                ),
                _ => return Err("Unsupported task type".into()),
            };

        // Get workflow_id and step_id from task environment
        let workflow_id = task_environment
            .iter()
            .find(|p| p.key == "workflow.id")
            .map(|p| p.value.clone())
            .or_else(|| std::env::var("WORKFLOW_ID").ok())
            .unwrap_or_default();

        let step_id = task_environment
            .iter()
            .find(|p| p.key == "step.id")
            .map(|p| p.value.clone())
            .or_else(|| std::env::var("STEP_ID").ok())
            .unwrap_or_else(|| task_id.to_string());

        Ok((
            cube_query,
            schema_ids,
            project_id,
            operator_settings,
            workflow_id,
            step_id,
        ))
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
                        println!("[ProductionContext] Found Y-axis table: {}", schema_id);
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
                        println!("[ProductionContext] Found X-axis table: {}", schema_id);
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
        workflow_id: &str,
        step_id: &str,
    ) -> Result<Vec<ColorInfo>, Box<dyn std::error::Error>> {
        use crate::tercen::client::proto::{e_column_schema, e_schema};
        use crate::tercen::TableStreamer;

        if workflow_id.is_empty() || step_id.is_empty() {
            println!(
                "[ProductionContext] Workflow/Step IDs not available - skipping color extraction"
            );
            return Ok(Vec::new());
        }

        // Fetch workflow
        let mut workflow_service = client.workflow_service()?;
        let request = tonic::Request::new(crate::tercen::client::proto::GetRequest {
            id: workflow_id.to_string(),
            ..Default::default()
        });
        let response = workflow_service.get(request).await?;
        let e_workflow = response.into_inner();

        let workflow = e_workflow
            .object
            .as_ref()
            .map(|obj| match obj {
                crate::tercen::client::proto::e_workflow::Object::Workflow(wf) => wf,
            })
            .ok_or("EWorkflow has no workflow object")?;

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

    /// Extract point size from workflow step
    async fn extract_point_size(
        client: &TercenClient,
        workflow_id: &str,
        step_id: &str,
    ) -> Result<Option<i32>, Box<dyn std::error::Error>> {
        if workflow_id.is_empty() || step_id.is_empty() {
            println!(
                "[ProductionContext] Workflow/Step IDs not available - skipping point_size extraction"
            );
            return Ok(None);
        }

        // Fetch workflow
        let mut workflow_service = client.workflow_service()?;
        let request = tonic::Request::new(crate::tercen::client::proto::GetRequest {
            id: workflow_id.to_string(),
            ..Default::default()
        });
        let response = workflow_service.get(request).await?;
        let e_workflow = response.into_inner();

        let workflow = e_workflow
            .object
            .as_ref()
            .map(|obj| match obj {
                crate::tercen::client::proto::e_workflow::Object::Workflow(wf) => wf,
            })
            .ok_or("EWorkflow has no workflow object")?;

        // Extract point size from step
        match crate::tercen::extract_point_size_from_step(workflow, step_id) {
            Ok(ps) => Ok(ps),
            Err(e) => {
                eprintln!("[ProductionContext] Failed to extract point_size: {}", e);
                Ok(None) // Use default on error
            }
        }
    }

    /// Extract chart kind from workflow step
    async fn extract_chart_kind(
        client: &TercenClient,
        workflow_id: &str,
        step_id: &str,
    ) -> Result<ChartKind, Box<dyn std::error::Error>> {
        if workflow_id.is_empty() || step_id.is_empty() {
            println!(
                "[ProductionContext] Workflow/Step IDs not available - defaulting to Point chart"
            );
            return Ok(ChartKind::Point);
        }

        // Fetch workflow
        let mut workflow_service = client.workflow_service()?;
        let request = tonic::Request::new(crate::tercen::client::proto::GetRequest {
            id: workflow_id.to_string(),
            ..Default::default()
        });
        let response = workflow_service.get(request).await?;
        let e_workflow = response.into_inner();

        let workflow = e_workflow
            .object
            .as_ref()
            .map(|obj| match obj {
                crate::tercen::client::proto::e_workflow::Object::Workflow(wf) => wf,
            })
            .ok_or("EWorkflow has no workflow object")?;

        // Extract chart kind from step
        match crate::tercen::extract_chart_kind_from_step(workflow, step_id) {
            Ok(ck) => {
                println!("[ProductionContext] Chart kind: {:?}", ck);
                Ok(ck)
            }
            Err(e) => {
                eprintln!("[ProductionContext] Failed to extract chart_kind: {}", e);
                Ok(ChartKind::Point) // Default to point on error
            }
        }
    }

    /// Extract crosstab dimensions from workflow step model
    ///
    /// Returns (width, height) calculated as:
    /// - width = columnTable.cellSize × columnTable.nRows
    /// - height = rowTable.cellSize × rowTable.nRows
    async fn extract_crosstab_dimensions(
        client: &TercenClient,
        workflow_id: &str,
        step_id: &str,
    ) -> Result<Option<(i32, i32)>, Box<dyn std::error::Error>> {
        use crate::tercen::client::proto::e_step;

        if workflow_id.is_empty() || step_id.is_empty() {
            return Ok(None);
        }

        // Fetch workflow
        let mut workflow_service = client.workflow_service()?;
        let request = tonic::Request::new(crate::tercen::client::proto::GetRequest {
            id: workflow_id.to_string(),
            ..Default::default()
        });
        let response = workflow_service.get(request).await?;
        let e_workflow = response.into_inner();

        let workflow = e_workflow
            .object
            .as_ref()
            .map(|obj| match obj {
                crate::tercen::client::proto::e_workflow::Object::Workflow(wf) => wf,
            })
            .ok_or("EWorkflow has no workflow object")?;

        // Find the step
        let step = workflow.steps.iter().find(|s| match &s.object {
            Some(e_step::Object::Datastep(ds)) => ds.id == step_id,
            Some(e_step::Object::Crosstabstep(cs)) => cs.id == step_id,
            _ => false,
        });

        let step = match step {
            Some(s) => s,
            None => return Ok(None),
        };

        // Get the Crosstab model from the step
        let model = match &step.object {
            Some(e_step::Object::Datastep(ds)) => ds.model.as_ref(),
            Some(e_step::Object::Crosstabstep(cs)) => cs.model.as_ref(),
            _ => None,
        };

        let model = match model {
            Some(m) => m,
            None => return Ok(None),
        };

        // Extract dimensions from columnTable and rowTable
        let width = model.column_table.as_ref().map(|ct| {
            let cell_size = ct.cell_size as i32;
            let n_rows = ct.n_rows.max(1); // At least 1
            cell_size * n_rows
        });

        let height = model.row_table.as_ref().map(|rt| {
            let cell_size = rt.cell_size as i32;
            let n_rows = rt.n_rows.max(1); // At least 1
            cell_size * n_rows
        });

        match (width, height) {
            (Some(w), Some(h)) if w > 0 && h > 0 => {
                println!(
                    "[ProductionContext] Crosstab dimensions: {}×{} pixels",
                    w, h
                );
                Ok(Some((w, h)))
            }
            _ => {
                eprintln!("[ProductionContext] Could not extract crosstab dimensions");
                Ok(None)
            }
        }
    }
}

impl TercenContext for ProductionContext {
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

    fn crosstab_dimensions(&self) -> Option<(i32, i32)> {
        self.crosstab_dimensions
    }

    fn client(&self) -> &Arc<TercenClient> {
        &self.client
    }
}
