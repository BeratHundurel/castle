use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE entry ADD COLUMN start_on TEXT;
                ALTER TABLE entry ADD COLUMN completed_at INTEGER;
                ALTER TABLE entry ADD COLUMN cancelled_at INTEGER;
                ALTER TABLE entry ADD COLUMN archived BOOLEAN NOT NULL DEFAULT 0;
                "#,
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_entry_archived_due_on")
                    .table(Entry::Table)
                    .col(Entry::Archived)
                    .col(Entry::DueOn)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_entry_archived_due_on")
                    .table(Entry::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Entry::Table)
                    .drop_column(Entry::Archived)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Entry::Table)
                    .drop_column(Entry::CancelledAt)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Entry::Table)
                    .drop_column(Entry::CompletedAt)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Entry::Table)
                    .drop_column(Entry::StartOn)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Entry {
    Table,
    StartOn,
    DueOn,
    CompletedAt,
    CancelledAt,
    Archived,
}
