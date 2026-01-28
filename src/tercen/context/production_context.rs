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
    point_size: Option<i32>,
    chart_kind: ChartKind,
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
        use crate::tercen::client::proto::{e_task, GetRequest};

        println!("[ProductionContext] Fetching task {}...", task_id);

        // Fetch the task
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

        // Extract namespace from operator settings
        let namespace = operator_settings
            .as_ref()
            .map(|os| os.namespace.clone())
            .unwrap_or_default();

        // Get workflow_id and step_id from task environment
        // Tercen uses "workflow.id" and "step.id" keys (with dots, not underscores)
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

        println!(
            "[ProductionContext] workflow_id={}, step_id={}",
            workflow_id, step_id
        );

        // Find Y-axis table
        let y_axis_table_id = Self::find_y_axis_table(&client, &schema_ids, &cube_query).await?;

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

        for schema_id in schema_ids {
            if !known_tables.contains(&schema_id.as_str()) {
                let schema = streamer.get_schema(schema_id).await?;
                if let Some(e_schema::Object::Cubequerytableschema(cqts)) = schema.object {
                    if cqts.query_table_type == "y" {
                        println!("[ProductionContext] Found Y-axis table: {}", schema_id);
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
        use crate::tercen::client::proto::e_schema;
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

        // Find color tables
        // Note: A color factor may be the same as a facet factor (column/row),
        // so we check ALL schema_ids, not just "unknown" ones.
        let streamer = TableStreamer::new(client);
        let mut color_table_ids: Vec<Option<String>> = Vec::new();

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
                        }
                    }
                }
            }
        }

        // Extract color info from step
        let color_infos =
            crate::tercen::extract_color_info_from_step(workflow, step_id, &color_table_ids)?;

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
