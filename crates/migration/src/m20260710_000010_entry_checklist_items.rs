use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(EntryChecklistItem::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EntryChecklistItem::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(EntryChecklistItem::EntryId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EntryChecklistItem::Title)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EntryChecklistItem::Checked)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(EntryChecklistItem::Position)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entry_checklist_item_entry")
                            .from(EntryChecklistItem::Table, EntryChecklistItem::EntryId)
                            .to(Entry::Table, Entry::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_entry_checklist_item_entry_id")
                    .table(EntryChecklistItem::Table)
                    .col(EntryChecklistItem::EntryId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(EntryChecklistItem::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Entry {
    Table,
    Id,
}

#[derive(Iden)]
enum EntryChecklistItem {
    Table,
    Id,
    EntryId,
    Title,
    Checked,
    Position,
}
