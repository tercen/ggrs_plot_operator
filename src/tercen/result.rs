//! Result upload module for saving operator results back to Tercen
//!
//! This module handles the complete flow of saving a generated PNG plot
//! back to Tercen so it can be displayed in the workflow UI.
//!
//! Flow:
//! 1. PNG bytes → Base64 string
//! 2. Create DataFrame with .content, filename, mimetype columns
//! 3. Convert DataFrame → Tercen Table (with TSON encoding)
//! 4. Wrap Table in OperatorResult
//! 5. Serialize OperatorResult to TSON bytes
//! 6. Upload via FileService
//! 7. Update task with fileResultId
//! 8. Wait for completion

use super::client::proto;
use super::client::TercenClient;
use super::table_convert;
use polars::prelude::*;
use std::sync::Arc;

/// Save a PNG plot result back to Tercen
///
/// Takes the generated PNG buffer, converts it to Tercen's result format,
/// and uploads it via the FileService.
///
/// # Arguments
/// * `client` - Tercen client for gRPC calls
/// * `project_id` - Project ID to upload the result to
/// * `png_buffer` - Raw PNG bytes from the renderer
///
/// # Returns
/// Result indicating success or error during upload
pub async fn save_result(
    client: Arc<TercenClient>,
    project_id: &str,
    png_buffer: Vec<u8>,
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

    // 2. Create result DataFrame
    println!("Creating result DataFrame...");
    let result_df = create_result_dataframe(base64_png)?;
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

    // 4. Wrap in OperatorResult
    println!("Creating OperatorResult...");
    let operator_result = create_operator_result(table)?;

    // 5. Serialize OperatorResult to JSON then TSON
    println!("Serializing to TSON...");
    let result_bytes = serialize_operator_result(&operator_result)?;
    println!("  TSON size: {} bytes", result_bytes.len());

    // 6. Create FileDocument
    println!("Creating FileDocument...");
    let file_doc = create_file_document(project_id, result_bytes.len() as i32);

    // 7. Upload via FileService
    println!("Uploading result...");
    let uploaded_doc = upload_result(&client, file_doc, result_bytes).await?;
    println!("  Uploaded with ID: {}", uploaded_doc.id);

    println!("Result saved successfully!");
    Ok(())
}

/// Create a result DataFrame with base64-encoded PNG
///
/// Creates a single-row DataFrame with columns:
/// - .content: Base64-encoded PNG bytes
/// - filename: "plot.png"
/// - mimetype: "image/png"
fn create_result_dataframe(png_base64: String) -> Result<DataFrame, Box<dyn std::error::Error>> {
    let df = df! {
        ".content" => [png_base64],
        "filename" => ["plot.png"],
        "mimetype" => ["image/png"]
    }?;

    Ok(df)
}

/// Convert DataFrame to Tercen Table with TSON encoding
///
/// This is delegated to the table_convert module
fn dataframe_to_table(df: &DataFrame) -> Result<proto::Table, Box<dyn std::error::Error>> {
    table_convert::dataframe_to_table(df)
}

/// Wrap Table in OperatorResult structure
fn create_operator_result(
    table: proto::Table,
) -> Result<proto::OperatorResult, Box<dyn std::error::Error>> {
    let result = proto::OperatorResult {
        tables: vec![table],
        join_operators: vec![],
    };
    Ok(result)
}

/// Serialize OperatorResult to TSON binary format
fn serialize_operator_result(
    result: &proto::OperatorResult,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // For now, we need to convert OperatorResult to TSON
    // The challenge is that OperatorResult is a proto message, not a TSON value
    // We'll need to manually construct the TSON structure

    use rustson::Value as TsonValue;
    use std::collections::HashMap;

    // Convert Table to TSON structure
    let mut tson_tables = Vec::new();

    for table in &result.tables {
        let mut table_map = HashMap::new();

        // Add nRows
        table_map.insert("nRows".to_string(), TsonValue::I32(table.n_rows));

        // Add columns
        let mut cols_list = Vec::new();
        for col in &table.columns {
            let mut col_map = HashMap::new();
            col_map.insert("name".to_string(), TsonValue::STR(col.name.clone()));
            col_map.insert("type".to_string(), TsonValue::STR(col.r#type.clone()));

            // Decode the TSON bytes back to get the data
            let col_data = rustson::decode_bytes(&col.values)
                .map_err(|e| format!("Failed to decode column values: {:?}", e))?;
            col_map.insert("data".to_string(), col_data);

            cols_list.push(TsonValue::MAP(col_map));
        }

        table_map.insert("cols".to_string(), TsonValue::LST(cols_list));
        tson_tables.push(TsonValue::MAP(table_map));
    }

    // Create top-level structure
    let mut result_map = HashMap::new();
    result_map.insert("tables".to_string(), TsonValue::LST(tson_tables));

    let tson_value = TsonValue::MAP(result_map);

    // Encode to TSON bytes
    let bytes = rustson::encode(&tson_value)
        .map_err(|e| format!("Failed to encode OperatorResult to TSON: {:?}", e))?;

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

/// Upload result via FileService
async fn upload_result(
    client: &TercenClient,
    file_doc: proto::FileDocument,
    result_bytes: Vec<u8>,
) -> Result<proto::FileDocument, Box<dyn std::error::Error>> {
    use futures::stream;

    let mut file_service = client.file_service()?;

    // Create EFileDocument wrapper
    let e_file_doc = proto::EFileDocument {
        object: Some(proto::e_file_document::Object::Filedocument(file_doc)),
    };

    // Create upload request stream
    // First message: file metadata
    let first_msg = proto::ReqUpload {
        file: Some(e_file_doc),
        bytes: vec![],
    };

    // Second message: data bytes
    let second_msg = proto::ReqUpload {
        file: None,
        bytes: result_bytes,
    };

    let request_stream = stream::iter(vec![first_msg, second_msg]);

    // Send streaming request
    let response = file_service.upload(request_stream).await?;
    let resp_upload = response.into_inner();

    // Extract FileDocument from response
    let uploaded_doc = resp_upload
        .result
        .and_then(|e| match e.object {
            Some(proto::e_file_document::Object::Filedocument(doc)) => Some(doc),
            _ => None,
        })
        .ok_or("Upload response missing FileDocument")?;

    Ok(uploaded_doc)
}
