use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        manager
            .create_table(
                Table::create()
                    .table(Note::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Note::Id)
                            .integer()
                            .auto_increment()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Note::Title).string().not_null())
                    .col(ColumnDef::new(Note::ProjectId).integer().null())
                    .col(ColumnDef::new(Note::FilePath).string().null())
                    .col(ColumnDef::new(Note::CachedContent).string().not_null())
                    .col(ColumnDef::new(Note::FileMissingSince).integer().null())
                    .col(ColumnDef::new(Note::CreatedAt).integer().not_null())
                    .col(ColumnDef::new(Note::UpdatedAt).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_note_project")
                            .from(Note::Table, Note::ProjectId)
                            .to(Project::Table, Project::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_note_project_id")
                    .table(Note::Table)
                    .col(Note::ProjectId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_note_file_path")
                    .table(Note::Table)
                    .col(Note::FilePath)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        db.execute_unprepared("PRAGMA foreign_keys = OFF").await?;
        db.execute_unprepared("PRAGMA legacy_alter_table = ON")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS board_old")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_board_project_id")
            .await?;
        db.execute_unprepared("ALTER TABLE board RENAME TO board_old")
            .await?;
        db.execute_unprepared(
            r#"
            CREATE TABLE board (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                title VARCHAR NOT NULL,
                project_id INTEGER NULL,
                CONSTRAINT fk_board_project
                    FOREIGN KEY (project_id)
                    REFERENCES project (id)
                    ON DELETE SET NULL
            )
            "#,
        )
        .await?;
        db.execute_unprepared(
            "INSERT INTO board (id, title, project_id) SELECT id, title, project_id FROM board_old",
        )
        .await?;
        db.execute_unprepared("DROP TABLE board_old").await?;
        db.execute_unprepared("CREATE INDEX idx_board_project_id ON board (project_id)")
            .await?;
        db.execute_unprepared("PRAGMA legacy_alter_table = OFF")
            .await?;
        db.execute_unprepared("PRAGMA foreign_keys = ON").await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("PRAGMA foreign_keys = OFF").await?;
        db.execute_unprepared("PRAGMA legacy_alter_table = ON")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS board_old")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_board_project_id")
            .await?;
        db.execute_unprepared("ALTER TABLE board RENAME TO board_old")
            .await?;
        db.execute_unprepared(
            r#"
            CREATE TABLE board (
                id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
                title VARCHAR NOT NULL,
                project_id INTEGER NOT NULL,
                CONSTRAINT fk_board_project
                    FOREIGN KEY (project_id)
                    REFERENCES project (id)
                    ON DELETE CASCADE
            )
            "#,
        )
        .await?;
        db.execute_unprepared(
            r#"
            INSERT INTO board (id, title, project_id)
            SELECT id, title, project_id FROM board_old WHERE project_id IS NOT NULL
            "#,
        )
        .await?;
        db.execute_unprepared("DROP TABLE board_old").await?;
        db.execute_unprepared("CREATE INDEX idx_board_project_id ON board (project_id)")
            .await?;
        db.execute_unprepared("PRAGMA legacy_alter_table = OFF")
            .await?;
        db.execute_unprepared("PRAGMA foreign_keys = ON").await?;

        manager
            .drop_table(Table::drop().table(Note::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(Iden)]
enum Project {
    Table,
    Id,
}

#[derive(Iden)]
enum Note {
    Table,
    Id,
    Title,
    ProjectId,
    FilePath,
    CachedContent,
    FileMissingSince,
    CreatedAt,
    UpdatedAt,
}
