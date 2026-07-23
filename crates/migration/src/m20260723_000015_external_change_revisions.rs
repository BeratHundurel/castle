use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const TRACKED_TABLES: [&str; 9] = [
    "project",
    "board",
    "card",
    "entry",
    "board_label",
    "entry_label",
    "entry_checklist_item",
    "entry_attachment",
    "note",
];
const BOARD_TABLES: [&str; 8] = [
    "project",
    "board",
    "card",
    "entry",
    "board_label",
    "entry_label",
    "entry_checklist_item",
    "entry_attachment",
];
const OPERATIONS: [&str; 3] = ["INSERT", "UPDATE", "DELETE"];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for table in TRACKED_TABLES {
            drop_trigger_group(db, table).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for table in BOARD_TABLES {
            create_trigger_group(
                db,
                table,
                "revision = revision + 1, board_revision = board_revision + 1",
            )
            .await?;
        }
        create_trigger_group(
            db,
            "note",
            "revision = revision + 1, note_revision = note_revision + 1",
        )
        .await?;
        Ok(())
    }
}

async fn create_trigger_group(
    db: &SchemaManagerConnection<'_>,
    table: &str,
    assignments: &str,
) -> Result<(), DbErr> {
    for operation in OPERATIONS {
        db.execute_unprepared(&format!(
            "CREATE TRIGGER {}
             AFTER {operation} ON {table}
             BEGIN
                 UPDATE castle_change_revision SET {assignments} WHERE id = 1;
             END;",
            trigger_name(table, operation)
        ))
        .await?;
    }
    Ok(())
}

async fn drop_trigger_group(db: &SchemaManagerConnection<'_>, table: &str) -> Result<(), DbErr> {
    for operation in OPERATIONS {
        db.execute_unprepared(&format!(
            "DROP TRIGGER IF EXISTS {}",
            trigger_name(table, operation)
        ))
        .await?;
    }
    Ok(())
}

fn trigger_name(table: &str, operation: &str) -> String {
    format!("castle_track_{}_{}", table, operation.to_ascii_lowercase())
}
