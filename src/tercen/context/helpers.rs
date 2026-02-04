//! Shared helper functions for context implementations
//!
//! These functions are used by both ProductionContext and DevContext to avoid duplication.

use crate::tercen::client::proto::{CubeQuery, CubeQueryTableSchema, Workflow};
use crate::tercen::colors::ColorInfo;
use crate::tercen::TercenClient;
use std::collections::HashMap;

/// Find Y-axis table from schema_ids
///
/// Searches through schema_ids to find a table with query_table_type == "y".
/// Skips known tables (qt_hash, column_hash, row_hash).
pub async fn find_y_axis_table(
    client: &TercenClient,
    schema_ids: &[String],
    cube_query: &CubeQuery,
    context_name: &str,
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
                    println!("[{}] Found Y-axis table: {}", context_name, schema_id);
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
///
/// Searches through schema_ids to find a table with query_table_type == "x".
/// Skips known tables (qt_hash, column_hash, row_hash).
pub async fn find_x_axis_table(
    client: &TercenClient,
    schema_ids: &[String],
    cube_query: &CubeQuery,
    context_name: &str,
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
                    println!("[{}] Found X-axis table: {}", context_name, schema_id);
                    return Ok(Some(schema_id.clone()));
                }
            }
        }
    }

    Ok(None)
}

/// Find color tables from schema_ids
///
/// Returns a tuple of:
/// - Vec of color table IDs (indexed by color_N suffix)
/// - HashMap of schema_id -> CubeQueryTableSchema for color tables
pub async fn find_color_tables(
    client: &TercenClient,
    schema_ids: &[String],
) -> Result<(Vec<Option<String>>, HashMap<String, CubeQueryTableSchema>), Box<dyn std::error::Error>>
{
    use crate::tercen::client::proto::e_schema;
    use crate::tercen::TableStreamer;

    let streamer = TableStreamer::new(client);
    let mut color_table_ids: Vec<Option<String>> = Vec::new();
    let mut color_table_schemas: HashMap<String, CubeQueryTableSchema> = HashMap::new();

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

    Ok((color_table_ids, color_table_schemas))
}

/// Extract color information from workflow (core implementation)
///
/// This is the shared implementation used by both ProductionContext and DevContext.
/// The workflow must already be fetched.
pub async fn extract_color_info_from_workflow(
    client: &TercenClient,
    schema_ids: &[String],
    workflow: &Workflow,
    step_id: &str,
    context_name: &str,
) -> Result<Vec<ColorInfo>, Box<dyn std::error::Error>> {
    use crate::tercen::client::proto::e_column_schema;
    use crate::tercen::TableStreamer;

    if schema_ids.is_empty() {
        println!(
            "[{}] No schema_ids available - skipping color extraction",
            context_name
        );
        return Ok(Vec::new());
    }

    // Find color tables and cache their schemas
    let (color_table_ids, color_table_schemas) = find_color_tables(client, schema_ids).await?;

    for (idx, table_id) in color_table_ids.iter().enumerate() {
        if let Some(id) = table_id {
            println!("[{}] Found color table {}: {}", context_name, idx, id);
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
    let streamer = TableStreamer::new(client);
    if let Some(first_categorical_idx) = color_infos
        .iter()
        .position(|ci| matches!(ci.mapping, crate::tercen::ColorMapping::Categorical(_)))
    {
        let color_info = &color_infos[first_categorical_idx];

        if let Some(ref table_id) = color_info.color_table_id {
            if let Some(cqts) = color_table_schemas.get(table_id) {
                let n_rows = cqts.n_rows as usize;

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

                    match streamer
                        .stream_tson(table_id, Some(factor_columns.clone()), 0, n_rows as i64)
                        .await
                    {
                        Ok(tson_data) => {
                            if !tson_data.is_empty() {
                                match crate::tercen::tson_to_dataframe(&tson_data) {
                                    Ok(df) => {
                                        let mut combined_labels = Vec::with_capacity(n_rows);
                                        for i in 0..df.nrow() {
                                            let parts: Vec<String> = factor_columns
                                                .iter()
                                                .filter_map(|col| {
                                                    df.get_value(i, col).ok().map(|v| v.as_string())
                                                })
                                                .collect();
                                            combined_labels.push(parts.join(", "));
                                        }
                                        eprintln!(
                                            "DEBUG extract_color_info: got {} combined color labels: {:?}",
                                            combined_labels.len(),
                                            combined_labels
                                        );

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
        let is_user_defined = match &color_info.mapping {
            crate::tercen::ColorMapping::Continuous(palette) => palette.is_user_defined,
            _ => true,
        };

        eprintln!(
            "DEBUG extract_color_info: factor='{}' is_user_defined={}",
            color_info.factor_name, is_user_defined
        );

        if !is_user_defined {
            if let Some(ref table_id) = color_info.color_table_id {
                if let Some(cqts) = color_table_schemas.get(table_id) {
                    for col_schema in &cqts.columns {
                        if let Some(e_column_schema::Object::Columnschema(cs)) = &col_schema.object
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

/// Extract axis transform types from CubeQuery
///
/// Transforms are stored in CubeQuery.axisQueries[0].preprocessors
/// The structure is:
/// - preprocessors[i].type = "y" or "x" (which axis the transform applies to)
/// - preprocessors[i].operatorRef.name = "log", "asinh", etc. (the actual transform)
pub fn extract_transforms_from_cube_query(
    cube_query: &CubeQuery,
) -> (Option<String>, Option<String>) {
    for (i, aq) in cube_query.axis_queries.iter().enumerate() {
        for (j, pp) in aq.preprocessors.iter().enumerate() {
            let transform_name = pp
                .operator_ref
                .as_ref()
                .map(|op_ref| op_ref.name.as_str())
                .unwrap_or("");

            eprintln!(
                "DEBUG extract_transforms: axisQuery[{}].preprocessors[{}] type='{}', operatorRef.name='{}'",
                i, j, pp.r#type, transform_name
            );
        }
    }

    let axis_query = match cube_query.axis_queries.first() {
        Some(aq) => aq,
        None => return (None, None),
    };

    let mut y_transform = None;
    let mut x_transform = None;

    for pp in &axis_query.preprocessors {
        let transform_name = pp
            .operator_ref
            .as_ref()
            .map(|op_ref| op_ref.name.as_str())
            .unwrap_or("");

        let axis_type = pp.r#type.as_str();

        let is_valid_transform = matches!(
            transform_name,
            "log" | "log10" | "ln" | "log2" | "asinh" | "sqrt"
        );

        if is_valid_transform {
            match axis_type {
                "y" => {
                    println!("[Context] Y-axis transform: {}", transform_name);
                    y_transform = Some(transform_name.to_string());
                }
                "x" => {
                    println!("[Context] X-axis transform: {}", transform_name);
                    x_transform = Some(transform_name.to_string());
                }
                _ => {}
            }
        }
    }

    (y_transform, x_transform)
}

/// Extract axis transform types from Crosstab model (step.model.axis.xyAxis)
///
/// This is used by DevContext which has direct access to the step model.
/// The structure is:
/// - xyAxis[0].preprocessors[i].type = "y" or "x"
/// - xyAxis[0].preprocessors[i].operatorRef.name = "log", "asinh", etc.
pub fn extract_transforms_from_step(
    workflow: &Workflow,
    step_id: &str,
    cube_query: &CubeQuery,
) -> (Option<String>, Option<String>) {
    use crate::tercen::client::proto::e_step;

    // First, try to get transforms from the Crosstab model (step.model.axis.xyAxis)
    let step = workflow.steps.iter().find(|s| match &s.object {
        Some(e_step::Object::Datastep(ds)) => ds.id == step_id,
        Some(e_step::Object::Crosstabstep(cs)) => cs.id == step_id,
        _ => false,
    });

    if let Some(step) = step {
        let model = match &step.object {
            Some(e_step::Object::Datastep(ds)) => ds.model.as_ref(),
            Some(e_step::Object::Crosstabstep(cs)) => cs.model.as_ref(),
            _ => None,
        };

        if let Some(crosstab) = model {
            if let Some(ref axis_list) = crosstab.axis {
                eprintln!(
                    "DEBUG extract_transforms: Found {} xyAxis in Crosstab.axis",
                    axis_list.xy_axis.len()
                );

                for (i, xy_axis) in axis_list.xy_axis.iter().enumerate() {
                    eprintln!(
                        "DEBUG extract_transforms: xyAxis[{}] has {} preprocessors",
                        i,
                        xy_axis.preprocessors.len()
                    );
                    for (j, pp) in xy_axis.preprocessors.iter().enumerate() {
                        eprintln!(
                            "DEBUG extract_transforms: xyAxis[{}].preprocessors[{}].type = '{}'",
                            i, j, pp.r#type
                        );
                    }
                }

                if let Some(xy_axis) = axis_list.xy_axis.first() {
                    // Check yAxis.axisSettings.meta for transform info
                    if let Some(ref y_axis) = xy_axis.y_axis {
                        if let Some(ref axis_settings) = y_axis.axis_settings {
                            eprintln!(
                                "DEBUG extract_transforms: yAxis.axisSettings.meta has {} pairs",
                                axis_settings.meta.len()
                            );
                            for pair in &axis_settings.meta {
                                eprintln!(
                                    "DEBUG extract_transforms:   yAxis.axisSettings.meta['{}'] = '{}'",
                                    pair.key, pair.value
                                );
                            }

                            for pair in &axis_settings.meta {
                                if pair.key == "transform" || pair.key == "scale" {
                                    let t = pair.value.as_str();
                                    if matches!(
                                        t,
                                        "log" | "log10" | "ln" | "log2" | "asinh" | "sqrt"
                                    ) {
                                        println!(
                                            "[DevContext] Y-axis transform (from yAxis.axisSettings): {}",
                                            t
                                        );
                                        return (Some(t.to_string()), None);
                                    }
                                }
                            }
                        }
                    }

                    // Check xAxis.axisSettings.meta
                    if let Some(ref x_axis) = xy_axis.x_axis {
                        if let Some(ref axis_settings) = x_axis.axis_settings {
                            eprintln!(
                                "DEBUG extract_transforms: xAxis.axisSettings.meta has {} pairs",
                                axis_settings.meta.len()
                            );
                            for pair in &axis_settings.meta {
                                eprintln!(
                                    "DEBUG extract_transforms:   xAxis.axisSettings.meta['{}'] = '{}'",
                                    pair.key, pair.value
                                );
                            }
                        }
                    }

                    // Extract transforms from preprocessors
                    let mut y_transform = None;
                    let mut x_transform = None;

                    for pp in &xy_axis.preprocessors {
                        let transform_name = pp
                            .operator_ref
                            .as_ref()
                            .map(|op_ref| op_ref.name.as_str())
                            .unwrap_or("");

                        let axis_type = pp.r#type.as_str();

                        eprintln!(
                            "DEBUG extract_transforms: preprocessor type='{}', operatorRef.name='{}'",
                            axis_type, transform_name
                        );

                        let is_valid_transform = matches!(
                            transform_name,
                            "log" | "log10" | "ln" | "log2" | "asinh" | "sqrt"
                        );

                        if is_valid_transform {
                            match axis_type {
                                "y" => {
                                    println!("[DevContext] Y-axis transform: {}", transform_name);
                                    y_transform = Some(transform_name.to_string());
                                }
                                "x" => {
                                    println!("[DevContext] X-axis transform: {}", transform_name);
                                    x_transform = Some(transform_name.to_string());
                                }
                                _ => {}
                            }
                        }
                    }

                    if y_transform.is_some() || x_transform.is_some() {
                        return (y_transform, x_transform);
                    }
                }
            }
        }
    }

    // Fallback: check CubeQuery.axisQueries
    eprintln!(
        "DEBUG extract_transforms: Checking CubeQuery.axisQueries ({} queries)",
        cube_query.axis_queries.len()
    );

    for (i, aq) in cube_query.axis_queries.iter().enumerate() {
        eprintln!(
            "DEBUG extract_transforms: axisQuery[{}] has {} preprocessors, chart_type='{}'",
            i,
            aq.preprocessors.len(),
            aq.chart_type
        );
        for (j, pp) in aq.preprocessors.iter().enumerate() {
            eprintln!(
                "DEBUG extract_transforms: axisQuery[{}].preprocessors[{}].type = '{}'",
                i, j, pp.r#type
            );
        }
    }

    if let Some(axis_query) = cube_query.axis_queries.first() {
        let y_transform = axis_query.preprocessors.iter().find_map(|pp| {
            let t = pp.r#type.as_str();
            match t {
                "log" | "log10" | "ln" | "log2" | "asinh" | "sqrt" => {
                    println!("[DevContext] Y-axis transform (from CubeQuery): {}", t);
                    Some(t.to_string())
                }
                _ => None,
            }
        });

        if y_transform.is_some() {
            return (y_transform, None);
        }
    }

    (None, None)
}

/// Extract crosstab dimensions from workflow step model
///
/// Returns (width, height) calculated as:
/// - width = columnTable.cellSize × columnTable.nRows
/// - height = rowTable.cellSize × rowTable.nRows
pub fn extract_crosstab_dimensions(workflow: &Workflow, step_id: &str) -> Option<(i32, i32)> {
    use crate::tercen::client::proto::e_step;

    let step = workflow.steps.iter().find(|s| match &s.object {
        Some(e_step::Object::Datastep(ds)) => ds.id == step_id,
        Some(e_step::Object::Crosstabstep(cs)) => cs.id == step_id,
        _ => false,
    })?;

    let model = match &step.object {
        Some(e_step::Object::Datastep(ds)) => ds.model.as_ref(),
        Some(e_step::Object::Crosstabstep(cs)) => cs.model.as_ref(),
        _ => None,
    }?;

    let width = model.column_table.as_ref().map(|ct| {
        let cell_size = ct.cell_size as i32;
        let n_rows = ct.n_rows.max(1);
        cell_size * n_rows
    })?;

    let height = model.row_table.as_ref().map(|rt| {
        let cell_size = rt.cell_size as i32;
        let n_rows = rt.n_rows.max(1);
        cell_size * n_rows
    })?;

    if width > 0 && height > 0 {
        Some((width, height))
    } else {
        None
    }
}

/// Fetch workflow by ID
pub async fn fetch_workflow(
    client: &TercenClient,
    workflow_id: &str,
) -> Result<Workflow, Box<dyn std::error::Error>> {
    use crate::tercen::client::proto::{e_workflow, GetRequest};

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

    Ok(workflow)
}
