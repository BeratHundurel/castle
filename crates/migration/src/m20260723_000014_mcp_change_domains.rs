use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const BASE_BOARD_TABLES: [&str; 4] = ["project", "board", "card", "entry"];
const RELATED_BOARD_TABLES: [&str; 4] = [
    "board_label",
    "entry_label",
    "entry_checklist_item",
    "entry_attachment",
];
const OPERATIONS: [&str; 3] = ["INSERT", "UPDATE", "DELETE"];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ChangeRevision::Table)
                    .add_column(
                        ColumnDef::new(ChangeRevision::BoardRevision)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ChangeRevision::Table)
                    .add_column(
                        ColumnDef::new(ChangeRevision::NoteRevision)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        let db = manager.get_connection();
        for table in BASE_BOARD_TABLES {
            replace_trigger_group(
                db,
                table,
                "revision = revision + 1, board_revision = board_revision + 1",
            )
            .await?;
        }
        for table in RELATED_BOARD_TABLES {
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

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        drop_trigger_group(db, "note").await?;
        for table in RELATED_BOARD_TABLES {
            drop_trigger_group(db, table).await?;
        }
        for table in BASE_BOARD_TABLES {
            replace_trigger_group(db, table, "revision = revision + 1").await?;
        }

        manager
            .alter_table(
                Table::alter()
                    .table(ChangeRevision::Table)
                    .drop_column(ChangeRevision::NoteRevision)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ChangeRevision::Table)
                    .drop_column(ChangeRevision::BoardRevision)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

async fn replace_trigger_group(
    db: &SchemaManagerConnection<'_>,
    table: &str,
    assignments: &str,
) -> Result<(), DbErr> {
    drop_trigger_group(db, table).await?;
    create_trigger_group(db, table, assignments).await
}

async fn create_trigger_group(
    db: &SchemaManagerConnection<'_>,
    table: &str,
    assignments: &str,
) -> Result<(), DbErr> {
    for operation in OPERATIONS {
        let trigger_name = trigger_name(table, operation);
        db.execute_unprepared(&format!(
            "CREATE TRIGGER {trigger_name}
             AFTER {operation} ON {table}
             BEGIN
                 UPDATE castle_change_revision SET {assignments} WHERE id = 1;
             END;"
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

#[derive(DeriveIden)]
enum ChangeRevision {
    #[sea_orm(iden = "castle_change_revision")]
    Table,
    BoardRevision,
    NoteRevision,
}
