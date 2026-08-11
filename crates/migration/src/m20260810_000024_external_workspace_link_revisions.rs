use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const OPERATIONS: [&str; 3] = ["insert", "update", "delete"];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for operation in OPERATIONS {
            db.execute_unprepared(&format!(
                "DROP TRIGGER IF EXISTS castle_track_workspace_link_{operation}"
            ))
            .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for operation in OPERATIONS {
            db.execute_unprepared(&format!(
                r#"
                CREATE TRIGGER castle_track_workspace_link_{operation}
                AFTER {operation} ON workspace_link BEGIN
                    UPDATE castle_change_revision
                    SET revision = revision + 1,
                        board_revision = board_revision + 1,
                        note_revision = note_revision + 1,
                        link_revision = link_revision + 1
                    WHERE id = 1;
                END;
                "#
            ))
            .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Migrator;
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    #[tokio::test]
    async fn local_workspace_links_do_not_look_like_external_changes() -> Result<(), DbErr> {
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
                "SELECT revision, board_revision, note_revision, link_revision FROM castle_change_revision WHERE id = 1",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("change revision row is missing".to_string()))?;
        assert_eq!(revision.try_get::<i64>("", "revision")?, 0);
        assert_eq!(revision.try_get::<i64>("", "board_revision")?, 0);
        assert_eq!(revision.try_get::<i64>("", "note_revision")?, 0);
        assert_eq!(revision.try_get::<i64>("", "link_revision")?, 0);

        let triggers = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'castle_track_workspace_link_%'",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("trigger count row is missing".to_string()))?;
        assert_eq!(triggers.try_get::<i64>("", "count")?, 0);
        Ok(())
    }
}
