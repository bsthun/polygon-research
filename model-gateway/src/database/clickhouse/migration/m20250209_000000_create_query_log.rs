use crate::common::clickhouse::ClickHouseClient;

#[async_trait::async_trait]
pub trait MigrationTrait {
    async fn up(&self, client: &ClickHouseClient) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    #[allow(dead_code)]
    async fn down(&self, client: &ClickHouseClient) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, client: &ClickHouseClient) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        client.client
            .query(
                r#"
                CREATE TABLE IF NOT EXISTS query_logs (
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
                "#
            )
            .execute()
            .await?;

        // Create migrations table
        client.client
            .query(
                r#"
                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version String,
                    applied_at DateTime DEFAULT now()
                ) ENGINE = MergeTree()
                ORDER BY version
                "#
            )
            .execute()
            .await?;

        Ok(())
    }

    async fn down(&self, client: &ClickHouseClient) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        client.client
            .query("DROP TABLE IF EXISTS query_logs")
            .execute()
            .await?;
        Ok(())
    }
}
