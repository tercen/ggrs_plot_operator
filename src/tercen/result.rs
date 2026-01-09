//! Result upload module for saving operator results back to Tercen
//!
//! This module handles the complete flow of saving a generated PNG plot
//! back to Tercen so it can be displayed in the workflow UI.
//!
//! Flow (following Python client pattern):
//! 1. PNG bytes → Base64 string
//! 2. Create DataFrame with .content, filename, mimetype columns
//! 3. Convert DataFrame → Tercen Table (with TSON encoding)
//! 4. Serialize to Sarno-compatible TSON format
//! 5. Upload via TableSchemaService.uploadTable()
//! 6. Create NEW RunComputationTask with fileResultId and original query
//! 7. Submit task via TaskService.create()
//! 8. Run task via TaskService.runTask()
//! 9. Wait for completion via TaskService.waitDone()
//! 10. Server automatically creates computedRelation linking result to step

use super::client::proto;
use super::client::TercenClient;
use super::table_convert;
use polars::prelude::*;
use std::sync::Arc;

/// Save a PNG plot result back to Tercen
///
/// Takes the generated PNG buffer, converts it to Tercen's result format,
/// uploads it, creates a new task to process the result, and waits for the
/// server to link the result to the workflow step.
///
/// This follows the Python client pattern:
/// 1. Upload result table via uploadTable()
/// 2. Create a NEW RunComputationTask with the fileResultId
/// 3. Run the task via runTask()
/// 4. Wait for the task to complete via waitDone()
/// 5. The server automatically creates the computedRelation linking the result
///
/// # Arguments
/// * `client` - Tercen client for gRPC calls
/// * `project_id` - Project ID to upload the result to
/// * `namespace` - Operator namespace for prefixing column names
/// * `png_buffer` - Raw PNG bytes from the renderer
/// * `plot_width` - Width of the plot in pixels
/// * `plot_height` - Height of the plot in pixels
/// * `original_task` - The original task from the operator (for getting cubeQuery, owner, etc.)
///
/// # Returns
/// Result indicating success or error during upload
pub async fn save_result(
    client: Arc<TercenClient>,
    project_id: &str,
    namespace: &str,
    png_buffer: Vec<u8>,
    plot_width: i32,
    plot_height: i32,
    original_task: &proto::ETask,
) -> Result<(), Box<dyn std::error::Error>> {
    use base64::Engine;

    println!("Encoding PNG to base64...");
    // 1. Encode PNG to base64
    let base64_png = base64::engine::general_purpose::STANDARD.encode(&png_buffer);
    println!(
        "  PNG size: {} bytes, base64 size: {} bytes",
        png_buffer.len(),
        base64_png.len()
    );

    // 2. Create result DataFrame with namespace-prefixed columns
    println!("Creating result DataFrame...");
    let result_df = create_result_dataframe(base64_png, namespace, plot_width, plot_height)?;
    println!(
        "  DataFrame: {} rows, {} columns",
        result_df.height(),
        result_df.width()
    );

    // 3. Convert to Table
    println!("Converting DataFrame to Table...");
    let table = dataframe_to_table(&result_df)?;
    println!(
        "  Table: {} rows, {} columns",
        table.n_rows,
        table.columns.len()
    );

    // 4. Serialize table to Sarno format (simple {"cols": [...]})
    println!("Serializing to Sarno TSON format...");
    let result_bytes = serialize_table_for_sarno(&table)?;
    println!("  TSON size: {} bytes", result_bytes.len());

    // 5. Create FileDocument
    println!("Creating FileDocument...");
    let file_doc = create_file_document(project_id, result_bytes.len() as i32);

    // 6. Upload via TableSchemaService.uploadTable()
    println!("Uploading result table...");
    let schema_id = upload_result_table(&client, file_doc, result_bytes).await?;
    println!("  Uploaded table with schema ID: {}", schema_id);

    // 7. Create a NEW task with the result (this is the Python pattern)
    println!("Creating new task to link result to workflow step...");
    let new_task = create_result_task(original_task, &schema_id, project_id)?;

    // 8. Create the task via TaskService
    println!("Submitting task to server...");
    let mut task_service = client.task_service()?;
    let created_task = task_service.create(new_task).await?.into_inner();
    let task_id = extract_task_id(&created_task)?;
    println!("  Created task with ID: {}", task_id);

    // 9. Run the task
    println!("Running task...");
    let run_req = proto::ReqRunTask {
        task_id: task_id.clone(),
    };
    task_service.run_task(run_req).await?;
    println!("  Task started");

    // 10. Wait for task completion
    println!("Waiting for task to complete...");
    let wait_req = proto::ReqWaitDone { task_id };
    let wait_response = task_service.wait_done(wait_req).await?.into_inner();
    let completed_task = wait_response.result.ok_or("Wait response missing task")?;
    println!("  Task completed");

    // 11. Check if task failed
    check_task_state(&completed_task)?;

    println!("Result saved and linked successfully!");
    Ok(())
}

/// Create a result DataFrame with base64-encoded PNG
///
/// Creates a single-row DataFrame with columns matching R plot_operator output:
/// - .content: Base64-encoded PNG bytes
/// - {namespace}.filename: "plot.png" (namespace-prefixed by operator)
/// - {namespace}.mimetype: "image/png" (namespace-prefixed by operator)
/// - {namespace}.plot_width: plot width in pixels (namespace-prefixed by operator)
/// - {namespace}.plot_height: plot height in pixels (namespace-prefixed by operator)
///
/// Note: Only .content has a leading dot. Other columns get namespace prefix from operator.
fn create_result_dataframe(
    png_base64: String,
    namespace: &str,
    plot_width: i32,
    plot_height: i32,
) -> Result<DataFrame, Box<dyn std::error::Error>> {
    let df = df! {
        ".content" => [png_base64],
        &format!("{}.filename", namespace) => ["plot.png"],
        &format!("{}.mimetype", namespace) => ["image/png"],
        &format!("{}.plot_width", namespace) => [plot_width as f64],
        &format!("{}.plot_height", namespace) => [plot_height as f64]
    }?;

    Ok(df)
}

/// Convert DataFrame to Tercen Table with TSON encoding
///
/// This is delegated to the table_convert module
fn dataframe_to_table(df: &DataFrame) -> Result<proto::Table, Box<dyn std::error::Error>> {
    table_convert::dataframe_to_table(df)
}

/// Serialize Table to Sarno-compatible TSON format
///
/// Sarno expects a simple structure:
/// ```json
/// {
///   "cols": [
///     {"name": "column_name", "type": <tson_type_int>, "data": [values...]}
///   ]
/// }
/// ```
///
/// Where type is a TSON type integer (from TsonSpec constants):
/// - 105 (LIST_INT32_TYPE) for int32
/// - 106 (LIST_INT64_TYPE) for int64
/// - 111 (LIST_FLOAT64_TYPE) for double/float64
/// - 112 (LIST_STRING_TYPE) for string
fn serialize_table_for_sarno(table: &proto::Table) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use rustson::Value as TsonValue;
    use std::collections::HashMap;

    // Build simple Sarno table structure: {"cols": [...]}
    let mut sarno_table = HashMap::new();
    let mut cols_list = Vec::new();

    for col in &table.columns {
        let mut col_map = HashMap::new();

        // Add column name
        col_map.insert("name".to_string(), TsonValue::STR(col.name.clone()));

        // Map Tercen type string to TSON type integer (from TsonSpec constants)
        let tson_type = match col.r#type.as_str() {
            "int32" => 105,  // LIST_INT32_TYPE
            "int64" => 106,  // LIST_INT64_TYPE
            "double" => 111, // LIST_FLOAT64_TYPE
            "string" => 112, // LIST_STRING_TYPE
            _ => return Err(format!("Unsupported column type: {}", col.r#type).into()),
        };
        col_map.insert("type".to_string(), TsonValue::I32(tson_type));

        // Decode the TSON-encoded column values to get the data array
        let col_data = rustson::decode_bytes(&col.values)
            .map_err(|e| format!("Failed to decode column values for '{}': {:?}", col.name, e))?;
        col_map.insert("data".to_string(), col_data);

        cols_list.push(TsonValue::MAP(col_map));
    }

    sarno_table.insert("cols".to_string(), TsonValue::LST(cols_list));
    let tson_value = TsonValue::MAP(sarno_table);

    // Encode to TSON bytes
    let bytes = rustson::encode(&tson_value)
        .map_err(|e| format!("Failed to encode table to TSON: {:?}", e))?;

    Ok(bytes)
}

/// Create FileDocument for result upload
fn create_file_document(project_id: &str, size: i32) -> proto::FileDocument {
    // Set file metadata
    let file_metadata = proto::FileMetadata {
        content_type: "application/octet-stream".to_string(),
        ..Default::default()
    };

    let e_metadata = proto::EFileMetadata {
        object: Some(proto::e_file_metadata::Object::Filemetadata(file_metadata)),
    };

    // Note: ACL will be assigned by the server based on projectId
    proto::FileDocument {
        name: "result".to_string(),
        project_id: project_id.to_string(),
        size,
        metadata: Some(e_metadata),
        ..Default::default()
    }
}

/// Upload result table via TableSchemaService.uploadTable()
///
/// This is the correct method for uploading operator results.
/// FileService.upload() is for regular files, but operator results
/// must use TableSchemaService.uploadTable() to properly set the dataUri.
async fn upload_result_table(
    client: &TercenClient,
    file_doc: proto::FileDocument,
    result_bytes: Vec<u8>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut table_service = client.table_service()?;

    // Create EFileDocument wrapper
    let e_file_doc = proto::EFileDocument {
        object: Some(proto::e_file_document::Object::Filedocument(file_doc)),
    };

    // Create upload request (single message in a stream)
    let request = proto::ReqUploadTable {
        file: Some(e_file_doc),
        bytes: result_bytes,
    };

    // Wrap in stream (even though it's just one message)
    use futures::stream;
    let request_stream = stream::iter(vec![request]);

    // Send request
    let response = table_service.upload_table(request_stream).await?;
    let resp_upload = response.into_inner();

    // Extract schema ID from response
    let e_schema = resp_upload.result.ok_or("Upload response missing schema")?;

    // Extract the actual schema from the wrapper
    use proto::e_schema;
    let schema_obj = e_schema.object.ok_or("ESchema has no object")?;

    let schema_id = match schema_obj {
        e_schema::Object::Tableschema(ts) => ts.id,
        e_schema::Object::Computedtableschema(cts) => cts.id,
        e_schema::Object::Cubequerytableschema(cqts) => cqts.id,
        e_schema::Object::Schema(s) => s.id,
    };

    Ok(schema_id)
}

/// Create a new RunComputationTask to link the result to the workflow step
///
/// This follows the Python client pattern where a NEW task is created (not modified)
/// with the fileResultId set, and the server automatically creates the computedRelation.
///
/// The new task copies key fields from the original task:
/// - query (CubeQuery) - CRITICAL: Same query that generated the data
/// - projectId
/// - owner
/// - state = InitState (ready to run)
fn create_result_task(
    original_task: &proto::ETask,
    schema_id: &str,
    project_id: &str,
) -> Result<proto::ETask, Box<dyn std::error::Error>> {
    use proto::e_task;

    // Extract query and owner from original task
    let original_task_obj = original_task
        .object
        .as_ref()
        .ok_or("Original task has no object field")?;

    let (query, owner, acl_context) = match original_task_obj {
        e_task::Object::Runcomputationtask(rct) => {
            let query = rct
                .query
                .clone()
                .ok_or("Original task has no query field")?;
            (query, rct.owner.clone(), rct.acl_context.clone())
        }
        _ => {
            return Err("Original task must be RunComputationTask".into());
        }
    };

    // Create InitState
    let init_state = proto::InitState::default();
    let e_state = proto::EState {
        object: Some(proto::e_state::Object::Initstate(init_state)),
    };

    // Create new RunComputationTask with all required fields
    let new_rct = proto::RunComputationTask {
        file_result_id: schema_id.to_string(),
        query: Some(query),
        state: Some(e_state),
        project_id: project_id.to_string(),
        owner,
        acl_context,
        ..Default::default()
    };

    let new_e_task = proto::ETask {
        object: Some(e_task::Object::Runcomputationtask(new_rct)),
    };

    Ok(new_e_task)
}

/// Extract task ID from ETask
fn extract_task_id(task: &proto::ETask) -> Result<String, Box<dyn std::error::Error>> {
    use proto::e_task;

    let task_obj = task.object.as_ref().ok_or("Task has no object field")?;

    let task_id = match task_obj {
        e_task::Object::Runcomputationtask(rct) => rct.id.clone(),
        e_task::Object::Computationtask(ct) => ct.id.clone(),
        _ => return Err("Unexpected task type".into()),
    };

    if task_id.is_empty() {
        return Err("Task ID is empty".into());
    }

    Ok(task_id)
}

/// Check if task failed and return error if so
fn check_task_state(task: &proto::ETask) -> Result<(), Box<dyn std::error::Error>> {
    use proto::{e_state, e_task};

    let task_obj = task.object.as_ref().ok_or("Task has no object")?;

    let state = match task_obj {
        e_task::Object::Runcomputationtask(rct) => rct.state.as_ref(),
        e_task::Object::Computationtask(ct) => ct.state.as_ref(),
        _ => return Err("Unexpected task type".into()),
    };

    let state = state.ok_or("Task has no state")?;
    let state_obj = state.object.as_ref().ok_or("State has no object")?;

    match state_obj {
        e_state::Object::Failedstate(failed) => {
            Err(format!("Task failed: {}", failed.reason).into())
        }
        e_state::Object::Donestate(_) => Ok(()),
        _ => Ok(()), // Other states (shouldn't happen after waitDone, but be safe)
    }
}
