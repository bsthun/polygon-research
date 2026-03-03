use crate::common::clickhouse::ClickHouseClient;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct QueryLog {
    pub id: u64,
    pub key_id: String,
    pub model: String,
    pub content: String,
    pub request_payload: serde_json::Value,
    pub response_payload: serde_json::Value,
    pub duration_first_token: u64,
    pub duration_completed: u64,
    pub input_token: u64,
    pub output_token: u64,
    pub cache_token: u64,
}

/// Inserts a single log entry
pub async fn insert_log(client: &ClickHouseClient, log: &QueryLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // generate new ID using SequenceId
    let new_id = client.sequence_id.next_id();

    // serialize JSON values to strings
    let request_json = serde_json::to_string(&log.request_payload).unwrap_or_default();
    let response_json = serde_json::to_string(&log.response_payload).unwrap_or_default();

    client.client
        .query("INSERT INTO query_log VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(new_id)
        .bind(log.key_id.as_str())
        .bind(log.model.as_str())
        .bind(log.content.as_str())
        .bind(request_json.as_str())
        .bind(response_json.as_str())
        .bind(log.duration_first_token)
        .bind(log.duration_completed)
        .bind(log.input_token)
        .bind(log.output_token)
        .bind(log.cache_token)
        .execute()
        .await?;

    Ok(())
}
