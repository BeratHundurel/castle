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
                CREATE TABLE IF NOT EXISTS workflow (
                    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    board_id INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    enabled BOOLEAN NOT NULL DEFAULT 1,
                    definition_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY (board_id) REFERENCES board(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_workflow_board_enabled
                    ON workflow(board_id, enabled, id);
                CREATE TABLE IF NOT EXISTS workflow_run (
                    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    workflow_id INTEGER NOT NULL,
                    board_id INTEGER NOT NULL,
                    entry_id INTEGER,
                    trigger_kind TEXT NOT NULL,
                    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'skipped')),
                    actions_json TEXT NOT NULL,
                    error TEXT,
                    started_at INTEGER NOT NULL,
                    finished_at INTEGER,
                    FOREIGN KEY (workflow_id) REFERENCES workflow(id) ON DELETE CASCADE,
                    FOREIGN KEY (board_id) REFERENCES board(id) ON DELETE CASCADE,
                    FOREIGN KEY (entry_id) REFERENCES entry(id) ON DELETE SET NULL
                );
                CREATE INDEX IF NOT EXISTS idx_workflow_run_board_started
                    ON workflow_run(board_id, started_at DESC, id DESC);
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
                DROP INDEX IF EXISTS idx_workflow_run_board_started;
                DROP TABLE IF EXISTS workflow_run;
                DROP INDEX IF EXISTS idx_workflow_board_enabled;
                DROP TABLE IF EXISTS workflow;
                "#,
            )
            .await?;
        Ok(())
    }
}
