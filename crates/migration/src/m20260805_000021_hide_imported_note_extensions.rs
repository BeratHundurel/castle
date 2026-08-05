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
                UPDATE note
                SET title = CASE
                    WHEN lower(title) LIKE '%.markdown' THEN substr(title, 1, length(title) - 9)
                    WHEN lower(title) LIKE '%.json' THEN substr(title, 1, length(title) - 5)
                    WHEN lower(title) LIKE '%.txt' THEN substr(title, 1, length(title) - 4)
                    WHEN lower(title) LIKE '%.md' THEN substr(title, 1, length(title) - 3)
                    ELSE title
                END
                WHERE file_managed_by_app = 0
                    AND project_id IN (
                        SELECT id
                        FROM project
                        WHERE folder_path IS NOT NULL
                    )
                    AND (
                        lower(title) LIKE '%.markdown'
                        OR lower(title) LIKE '%.json'
                        OR lower(title) LIKE '%.txt'
                        OR lower(title) LIKE '%.md'
                    )
                "#,
            )
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
