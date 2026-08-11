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
                CREATE TABLE IF NOT EXISTS workspace_link (
                    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                    source_note_id INTEGER NULL,
                    source_entry_id INTEGER NULL,
                    target_note_id INTEGER NULL,
                    target_board_id INTEGER NULL,
                    target_card_id INTEGER NULL,
                    target_entry_id INTEGER NULL,
                    target_saved_view_id INTEGER NULL,
                    origin TEXT NOT NULL,
                    ordinal INTEGER NOT NULL DEFAULT 0,
                    raw_target TEXT NULL,
                    display_text TEXT NULL,
                    start_byte INTEGER NULL,
                    end_byte INTEGER NULL,
                    line_number INTEGER NULL,
                    created_at INTEGER NOT NULL,
                    CONSTRAINT ck_workspace_link_source CHECK (
                        (source_note_id IS NOT NULL) + (source_entry_id IS NOT NULL) = 1
                    ),
                    CONSTRAINT ck_workspace_link_target CHECK (
                        (target_note_id IS NOT NULL) +
                        (target_board_id IS NOT NULL) +
                        (target_card_id IS NOT NULL) +
                        (target_entry_id IS NOT NULL) = 1
                    ),
                    CONSTRAINT ck_workspace_link_view CHECK (
                        target_saved_view_id IS NULL OR target_board_id IS NOT NULL
                    ),
                    CONSTRAINT ck_workspace_link_origin CHECK (
                        origin IN ('manual', 'note_wikilink', 'entry_wikilink', 'embed')
                    ),
                    FOREIGN KEY (source_note_id) REFERENCES note(id) ON DELETE CASCADE,
                    FOREIGN KEY (source_entry_id) REFERENCES entry(id) ON DELETE CASCADE,
                    FOREIGN KEY (target_note_id) REFERENCES note(id) ON DELETE CASCADE,
                    FOREIGN KEY (target_board_id) REFERENCES board(id) ON DELETE CASCADE,
                    FOREIGN KEY (target_card_id) REFERENCES card(id) ON DELETE CASCADE,
                    FOREIGN KEY (target_entry_id) REFERENCES entry(id) ON DELETE CASCADE,
                    FOREIGN KEY (target_saved_view_id) REFERENCES saved_board_view(id) ON DELETE SET NULL
                );
                CREATE INDEX IF NOT EXISTS idx_workspace_link_source_note
                    ON workspace_link(source_note_id, origin, ordinal);
                CREATE INDEX IF NOT EXISTS idx_workspace_link_source_entry
                    ON workspace_link(source_entry_id, origin, ordinal);
                CREATE INDEX IF NOT EXISTS idx_workspace_link_target_note
                    ON workspace_link(target_note_id);
                CREATE INDEX IF NOT EXISTS idx_workspace_link_target_board
                    ON workspace_link(target_board_id);
                CREATE INDEX IF NOT EXISTS idx_workspace_link_target_card
                    ON workspace_link(target_card_id);
                CREATE INDEX IF NOT EXISTS idx_workspace_link_target_entry
                    ON workspace_link(target_entry_id);
                CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_link_manual_board
                    ON workspace_link(source_note_id, target_board_id)
                    WHERE origin = 'manual' AND target_board_id IS NOT NULL;
                CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_link_manual_card
                    ON workspace_link(source_note_id, target_card_id)
                    WHERE origin = 'manual' AND target_card_id IS NOT NULL;
                CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_link_manual_entry
                    ON workspace_link(source_note_id, target_entry_id)
                    WHERE origin = 'manual' AND target_entry_id IS NOT NULL;

                CREATE TABLE IF NOT EXISTS workspace_link_index_state (
                    source_kind TEXT NOT NULL,
                    source_id INTEGER NOT NULL,
                    indexed_content TEXT NOT NULL,
                    PRIMARY KEY (source_kind, source_id),
                    CONSTRAINT ck_workspace_link_index_kind CHECK (
                        source_kind IN ('note', 'entry')
                    )
                );
                "#,
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ChangeRevision::Table)
                    .add_column(
                        ColumnDef::new(ChangeRevision::LinkRevision)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                CREATE TRIGGER castle_track_workspace_link_insert
                AFTER INSERT ON workspace_link BEGIN
                    UPDATE castle_change_revision
                    SET revision = revision + 1,
                        board_revision = board_revision + 1,
                        note_revision = note_revision + 1,
                        link_revision = link_revision + 1
                    WHERE id = 1;
                END;
                CREATE TRIGGER castle_track_workspace_link_update
                AFTER UPDATE ON workspace_link BEGIN
                    UPDATE castle_change_revision
                    SET revision = revision + 1,
                        board_revision = board_revision + 1,
                        note_revision = note_revision + 1,
                        link_revision = link_revision + 1
                    WHERE id = 1;
                END;
                CREATE TRIGGER castle_track_workspace_link_delete
                AFTER DELETE ON workspace_link BEGIN
                    UPDATE castle_change_revision
                    SET revision = revision + 1,
                        board_revision = board_revision + 1,
                        note_revision = note_revision + 1,
                        link_revision = link_revision + 1
                    WHERE id = 1;
                END;
                CREATE TRIGGER castle_cleanup_note_workspace_link_index
                AFTER DELETE ON note BEGIN
                    DELETE FROM workspace_link_index_state
                    WHERE source_kind = 'note' AND source_id = OLD.id;
                END;
                CREATE TRIGGER castle_cleanup_entry_workspace_link_index
                AFTER DELETE ON entry BEGIN
                    DELETE FROM workspace_link_index_state
                    WHERE source_kind = 'entry' AND source_id = OLD.id;
                END;
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
                DROP TRIGGER IF EXISTS castle_cleanup_note_workspace_link_index;
                DROP TRIGGER IF EXISTS castle_cleanup_entry_workspace_link_index;
                "#,
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(WorkspaceLinkIndexState::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(WorkspaceLink::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ChangeRevision::Table)
                    .drop_column(ChangeRevision::LinkRevision)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum WorkspaceLink {
    Table,
}

#[derive(Iden)]
enum WorkspaceLinkIndexState {
    Table,
}

#[derive(DeriveIden)]
enum ChangeRevision {
    #[sea_orm(iden = "castle_change_revision")]
    Table,
    LinkRevision,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Migrator;
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    #[tokio::test]
    async fn workspace_links_enforce_one_source_and_target() -> Result<(), DbErr> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        db.execute_unprepared(
            r#"
            INSERT INTO note (id, title, cached_content, file_managed_by_app, created_at, updated_at)
            VALUES (1, 'Note', '', 0, 0, 0);
            INSERT INTO board (id, title, last_selected_view_id) VALUES (1, 'Board', 0);
            INSERT INTO workspace_link (
                source_note_id, target_board_id, origin, ordinal, created_at
            ) VALUES (1, 1, 'manual', 0, 0);
            "#,
        )
        .await?;
        let revision = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT link_revision FROM castle_change_revision WHERE id = 1",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("change revision row is missing".to_string()))?;
        assert_eq!(revision.try_get::<i64>("", "link_revision")?, 0);

        let invalid = db
            .execute_unprepared(
                r#"
                INSERT INTO workspace_link (
                    source_note_id, target_note_id, target_board_id, origin, ordinal, created_at
                ) VALUES (1, 1, 1, 'manual', 0, 0);
                "#,
            )
            .await;
        assert!(invalid.is_err());
        let indexes = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'index' AND name LIKE 'idx_workspace_link_%'",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("index count row is missing".to_string()))?;
        assert_eq!(indexes.try_get::<i64>("", "count")?, 9);
        db.execute_unprepared("DELETE FROM board WHERE id = 1")
            .await?;
        let links = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM workspace_link",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("link count row is missing".to_string()))?;
        assert_eq!(links.try_get::<i64>("", "count")?, 0);

        Migrator::down(&db, Some(3)).await?;
        let tables = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name IN ('workspace_link', 'workspace_link_index_state')",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("table count row is missing".to_string()))?;
        assert_eq!(tables.try_get::<i64>("", "count")?, 0);
        let revision_columns = db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA table_info(castle_change_revision)",
            ))
            .await?;
        assert!(revision_columns.iter().all(|column| {
            column
                .try_get::<String>("", "name")
                .is_ok_and(|name| name != "link_revision")
        }));
        Ok(())
    }
}
