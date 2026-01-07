//! Arrow to GGRS DataFrame conversion
//!
//! Converts Arrow IPC format (from Tercen) directly to GGRS DataFrame

use super::error::{Result, TercenError};
use arrow::array::*;
use arrow::ipc::reader::StreamReader;
use arrow::record_batch::RecordBatch;
use ggrs_core::data::{DataFrame, Record, Value};
use std::collections::HashMap;

/// Convert Arrow IPC bytes to GGRS DataFrame
pub fn arrow_to_dataframe(arrow_bytes: &[u8]) -> Result<DataFrame> {
    if arrow_bytes.is_empty() {
        return Ok(DataFrame::new());
    }

    // Debug: show first few bytes
    eprintln!("Arrow data: {} bytes, first 32 bytes: {:?}",
             arrow_bytes.len(),
             &arrow_bytes[..std::cmp::min(32, arrow_bytes.len())]);

    // Parse Arrow IPC stream
    let cursor = std::io::Cursor::new(arrow_bytes);
    let reader = StreamReader::try_new(cursor, None)
        .map_err(|e| TercenError::Other(format!("Failed to parse Arrow IPC: {}", e)))?;

    let mut all_records = Vec::new();

    // Process each RecordBatch
    for batch_result in reader {
        let batch = batch_result
            .map_err(|e| TercenError::Other(format!("Failed to read Arrow batch: {}", e)))?;

        let records = record_batch_to_records(&batch)?;
        all_records.extend(records);
    }

    // Convert to DataFrame
    DataFrame::from_records(all_records)
        .map_err(|e| TercenError::Other(format!("Failed to create DataFrame: {}", e)))
}

/// Convert a single Arrow RecordBatch to Vec<Record>
fn record_batch_to_records(batch: &RecordBatch) -> Result<Vec<Record>> {
    let schema = batch.schema();
    let num_rows = batch.num_rows();
    let num_cols = batch.num_columns();

    let mut records = Vec::with_capacity(num_rows);

    for row_idx in 0..num_rows {
        let mut record = HashMap::new();

        for col_idx in 0..num_cols {
            let field = schema.field(col_idx);
            let column_name = field.name().clone();
            let array = batch.column(col_idx);

            let value = array_value_to_ggrs(array.as_ref(), row_idx)?;
            record.insert(column_name, value);
        }

        records.push(record);
    }

    Ok(records)
}

/// Convert Arrow array value at index to GGRS Value
fn array_value_to_ggrs(array: &dyn Array, index: usize) -> Result<Value> {
    if array.is_null(index) {
        return Ok(Value::Null);
    }

    // Try different array types
    if let Some(arr) = array.as_any().downcast_ref::<Float64Array>() {
        return Ok(Value::Float(arr.value(index)));
    }

    if let Some(arr) = array.as_any().downcast_ref::<Float32Array>() {
        return Ok(Value::Float(arr.value(index) as f64));
    }

    if let Some(arr) = array.as_any().downcast_ref::<Int64Array>() {
        return Ok(Value::Int(arr.value(index)));
    }

    if let Some(arr) = array.as_any().downcast_ref::<Int32Array>() {
        return Ok(Value::Int(arr.value(index) as i64));
    }

    if let Some(arr) = array.as_any().downcast_ref::<Int16Array>() {
        return Ok(Value::Int(arr.value(index) as i64));
    }

    if let Some(arr) = array.as_any().downcast_ref::<Int8Array>() {
        return Ok(Value::Int(arr.value(index) as i64));
    }

    if let Some(arr) = array.as_any().downcast_ref::<UInt64Array>() {
        return Ok(Value::Int(arr.value(index) as i64));
    }

    if let Some(arr) = array.as_any().downcast_ref::<UInt32Array>() {
        return Ok(Value::Int(arr.value(index) as i64));
    }

    if let Some(arr) = array.as_any().downcast_ref::<UInt16Array>() {
        return Ok(Value::Int(arr.value(index) as i64));
    }

    if let Some(arr) = array.as_any().downcast_ref::<UInt8Array>() {
        return Ok(Value::Int(arr.value(index) as i64));
    }

    if let Some(arr) = array.as_any().downcast_ref::<BooleanArray>() {
        return Ok(Value::Bool(arr.value(index)));
    }

    if let Some(arr) = array.as_any().downcast_ref::<StringArray>() {
        return Ok(Value::String(arr.value(index).to_string()));
    }

    if let Some(arr) = array.as_any().downcast_ref::<LargeStringArray>() {
        return Ok(Value::String(arr.value(index).to_string()));
    }

    // Fallback: convert to string
    Err(TercenError::Other(format!(
        "Unsupported Arrow array type at index {}: {:?}",
        index,
        array.data_type()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_arrow() {
        let result = arrow_to_dataframe(&[]);
        assert!(result.is_ok());
        let df = result.unwrap();
        assert_eq!(df.nrow(), 0);
    }
}
