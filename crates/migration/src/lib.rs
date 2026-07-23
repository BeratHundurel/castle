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
        ]
    }
}
