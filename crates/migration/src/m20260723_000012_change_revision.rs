use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const TRACKED_TABLES: [&str; 4] = ["project", "board", "card", "entry"];
const OPERATIONS: [&str; 3] = ["INSERT", "UPDATE", "DELETE"];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE castle_change_revision (
                id INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
                revision INTEGER NOT NULL DEFAULT 0
            );
            INSERT INTO castle_change_revision (id, revision) VALUES (1, 0);",
        )
        .await?;

        for table in TRACKED_TABLES {
            for operation in OPERATIONS {
                let trigger_name =
                    format!("castle_track_{}_{}", table, operation.to_ascii_lowercase());
                db.execute_unprepared(&format!(
                    "CREATE TRIGGER {trigger_name}
                     AFTER {operation} ON {table}
                     BEGIN
                         UPDATE castle_change_revision
                         SET revision = revision + 1
                         WHERE id = 1;
                     END;"
                ))
                .await?;
            }
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for table in TRACKED_TABLES {
            for operation in OPERATIONS {
                let trigger_name =
                    format!("castle_track_{}_{}", table, operation.to_ascii_lowercase());
                db.execute_unprepared(&format!("DROP TRIGGER IF EXISTS {trigger_name}"))
                    .await?;
            }
        }
        db.execute_unprepared("DROP TABLE IF EXISTS castle_change_revision")
            .await?;
        Ok(())
    }
}
