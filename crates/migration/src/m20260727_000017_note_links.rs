use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(NoteAlias::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NoteAlias::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(NoteAlias::NoteId).integer().not_null())
                    .col(ColumnDef::new(NoteAlias::Alias).string().not_null())
                    .col(
                        ColumnDef::new(NoteAlias::NormalizedAlias)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(NoteAlias::CreatedAt).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_note_alias_note")
                            .from(NoteAlias::Table, NoteAlias::NoteId)
                            .to(Note::Table, Note::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_note_alias_note_id")
                    .table(NoteAlias::Table)
                    .col(NoteAlias::NoteId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_note_alias_normalized")
                    .table(NoteAlias::Table)
                    .col(NoteAlias::NormalizedAlias)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(NoteLink::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(NoteLink::SourceNoteId).integer().not_null())
                    .col(ColumnDef::new(NoteLink::Ordinal).integer().not_null())
                    .col(ColumnDef::new(NoteLink::TargetNoteId).integer().null())
                    .col(ColumnDef::new(NoteLink::RawTarget).string().not_null())
                    .col(ColumnDef::new(NoteLink::DisplayText).string().null())
                    .col(ColumnDef::new(NoteLink::StartByte).integer().not_null())
                    .col(ColumnDef::new(NoteLink::EndByte).integer().not_null())
                    .col(ColumnDef::new(NoteLink::LineNumber).integer().not_null())
                    .primary_key(
                        Index::create()
                            .col(NoteLink::SourceNoteId)
                            .col(NoteLink::Ordinal),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_note_link_source")
                            .from(NoteLink::Table, NoteLink::SourceNoteId)
                            .to(Note::Table, Note::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_note_link_target")
                            .from(NoteLink::Table, NoteLink::TargetNoteId)
                            .to(Note::Table, Note::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_note_link_target")
                    .table(NoteLink::Table)
                    .col(NoteLink::TargetNoteId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(NoteLinkIndexState::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NoteLinkIndexState::NoteId)
                            .integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(NoteLinkIndexState::IndexedUpdatedAt)
                            .integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_note_link_index_state_note")
                            .from(NoteLinkIndexState::Table, NoteLinkIndexState::NoteId)
                            .to(Note::Table, Note::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(NoteLinkIndexState::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(NoteLink::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(NoteAlias::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Note {
    Table,
    Id,
}

#[derive(Iden)]
enum NoteAlias {
    Table,
    Id,
    NoteId,
    Alias,
    NormalizedAlias,
    CreatedAt,
}

#[derive(Iden)]
enum NoteLink {
    Table,
    SourceNoteId,
    Ordinal,
    TargetNoteId,
    RawTarget,
    DisplayText,
    StartByte,
    EndByte,
    LineNumber,
}

#[derive(Iden)]
enum NoteLinkIndexState {
    Table,
    NoteId,
    IndexedUpdatedAt,
}
