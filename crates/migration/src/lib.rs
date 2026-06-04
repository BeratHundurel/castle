pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20260101_000002_add_card_position;
mod m20260522_000003_notes_and_optional_board_projects;
mod m20260604_000004_project_archive_and_position;
mod m20260604_000005_entry_position;
mod m20260604_000006_note_file_ownership;

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
        ]
    }
}
