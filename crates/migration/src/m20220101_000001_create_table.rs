use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Project::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Project::Id)
                            .integer()
                            .auto_increment()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Project::Name).string().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Board::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Board::Id)
                            .integer()
                            .auto_increment()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Board::Title).string().not_null())
                    .col(ColumnDef::new(Board::ProjectId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_board_project")
                            .from(Board::Table, Board::ProjectId)
                            .to(Project::Table, Project::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Card::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Card::Id)
                            .integer()
                            .auto_increment()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Card::Title).string().not_null())
                    .col(
                        ColumnDef::new(Card::Position)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Card::BoardId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_card_board")
                            .from(Card::Table, Card::BoardId)
                            .to(Board::Table, Board::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Entry::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Entry::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Entry::Title).string().not_null())
                    .col(ColumnDef::new(Entry::Description).string().not_null())
                    .col(ColumnDef::new(Entry::CardId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entry_card")
                            .from(Entry::Table, Entry::CardId)
                            .to(Card::Table, Card::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_board_project_id")
                    .table(Board::Table)
                    .col(Board::ProjectId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_card_board_id")
                    .table(Card::Table)
                    .col(Card::BoardId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_entry_card_id")
                    .table(Entry::Table)
                    .col(Entry::CardId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Entry::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Card::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Board::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Project::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(Iden)]
enum Project {
    Table,
    Id,
    Name,
}

#[derive(Iden)]
enum Board {
    Table,
    Id,
    Title,
    ProjectId,
}

#[derive(Iden)]
enum Card {
    Table,
    Id,
    Title,
    Position,
    BoardId,
}

#[derive(Iden)]
enum Entry {
    Table,
    Id,
    Title,
    Description,
    CardId,
}
