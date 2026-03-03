use crate::common::config::ClickHouseConfig;
use crate::util::sequence_id::SequenceId;
use clickhouse::Client;
use serde::Serialize;
use std::sync::Arc;

/// Query log entry for storing in ClickHouse
#[derive(Debug, Clone, Serialize, clickhouse::Row)]
pub struct QueryLog {
    pub id: u64,
    pub key_id: String,
    pub model: String,
    pub content: String,
    pub request_payload: String,
    pub response_payload: String,
    pub input_token: u64,
    pub output_token: u64,
    pub cache_token: u64,
}

/// Query log entry with id for ClickHouse insert
#[derive(Debug, Clone, Serialize, clickhouse::Row)]
struct QueryLogWithId {
    id: u64,
    key_id: String,
    model: String,
    content: String,
    request_payload: String,
    response_payload: String,
    input_token: u64,
    output_token: u64,
    cache_token: u64,
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

    /// Inserts a query log entry using insert API
    pub async fn insert_log(&self, log: &QueryLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // * generate new ID using SequenceId
        let new_id = self.sequence_id.next_id();

        // * create a new QueryLog with the calculated id
        let log_with_id = QueryLogWithId {
            id: new_id,
            key_id: log.key_id.clone(),
            model: log.model.clone(),
            content: log.content.clone(),
            request_payload: log.request_payload.clone(),
            response_payload: log.response_payload.clone(),
            input_token: log.input_token,
            output_token: log.output_token,
            cache_token: log.cache_token,
        };

        let mut insert = self.client.insert("query_log")?;
        insert.write(&log_with_id).await?;
        insert.end().await?;

        Ok(())
    }
}
