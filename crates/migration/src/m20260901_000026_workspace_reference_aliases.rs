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
                CREATE TABLE IF NOT EXISTS workspace_reference_alias (
                    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    alias TEXT NOT NULL,
                    normalized_alias TEXT NOT NULL,
                    project_id INTEGER NULL,
                    board_id INTEGER NULL,
                    list_id INTEGER NULL,
                    card_id INTEGER NULL,
                    saved_view_id INTEGER NULL,
                    created_at INTEGER NOT NULL,
                    CONSTRAINT ck_workspace_reference_alias_target CHECK (
                        (project_id IS NOT NULL) +
                        (board_id IS NOT NULL) +
                        (list_id IS NOT NULL) +
                        (card_id IS NOT NULL) +
                        (saved_view_id IS NOT NULL) = 1
                    ),
                    FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE,
                    FOREIGN KEY (board_id) REFERENCES board(id) ON DELETE CASCADE,
                    FOREIGN KEY (list_id) REFERENCES card(id) ON DELETE CASCADE,
                    FOREIGN KEY (card_id) REFERENCES entry(id) ON DELETE CASCADE,
                    FOREIGN KEY (saved_view_id) REFERENCES saved_board_view(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_workspace_reference_alias_normalized
                    ON workspace_reference_alias(normalized_alias);
                CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_reference_alias_project
                    ON workspace_reference_alias(project_id, normalized_alias)
                    WHERE project_id IS NOT NULL;
                CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_reference_alias_board
                    ON workspace_reference_alias(board_id, normalized_alias)
                    WHERE board_id IS NOT NULL;
                CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_reference_alias_list
                    ON workspace_reference_alias(list_id, normalized_alias)
                    WHERE list_id IS NOT NULL;
                CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_reference_alias_card
                    ON workspace_reference_alias(card_id, normalized_alias)
                    WHERE card_id IS NOT NULL;
                CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_reference_alias_view
                    ON workspace_reference_alias(saved_view_id, normalized_alias)
                    WHERE saved_view_id IS NOT NULL;
                DELETE FROM note_link_index_state;
                DELETE FROM workspace_link_index_state;
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
                DROP INDEX IF EXISTS idx_workspace_reference_alias_normalized;
                DROP INDEX IF EXISTS idx_workspace_reference_alias_project;
                DROP INDEX IF EXISTS idx_workspace_reference_alias_board;
                DROP INDEX IF EXISTS idx_workspace_reference_alias_list;
                DROP INDEX IF EXISTS idx_workspace_reference_alias_card;
                DROP INDEX IF EXISTS idx_workspace_reference_alias_view;
                DROP TABLE IF EXISTS workspace_reference_alias;
                "#,
            )
            .await?;
        Ok(())
    }
}
