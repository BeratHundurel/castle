use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BoardProperty::Table)
                    .if_not_exists()
                    .col(id_column(BoardProperty::Id))
                    .col(ColumnDef::new(BoardProperty::BoardId).integer().not_null())
                    .col(ColumnDef::new(BoardProperty::Name).string().not_null())
                    .col(ColumnDef::new(BoardProperty::Kind).string().not_null())
                    .col(ColumnDef::new(BoardProperty::Position).integer().not_null())
                    .col(ColumnDef::new(BoardProperty::DeletedAt).integer().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_board_property_board")
                            .from(BoardProperty::Table, BoardProperty::BoardId)
                            .to(Board::Table, Board::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_board_property_board_position")
                    .table(BoardProperty::Table)
                    .col(BoardProperty::BoardId)
                    .col(BoardProperty::Position)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(BoardPropertyOption::Table)
                    .if_not_exists()
                    .col(id_column(BoardPropertyOption::Id))
                    .col(
                        ColumnDef::new(BoardPropertyOption::PropertyId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BoardPropertyOption::Name)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BoardPropertyOption::Color)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BoardPropertyOption::Position)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BoardPropertyOption::DeletedAt)
                            .integer()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_board_property_option_property")
                            .from(BoardPropertyOption::Table, BoardPropertyOption::PropertyId)
                            .to(BoardProperty::Table, BoardProperty::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_board_property_option_property_position")
                    .table(BoardPropertyOption::Table)
                    .col(BoardPropertyOption::PropertyId)
                    .col(BoardPropertyOption::Position)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(EntryPropertyValue::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EntryPropertyValue::EntryId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EntryPropertyValue::PropertyId)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EntryPropertyValue::TextValue)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(EntryPropertyValue::NumberValue)
                            .double()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(EntryPropertyValue::BooleanValue)
                            .boolean()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(EntryPropertyValue::DateValue)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(EntryPropertyValue::OptionId)
                            .integer()
                            .null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(EntryPropertyValue::EntryId)
                            .col(EntryPropertyValue::PropertyId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entry_property_value_entry")
                            .from(EntryPropertyValue::Table, EntryPropertyValue::EntryId)
                            .to(Entry::Table, Entry::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entry_property_value_property")
                            .from(EntryPropertyValue::Table, EntryPropertyValue::PropertyId)
                            .to(BoardProperty::Table, BoardProperty::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entry_property_value_option")
                            .from(EntryPropertyValue::Table, EntryPropertyValue::OptionId)
                            .to(BoardPropertyOption::Table, BoardPropertyOption::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        for (name, column) in [
            ("idx_entry_property_text", EntryPropertyValue::TextValue),
            ("idx_entry_property_number", EntryPropertyValue::NumberValue),
            (
                "idx_entry_property_boolean",
                EntryPropertyValue::BooleanValue,
            ),
            ("idx_entry_property_date", EntryPropertyValue::DateValue),
            ("idx_entry_property_option", EntryPropertyValue::OptionId),
        ] {
            manager
                .create_index(
                    Index::create()
                        .name(name)
                        .table(EntryPropertyValue::Table)
                        .col(EntryPropertyValue::PropertyId)
                        .col(column)
                        .to_owned(),
                )
                .await?;
        }

        manager
            .create_table(
                Table::create()
                    .table(SavedBoardView::Table)
                    .if_not_exists()
                    .col(id_column(SavedBoardView::Id))
                    .col(ColumnDef::new(SavedBoardView::BoardId).integer().not_null())
                    .col(ColumnDef::new(SavedBoardView::Name).string().not_null())
                    .col(
                        ColumnDef::new(SavedBoardView::Position)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SavedBoardView::IsDefault)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(SavedBoardView::ConfigVersion)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(ColumnDef::new(SavedBoardView::ConfigJson).text().not_null())
                    .col(ColumnDef::new(SavedBoardView::DeletedAt).integer().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_saved_board_view_board")
                            .from(SavedBoardView::Table, SavedBoardView::BoardId)
                            .to(Board::Table, Board::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_saved_board_view_board_position")
                    .table(SavedBoardView::Table)
                    .col(SavedBoardView::BoardId)
                    .col(SavedBoardView::Position)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SavedBoardView::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(EntryPropertyValue::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(BoardPropertyOption::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(BoardProperty::Table).to_owned())
            .await?;
        Ok(())
    }
}

fn id_column<T: IntoIden>(column: T) -> ColumnDef {
    ColumnDef::new(column)
        .integer()
        .not_null()
        .auto_increment()
        .primary_key()
        .to_owned()
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
enum BoardProperty {
    Table,
    Id,
    BoardId,
    Name,
    Kind,
    Position,
    DeletedAt,
}
#[derive(Iden)]
enum BoardPropertyOption {
    Table,
    Id,
    PropertyId,
    Name,
    Color,
    Position,
    DeletedAt,
}
#[derive(Iden)]
enum EntryPropertyValue {
    Table,
    EntryId,
    PropertyId,
    TextValue,
    NumberValue,
    BooleanValue,
    DateValue,
    OptionId,
}
#[derive(Iden)]
enum SavedBoardView {
    Table,
    Id,
    BoardId,
    Name,
    Position,
    IsDefault,
    ConfigVersion,
    ConfigJson,
    DeletedAt,
}
