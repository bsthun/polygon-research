use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use crate::common::config::PostgresConfig;
use crate::database::postgres::migration::Migrator;

/// Connects to postgres and runs pending migrations
pub async fn init(config: &PostgresConfig) -> DatabaseConnection {
    let db = Database::connect(&config.dsn)
        .await
        .expect("unable to connect to postgres database");

    Migrator::up(&db, None)
        .await
        .expect("failed to run postgres migrations");

    db
}
