use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DbBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let foreign_keys = db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_key_list(card)",
            ))
            .await?;
        let mut references_stale_board = false;
        for foreign_key in foreign_keys {
            if foreign_key.try_get::<String>("", "table")? == "board_old" {
                references_stale_board = true;
                break;
            }
        }

        if !references_stale_board {
            return Ok(());
        }

        db.execute_unprepared("PRAGMA writable_schema = ON").await?;
        let repair_result = db
            .execute_unprepared(
                r#"
                UPDATE sqlite_schema
                SET sql = replace(sql, 'REFERENCES "board_old"', 'REFERENCES "board"')
                WHERE type = 'table'
                    AND name = 'card'
                    AND sql LIKE '%REFERENCES "board_old"%'
                "#,
            )
            .await;
        let reset_result = db
            .execute_unprepared("PRAGMA writable_schema = RESET")
            .await;
        let repair_result = repair_result?;
        reset_result?;

        if repair_result.rows_affected() != 1 {
            return Err(DbErr::Migration(
                "could not repair the stale card.board_id foreign key".to_string(),
            ));
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }

    fn use_transaction(&self) -> Option<bool> {
        Some(true)
    }
}
