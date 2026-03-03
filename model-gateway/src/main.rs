mod common;
mod database;
mod handler;
mod util;

use common::config::init as init_config;
use common::server::ServerConfig;
use handler::handler::State;

use sea_orm::DatabaseConnection;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // * load config
    let config = init_config();
    println!("Listen on: {}", config.listen);

    // * initialize postgres if configured
    let postgres: Option<DatabaseConnection> = if let Some(pg_config) = &config.postgres {
        println!("Postgres: {}", pg_config.dsn);
        Some(common::postgres::init(pg_config).await)
    } else {
        None
    };

    // * initialize clickhouse if configured
    let clickhouse = if let Some(ch_config) = &config.clickhouse {
        println!(
            "ClickHouse: {} (database: {})",
            ch_config.url, ch_config.database
        );
        let client = common::clickhouse::init(ch_config);
        database::clickhouse::migration::Migrator::up(&client).await
            .expect("failed to run clickhouse migrations");
        Some(Arc::new(Mutex::new(client)))
    } else {
        None
    };

    let state = State {
        config: config.clone(),
        postgres,
        clickhouse,
    };

    // * start server
    ServerConfig::new(config.listen).run(state).await
}
