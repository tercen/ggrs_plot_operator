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
/// uploads it, updates the existing task with the fileResultId, and waits
/// for the server to process the result and link it to the workflow step.
///
/// This follows the Python production client pattern (OperatorContext.save):
/// 1. Upload file via FileService.upload() → get FileDocument ID
/// 2. Update EXISTING task's fileResultId field
/// 3. Call TaskService.update() to save the updated task
/// 4. Call TaskService.waitDone() to wait for server processing
/// 5. Server (Sarno) processes file and creates computedRelation automatically
///
/// # Arguments
/// * `client` - Tercen client for gRPC calls
/// * `project_id` - Project ID to upload the result to
/// * `namespace` - Operator namespace for prefixing column names
/// * `png_buffer` - Raw PNG bytes from the renderer
/// * `plot_width` - Width of the plot in pixels
/// * `plot_height` - Height of the plot in pixels
/// * `task` - Mutable reference to the task (will be updated with fileResultId)
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
    task: &mut proto::ETask,
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

    // 6. Check if task already has a fileResultId (normal operator flow)
    let existing_file_result_id = get_task_file_result_id(task)?;

    if existing_file_result_id.is_empty() {
        // Webapp/test scenario: Create new file and update task
        println!("Uploading result file (webapp scenario)...");
        let file_doc_id = upload_result_file(&client, file_doc, result_bytes).await?;
        println!("  Uploaded file with ID: {}", file_doc_id);

        println!("Updating task with fileResultId...");
        update_task_file_result_id(task, &file_doc_id)?;
        println!("  Task fileResultId set to: {}", file_doc_id);

        println!("Saving updated task...");
        let mut task_service = client.task_service()?;
        let update_response = task_service.update(task.clone()).await?;
        let _updated_task = update_response.into_inner();
        println!("  Task updated");

        // Note: Python calls waitDone() here in webapp scenario
        // We should exit cleanly and let the task runner process the result
        println!("Result uploaded - exiting for server to process");
    } else {
        // Normal operator scenario: Upload to existing file
        println!(
            "Uploading to existing result file: {}",
            existing_file_result_id
        );

        // Get the existing FileDocument
        let mut file_service = client.file_service()?;
        let get_req = proto::GetRequest {
            id: existing_file_result_id.clone(),
            ..Default::default()
        };
        let e_file_doc = file_service.get(get_req).await?.into_inner();

        // Extract FileDocument
        use proto::e_file_document;
        let file_doc_obj = e_file_doc.object.ok_or("EFileDocument has no object")?;
        let e_file_document::Object::Filedocument(file_doc) = file_doc_obj;

        // Upload to existing file (overwrites content)
        upload_result_file(&client, file_doc, result_bytes).await?;
        println!("  Uploaded to existing file");

        // No update(), no waitDone() - just exit cleanly
        println!("Result uploaded - exiting normally");
    }

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

/// Upload result file via FileService.upload()
///
/// This uploads an OperatorResult (TSON-encoded table) as a FileDocument.
/// The returned FileDocument ID is what goes into task.fileResultId.
/// The server (Sarno) will then process this file to create the actual schemas
/// and computedRelation.
///
/// Note: We use FileService.upload() (NOT TableSchemaService.uploadTable())
/// because we need a FileDocument with a dataUri, not just a Schema.
async fn upload_result_file(
    client: &TercenClient,
    file_doc: proto::FileDocument,
    result_bytes: Vec<u8>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut file_service = client.file_service()?;

    // Create EFileDocument wrapper
    let e_file_doc = proto::EFileDocument {
        object: Some(proto::e_file_document::Object::Filedocument(file_doc)),
    };

    // Create upload request (single message in a stream)
    let request = proto::ReqUpload {
        file: Some(e_file_doc),
        bytes: result_bytes,
    };

    // Wrap in stream (even though it's just one message)
    use futures::stream;
    let request_stream = stream::iter(vec![request]);

    // Send request
    let response = file_service.upload(request_stream).await?;
    let resp_upload = response.into_inner();

    // Extract FileDocument ID from response
    let e_file_doc = resp_upload
        .result
        .ok_or("Upload response missing file document")?;

    // Extract the actual FileDocument from the wrapper
    use proto::e_file_document;
    let file_doc_obj = e_file_doc.object.ok_or("EFileDocument has no object")?;

    // EFileDocument only has one variant: filedocument
    let file_doc_id = match file_doc_obj {
        e_file_document::Object::Filedocument(fd) => fd.id,
    };

    Ok(file_doc_id)
}

/// Get the task's fileResultId if it exists
///
/// Returns empty string if fileResultId is not set.
fn get_task_file_result_id(task: &proto::ETask) -> Result<String, Box<dyn std::error::Error>> {
    use proto::e_task;

    let task_obj = task.object.as_ref().ok_or("Task has no object field")?;

    match task_obj {
        e_task::Object::Runcomputationtask(rct) => Ok(rct.file_result_id.clone()),
        _ => Err("Expected RunComputationTask".into()),
    }
}

/// Update the task's fileResultId field
///
/// This updates the EXISTING task (following Python OperatorContext pattern).
/// The server will process the file and create the computedRelation automatically.
fn update_task_file_result_id(
    task: &mut proto::ETask,
    file_doc_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use proto::e_task;

    let task_obj = task.object.as_mut().ok_or("Task has no object field")?;

    match task_obj {
        e_task::Object::Runcomputationtask(rct) => {
            rct.file_result_id = file_doc_id.to_string();
            Ok(())
        }
        _ => Err("Expected RunComputationTask".into()),
    }
}
