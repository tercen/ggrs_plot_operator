#![allow(dead_code)]
use super::client::proto::ReqStreamTable;
use super::client::TercenClient;
use super::error::{Result, TercenError};
use tokio_stream::StreamExt;

/// Tercen table data streamer
pub struct TableStreamer<'a> {
    client: &'a TercenClient,
}

impl<'a> TableStreamer<'a> {
    /// Create a new table streamer
    pub fn new(client: &'a TercenClient) -> Self {
        TableStreamer { client }
    }

    /// Stream table data as TSON format
    ///
    /// # Arguments
    /// * `table_id` - The Tercen table ID to stream
    /// * `columns` - Optional list of columns to fetch (None = all columns)
    /// * `offset` - Starting row offset
    /// * `limit` - Maximum number of rows to fetch
    ///
    /// # Returns
    /// Vector of TSON binary data chunks as bytes
    /// Get the schema for a table to retrieve metadata like row count
    pub async fn get_schema(&self, table_id: &str) -> Result<super::client::proto::ESchema> {
        use super::client::proto::GetRequest;

        let mut table_service = self.client.table_service()?;
        let request = tonic::Request::new(GetRequest {
            id: table_id.to_string(),
            ..Default::default()
        });

        let response = table_service
            .get(request)
            .await
            .map_err(|e| TercenError::Grpc(Box::new(e)))?;

        Ok(response.into_inner())
    }

    pub async fn stream_tson(
        &self,
        table_id: &str,
        columns: Option<Vec<String>>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<u8>> {
        let mut table_service = self.client.table_service()?;

        let request = tonic::Request::new(ReqStreamTable {
            table_id: table_id.to_string(),
            cnames: columns.unwrap_or_default(),
            offset,
            limit,
            binary_format: String::new(), // Empty = TSON format (default)
        });

        let mut stream = table_service
            .stream_table(request)
            .await
            .map_err(|e| TercenError::Grpc(Box::new(e)))?
            .into_inner();

        let mut all_data = Vec::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    all_data.extend_from_slice(&chunk.result);
                }
                Err(e) => return Err(TercenError::Grpc(Box::new(e))),
            }
        }

        Ok(all_data)
    }

    /// Stream entire table in chunks, calling callback for each chunk
    ///
    /// # Arguments
    /// * `table_id` - The Tercen table ID to stream
    /// * `columns` - Optional list of columns to fetch
    /// * `chunk_size` - Number of rows per chunk
    /// * `callback` - Function to call with each TSON chunk
    pub async fn stream_table_chunked<F>(
        &self,
        table_id: &str,
        columns: Option<Vec<String>>,
        chunk_size: i64,
        mut callback: F,
    ) -> Result<()>
    where
        F: FnMut(Vec<u8>) -> Result<()>,
    {
        let mut offset = 0;

        loop {
            let chunk = self
                .stream_tson(table_id, columns.clone(), offset, chunk_size)
                .await?;

            if chunk.is_empty() {
                break;
            }

            callback(chunk)?;

            offset += chunk_size;
        }

        Ok(())
    }
}
