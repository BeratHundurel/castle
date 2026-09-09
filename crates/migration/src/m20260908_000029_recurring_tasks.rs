use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                CREATE TABLE IF NOT EXISTS recurring_task (
                    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    entry_id INTEGER NOT NULL,
                    rule_json TEXT NOT NULL,
                    next_on TEXT NOT NULL,
                    until_on TEXT,
                    enabled BOOLEAN NOT NULL DEFAULT 1,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY (entry_id) REFERENCES entry(id) ON DELETE CASCADE
                );
                CREATE UNIQUE INDEX IF NOT EXISTS idx_recurring_task_entry
                    ON recurring_task(entry_id);
                CREATE INDEX IF NOT EXISTS idx_recurring_task_next_on
                    ON recurring_task(enabled, next_on);
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                DROP INDEX IF EXISTS idx_recurring_task_next_on;
                DROP INDEX IF EXISTS idx_recurring_task_entry;
                DROP TABLE IF EXISTS recurring_task;
                "#,
            )
            .await?;
        Ok(())
    }
}
