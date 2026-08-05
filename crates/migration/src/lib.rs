pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20260101_000002_add_card_position;
mod m20260522_000003_notes_and_optional_board_projects;
mod m20260604_000004_project_archive_and_position;
mod m20260604_000005_entry_position;
mod m20260604_000006_note_file_ownership;
mod m20260607_180117_full_text;
mod m20260709_000008_board_labels;
mod m20260710_000009_entry_due_date;
mod m20260710_000010_entry_checklist_items;
mod m20260712_000011_home_and_trash;
mod m20260723_000012_change_revision;
mod m20260723_000013_entry_attachments_and_reminders;
mod m20260723_000014_mcp_change_domains;
mod m20260723_000015_external_change_revisions;
mod m20260723_000016_project_folder_path;
mod m20260727_000017_note_links;
mod m20260727_000018_board_properties_and_views;
mod m20260805_000019_board_templates;
mod m20260805_000020_repair_card_board_foreign_key;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20260101_000002_add_card_position::Migration),
            Box::new(m20260522_000003_notes_and_optional_board_projects::Migration),
            Box::new(m20260604_000004_project_archive_and_position::Migration),
            Box::new(m20260604_000005_entry_position::Migration),
            Box::new(m20260604_000006_note_file_ownership::Migration),
            Box::new(m20260607_180117_full_text::Migration),
            Box::new(m20260709_000008_board_labels::Migration),
            Box::new(m20260710_000009_entry_due_date::Migration),
            Box::new(m20260710_000010_entry_checklist_items::Migration),
            Box::new(m20260712_000011_home_and_trash::Migration),
            Box::new(m20260723_000012_change_revision::Migration),
            Box::new(m20260723_000013_entry_attachments_and_reminders::Migration),
            Box::new(m20260723_000014_mcp_change_domains::Migration),
            Box::new(m20260723_000015_external_change_revisions::Migration),
            Box::new(m20260723_000016_project_folder_path::Migration),
            Box::new(m20260727_000017_note_links::Migration),
            Box::new(m20260727_000018_board_properties_and_views::Migration),
            Box::new(m20260805_000019_board_templates::Migration),
            Box::new(m20260805_000020_repair_card_board_foreign_key::Migration),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    async fn card_board_reference(db: &sea_orm::DatabaseConnection) -> Result<String, DbErr> {
        let foreign_keys = db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_key_list(card)",
            ))
            .await?;
        for foreign_key in foreign_keys {
            if foreign_key.try_get::<String>("", "from")? == "board_id" {
                return foreign_key.try_get("", "table");
            }
        }
        Err(DbErr::Custom(
            "card.board_id foreign key was not found".to_string(),
        ))
    }

    #[tokio::test]
    async fn optional_board_project_migration_keeps_card_reference() -> Result<(), DbErr> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, Some(3)).await?;

        assert_eq!(card_board_reference(&db).await?, "board");
        Ok(())
    }

    #[tokio::test]
    async fn latest_migration_repairs_stale_card_reference() -> Result<(), DbErr> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, Some(19)).await?;
        db.execute_unprepared(
            r#"
            PRAGMA writable_schema = ON;
            UPDATE sqlite_schema
            SET sql = replace(sql, 'REFERENCES "board"', 'REFERENCES "board_old"')
            WHERE type = 'table' AND name = 'card';
            PRAGMA writable_schema = RESET;
            "#,
        )
        .await?;
        assert_eq!(card_board_reference(&db).await?, "board_old");

        Migrator::up(&db, None).await?;

        assert_eq!(card_board_reference(&db).await?, "board");
        db.execute_unprepared(
            r#"
            INSERT INTO board (title) VALUES ('Triage');
            INSERT INTO card (title, board_id, position) VALUES ('Reported', last_insert_rowid(), 0);
            "#,
        )
        .await?;
        Ok(())
    }

}
