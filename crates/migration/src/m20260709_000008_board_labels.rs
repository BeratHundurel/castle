use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BoardLabel::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BoardLabel::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BoardLabel::BoardId).integer().not_null())
                    .col(ColumnDef::new(BoardLabel::Name).string().not_null())
                    .col(ColumnDef::new(BoardLabel::Color).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_board_label_board")
                            .from(BoardLabel::Table, BoardLabel::BoardId)
                            .to(Board::Table, Board::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(EntryLabel::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EntryLabel::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(EntryLabel::EntryId).integer().not_null())
                    .col(
                        ColumnDef::new(EntryLabel::BoardLabelId)
                            .integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entry_label_entry")
                            .from(EntryLabel::Table, EntryLabel::EntryId)
                            .to(Entry::Table, Entry::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entry_label_board_label")
                            .from(EntryLabel::Table, EntryLabel::BoardLabelId)
                            .to(BoardLabel::Table, BoardLabel::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_board_label_board_id")
                    .table(BoardLabel::Table)
                    .col(BoardLabel::BoardId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_entry_label_entry_id")
                    .table(EntryLabel::Table)
                    .col(EntryLabel::EntryId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_entry_label_unique")
                    .table(EntryLabel::Table)
                    .col(EntryLabel::EntryId)
                    .col(EntryLabel::BoardLabelId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(EntryLabel::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(BoardLabel::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(Iden)]
enum Board {
    Table,
    Id,
}

#[derive(Iden)]
enum Entry {
    Table,
    Id,
}

#[derive(Iden)]
enum BoardLabel {
    Table,
    Id,
    BoardId,
    Name,
    Color,
}

#[derive(Iden)]
enum EntryLabel {
    Table,
    Id,
    EntryId,
    BoardLabelId,
}
