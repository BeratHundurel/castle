use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE VIRTUAL TABLE search_index USING fts5(
                    item_type,
                    item_id UNINDEXED,
                    parent_id UNINDEXED,
                    project_id UNINDEXED,
                    title,
                    body,
                    tokenize = 'unicode61'
                )",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS search_index")
            .await?;

        Ok(())
    }
}
