use crate::common::clickhouse::ClickHouseClient;
use clickhouse::Client;

mod m20250209_000000_create_query_log;

use m20250209_000000_create_query_log::{Migration, MigrationTrait};

pub struct Migrator;

impl Migrator {
    /// Runs pending migrations
    pub async fn up(client: &ClickHouseClient) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // * create database using `default` context so we don't fail if it doesn't exist yet
        let bootstrap = Client::default()
            .with_url(&client.url)
            .with_user(&client.username)
            .with_password(&client.password);

        bootstrap
            .query(&format!("CREATE DATABASE IF NOT EXISTS `{}`", client.database))
            .execute()
            .await?;

        // Create migrations table first if not exists
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

        // Check if already applied
        let version = "m20250209_000000_create_query_log";
        let already_applied: u64 = client.client
            .query(&format!("SELECT count() FROM schema_migrations WHERE version = '{}'", version))
            .fetch_one()
            .await?;

        if already_applied == 0 {
            Migration.up(client).await?;

            // Record migration
            client.client
                .query(&format!("INSERT INTO schema_migrations (version) VALUES ('{}')", version))
                .execute()
                .await?;

            println!("Applied migration: {}", version);
        }

        Ok(())
    }
}
