use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Board::Table)
                    .add_column(
                        ColumnDef::new(Board::LastSelectedViewId)
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
                UPDATE board
                SET last_selected_view_id = COALESCE(
                    (
                        SELECT id
                        FROM saved_board_view
                        WHERE board_id = board.id AND deleted_at IS NULL
                        ORDER BY is_default DESC, position ASC, id ASC
                        LIMIT 1
                    ),
                    0
                )
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Board::Table)
                    .drop_column(Board::LastSelectedViewId)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum Board {
    Table,
    LastSelectedViewId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Migrator;
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    #[tokio::test]
    async fn migration_keeps_the_view_boards_previously_opened_with() -> Result<(), DbErr> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, Some(21)).await?;
        db.execute_unprepared(
            r#"
            INSERT INTO board (id, title) VALUES
                (1, 'Default view'),
                (2, 'First view'),
                (3, 'All cards');
            INSERT INTO saved_board_view (
                id, board_id, name, position, is_default, config_version, config_json
            ) VALUES
                (1, 1, 'First', 0, 0, 2, '{}'),
                (2, 1, 'Default', 1, 1, 2, '{}'),
                (3, 2, 'First', 0, 0, 2, '{}'),
                (4, 2, 'Second', 1, 0, 2, '{}');
            "#,
        )
        .await?;

        Migrator::up(&db, None).await?;

        let selected = db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT last_selected_view_id FROM board ORDER BY id",
            ))
            .await?
            .into_iter()
            .map(|row| row.try_get::<i64>("", "last_selected_view_id"))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(selected, vec![2, 3, 0]);
        Ok(())
    }
}
