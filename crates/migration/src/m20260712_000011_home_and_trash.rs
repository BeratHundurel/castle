use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_nullable_integer(manager, Project::Table, Project::DeletedAt).await?;

        add_boolean(manager, Note::Table, Note::IsPinned).await?;
        add_nullable_integer(manager, Note::Table, Note::LastOpenedAt).await?;
        add_nullable_integer(manager, Note::Table, Note::DeletedAt).await?;

        add_boolean(manager, Board::Table, Board::IsPinned).await?;
        add_nullable_integer(manager, Board::Table, Board::LastOpenedAt).await?;
        add_nullable_integer(manager, Board::Table, Board::DeletedAt).await?;

        add_nullable_integer(manager, Card::Table, Card::DeletedAt).await?;
        add_nullable_integer(manager, Entry::Table, Entry::DeletedAt).await?;

        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE project SET deleted_at = CAST(strftime('%s', 'now') AS INTEGER) WHERE archived = 1 AND deleted_at IS NULL",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_column(manager, Entry::Table, Entry::DeletedAt).await?;
        drop_column(manager, Card::Table, Card::DeletedAt).await?;

        drop_column(manager, Board::Table, Board::DeletedAt).await?;
        drop_column(manager, Board::Table, Board::LastOpenedAt).await?;
        drop_column(manager, Board::Table, Board::IsPinned).await?;

        drop_column(manager, Note::Table, Note::DeletedAt).await?;
        drop_column(manager, Note::Table, Note::LastOpenedAt).await?;
        drop_column(manager, Note::Table, Note::IsPinned).await?;

        drop_column(manager, Project::Table, Project::DeletedAt).await?;
        Ok(())
    }
}

async fn add_nullable_integer<T, C>(
    manager: &SchemaManager<'_>,
    table: T,
    column: C,
) -> Result<(), DbErr>
where
    T: IntoIden + 'static,
    C: IntoIden + 'static,
{
    manager
        .alter_table(
            Table::alter()
                .table(table)
                .add_column(ColumnDef::new(column).big_integer().null())
                .to_owned(),
        )
        .await
}

async fn add_boolean<T, C>(manager: &SchemaManager<'_>, table: T, column: C) -> Result<(), DbErr>
where
    T: IntoIden + 'static,
    C: IntoIden + 'static,
{
    manager
        .alter_table(
            Table::alter()
                .table(table)
                .add_column(ColumnDef::new(column).boolean().not_null().default(false))
                .to_owned(),
        )
        .await
}

async fn drop_column<T, C>(manager: &SchemaManager<'_>, table: T, column: C) -> Result<(), DbErr>
where
    T: IntoIden + 'static,
    C: IntoIden + 'static,
{
    manager
        .alter_table(Table::alter().table(table).drop_column(column).to_owned())
        .await
}

#[derive(Iden)]
enum Project {
    Table,
    DeletedAt,
}

#[derive(Iden)]
enum Note {
    Table,
    IsPinned,
    LastOpenedAt,
    DeletedAt,
}

#[derive(Iden)]
enum Board {
    Table,
    IsPinned,
    LastOpenedAt,
    DeletedAt,
}

#[derive(Iden)]
enum Card {
    Table,
    DeletedAt,
}

#[derive(Iden)]
enum Entry {
    Table,
    DeletedAt,
}
