use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS keys (
                id         BIGSERIAL PRIMARY KEY,
                name       VARCHAR(255) NOT NULL,
                token      VARCHAR(255) NOT NULL,
                created_at TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE OR REPLACE FUNCTION auto_updated_at()
                RETURNS TRIGGER AS $$
            BEGIN
                NEW.updated_at = CURRENT_TIMESTAMP;
                RETURN NEW;
            END;
            $$ language 'plpgsql';

            CREATE TRIGGER auto_updated_at_keys
                BEFORE UPDATE ON keys
                FOR EACH ROW
            EXECUTE FUNCTION auto_updated_at();
            "#
        ).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            r#"
            DROP TABLE IF EXISTS keys;
            DROP FUNCTION IF EXISTS auto_updated_at();
            "#
        ).await?;
        Ok(())
    }
}
