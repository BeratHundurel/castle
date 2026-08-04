use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BoardTemplate::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BoardTemplate::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BoardTemplate::Name).string().not_null())
                    .col(
                        ColumnDef::new(BoardTemplate::Description)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BoardTemplate::DefinitionJson)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BoardTemplate::CreatedAt)
                            .integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_board_template_created_at")
                    .table(BoardTemplate::Table)
                    .col(BoardTemplate::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BoardTemplate::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum BoardTemplate {
    Table,
    Id,
    Name,
    Description,
    DefinitionJson,
    CreatedAt,
}
