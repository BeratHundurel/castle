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
                ALTER TABLE card
                ADD COLUMN workflow_role TEXT NOT NULL DEFAULT 'neutral'
                CHECK (workflow_role IN ('neutral', 'done', 'cancelled'))
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Card::Table)
                    .drop_column(Card::WorkflowRole)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Card {
    Table,
    WorkflowRole,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Migrator;
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    #[tokio::test]
    async fn workflow_role_defaults_to_neutral_and_rejects_unknown_values() -> Result<(), DbErr> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, Some(26)).await?;
        db.execute_unprepared(
            "INSERT INTO board (id, title, last_selected_view_id) VALUES (1, 'Board', 0)",
        )
        .await?;

        Migrator::up(&db, None).await?;
        db.execute_unprepared(
            "INSERT INTO card (id, title, board_id, position) VALUES (1, 'Ideas', 1, 0)",
        )
        .await?;

        let role = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT workflow_role FROM card WHERE id = 1",
            ))
            .await?
            .ok_or_else(|| DbErr::Custom("workflow role row is missing".to_string()))?
            .try_get::<String>("", "workflow_role")?;
        assert_eq!(role, "neutral");

        let invalid = db
            .execute_unprepared(
                "INSERT INTO card (id, title, board_id, position, workflow_role) VALUES (2, 'Invalid', 1, 1, 'unknown')",
            )
            .await;
        assert!(invalid.is_err());
        Ok(())
    }
}
