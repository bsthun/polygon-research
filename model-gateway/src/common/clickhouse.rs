use crate::common::config::ClickHouseConfig;
use crate::util::sequence_id::SequenceId;
use clickhouse::Client;
use serde::Serialize;
use std::sync::Arc;

/// Query log entry for storing in ClickHouse
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

/// ClickHouse client wrapper
pub struct ClickHouseClient {
    client: Client,
    sequence_id: Arc<SequenceId>,
}

impl ClickHouseClient {
    /// Creates a new ClickHouse client from config
    pub fn new(config: &ClickHouseConfig) -> Self {
        let client = Client::default()
            .with_url(&config.url)
            .with_database(&config.database)
            .with_user(&config.username)
            .with_password(&config.password);

        let sequence_id = Arc::new(SequenceId::with_node_id(config.node_id));

        Self { client, sequence_id }
    }

    /// Initializes the query_log table if it doesn't exist
    pub async fn init_table(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client
            .query(
                r#"
                CREATE TABLE IF NOT EXISTS query_log (
                    id UInt64,
                    key_id String,
                    model String,
                    content String,
                    request_payload String,
                    response_payload String,
                    duration_first_token UInt64,
                    duration_completed UInt64,
                    input_token UInt64,
                    output_token UInt64,
                    cache_token UInt64
                ) ENGINE = MergeTree()
                ORDER BY id
                "#,
            )
            .execute()
            .await?;

        Ok(())
    }

    /// Inserts a query log entry using query API
    pub async fn insert_log(&self, log: &QueryLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // * generate new ID using SequenceId
        let new_id = self.sequence_id.next_id();

        // * serialize JSON values to strings
        let request_json = serde_json::to_string(&log.request_payload).unwrap_or_default();
        let response_json = serde_json::to_string(&log.response_payload).unwrap_or_default();

        self.client
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
}
