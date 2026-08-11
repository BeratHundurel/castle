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
                DELETE FROM note_link_index_state;
                DELETE FROM workspace_link_index_state;
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Migrator;
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    #[tokio::test]
    async fn migration_invalidates_index_state_without_removing_links() -> Result<(), DbErr> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, Some(24)).await?;
        db.execute_unprepared(
            r#"
            INSERT INTO note (id, title, cached_content, file_managed_by_app, created_at, updated_at)
            VALUES (1, 'Source', '[[board:1|Roadmap]]', 0, 0, 1);
            INSERT INTO board (id, title, last_selected_view_id) VALUES (1, 'Roadmap', 0);
            INSERT INTO note_link_index_state (note_id, indexed_updated_at)
            VALUES (1, 1);
            INSERT INTO workspace_link_index_state (source_kind, source_id, indexed_content)
            VALUES ('note', 1, 'workspace-fingerprint');
            INSERT INTO workspace_link (
                source_note_id, target_board_id, origin, ordinal, created_at
            ) VALUES (1, 1, 'note_wikilink', 0, 0);
            "#,
        )
        .await?;

        Migrator::up(&db, None).await?;

        for table in ["note_link_index_state", "workspace_link_index_state"] {
            let count = db
                .query_one_raw(Statement::from_string(
                    DbBackend::Sqlite,
                    format!("SELECT COUNT(*) AS count FROM {table}"),
                ))
                .await?
                .ok_or_else(|| DbErr::Custom(format!("{table} count row is missing")))?
                .try_get::<i64>("", "count")?;
            assert_eq!(count, 0);
        }

        let link_count = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM workspace_link",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("workspace link count row is missing".to_string()))?
            .try_get::<i64>("", "count")?;
        assert_eq!(link_count, 1);
        Ok(())
    }
}
