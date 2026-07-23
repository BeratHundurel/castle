use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Entry::Table)
                    .add_column(
                        ColumnDef::new(Entry::ReminderEnabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Entry::Table)
                    .add_column(ColumnDef::new(Entry::ReminderNotifiedFor).string().null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(EntryAttachment::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EntryAttachment::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(EntryAttachment::EntryId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EntryAttachment::FileName)
                            .string()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entry_attachment_entry")
                            .from(EntryAttachment::Table, EntryAttachment::EntryId)
                            .to(Entry::Table, Entry::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_entry_attachment_entry_id")
                    .table(EntryAttachment::Table)
                    .col(EntryAttachment::EntryId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(EntryAttachment::Table).to_owned())
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Entry::Table)
                    .drop_column(Entry::ReminderNotifiedFor)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Entry::Table)
                    .drop_column(Entry::ReminderEnabled)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Entry {
    Table,
    Id,
    ReminderEnabled,
    ReminderNotifiedFor,
}

#[derive(Iden)]
enum EntryAttachment {
    Table,
    Id,
    EntryId,
    FileName,
}
