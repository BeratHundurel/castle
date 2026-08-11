use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write as _},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result};
use entity::{board::Entity as Board, note, note::Entity as Note, project::Entity as Project};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};

use crate::{
    board_templates::{
        BoardTemplateColumn, BoardTemplateDefinition, BoardTemplateEntry,
        create_board_from_template_in_transaction,
    },
    workspace::WorkspaceItem,
};

pub const DOCS_NOTE_TITLE: &str = "docs.md";
pub const STARTER_BOARD_TITLE: &str = "Your first board";

const DOCS_CONTENT: &str = r#"# Welcome to Castle

Castle is a local workspace for notes and boards. This guide and the starter board are yours: edit them, move them, or delete them whenever you are ready.

## Start here

1. Write in this note. Castle saves managed notes as you work.
2. Open **Your first board** in the sidebar and drag a card between columns.
3. Use the **+** menu to create another note or choose a board template.
4. Press `Ctrl+P` to find actions and anything in your workspace.

## Notes

- Write Markdown, plain text, or JSON.
- Link notes with `[[double brackets]]`.
- Toggle Markdown preview from the editor or command palette.
- Use the outline to move through longer documents.

```json
{
  "tip": "This block is here so you can try JSON and Markdown formatting."
}
```

## Boards

Boards are flexible lists, not a prescribed task system. Use them for a project, reading queue, content plan, collection, or anything else that benefits from moving cards through a space.

- Open a card to add details.
- Add, rename, reorder, or remove columns.
- Save a useful board as your own reusable template.

## Make it yours

Open Settings to choose a theme, typography, editor behavior, and optional Vim mode. Castle starts with examples, but the workspace belongs to you.
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FreshWorkspace {
    pub docs_note: WorkspaceItem,
    pub starter_board: WorkspaceItem,
    pub docs_path: PathBuf,
}

pub async fn seed_fresh_workspace(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    data_dir: &Path,
) -> Result<Option<FreshWorkspace>> {
    if workspace_has_items(db).await? {
        return Ok(None);
    }

    let docs_path = write_docs_file(data_dir)?;
    let transaction = db.begin().await?;
    let now = now_ts();
    let note_result = note::ActiveModel {
        title: Set(DOCS_NOTE_TITLE.to_string()),
        project_id: Set(None),
        file_path: Set(Some(docs_path.to_string_lossy().into_owned())),
        file_managed_by_app: Set(true),
        cached_content: Set(DOCS_CONTENT.to_string()),
        file_missing_since: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        last_opened_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(&transaction)
    .await;

    let note = match note_result {
        Ok(note) => note,
        Err(err) => {
            remove_seed_file(&docs_path);
            return Err(err.into());
        }
    };

    let starter_board = match create_board_from_template_in_transaction(
        &transaction,
        None,
        STARTER_BOARD_TITLE.to_string(),
        starter_board_definition(),
    )
    .await
    {
        Ok(board) => board,
        Err(err) => {
            remove_seed_file(&docs_path);
            return Err(err);
        }
    };

    if let Err(err) = transaction.commit().await {
        remove_seed_file(&docs_path);
        return Err(err.into());
    }
    crate::note_links::index_note_links(db, note.id, DOCS_CONTENT, note.updated_at).await?;

    Ok(Some(FreshWorkspace {
        docs_note: WorkspaceItem {
            id: note.id as u32,
            title: note.title,
        },
        starter_board,
        docs_path,
    }))
}

async fn workspace_has_items(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
) -> Result<bool> {
    Ok(Project::find().one(db).await?.is_some()
        || Board::find().one(db).await?.is_some()
        || Note::find().one(db).await?.is_some())
}

fn write_docs_file(data_dir: &Path) -> Result<PathBuf> {
    let notes_dir = data_dir.join("notes");
    fs::create_dir_all(&notes_dir)
        .with_context(|| format!("failed to create {}", notes_dir.display()))?;

    for suffix in 1_u32.. {
        let file_name = if suffix == 1 {
            "docs.md".to_string()
        } else {
            format!("docs-{suffix}.md")
        };
        let path = notes_dir.join(file_name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                if let Err(err) = file.write_all(DOCS_CONTENT.as_bytes()) {
                    drop(file);
                    remove_seed_file(&path);
                    return Err(err).with_context(|| format!("failed to write {}", path.display()));
                }
                return Ok(path);
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => {
                return Err(err).with_context(|| format!("failed to create {}", path.display()));
            }
        }
    }

    unreachable!()
}

fn remove_seed_file(path: &Path) {
    if let Err(err) = fs::remove_file(path) {
        eprintln!(
            "Failed to clean up onboarding file {}: {err}",
            path.display()
        );
    }
}

fn starter_board_definition() -> BoardTemplateDefinition {
    BoardTemplateDefinition {
        columns: vec![
            BoardTemplateColumn {
                title: "Ideas".to_string(),
                entries: vec![BoardTemplateEntry {
                    title: "Capture something you want to shape".to_string(),
                    description: "Ideas can be projects, questions, collections, or anything else you want to make visible.".to_string(),
                }],
            },
            BoardTemplateColumn {
                title: "In progress".to_string(),
                entries: vec![
                    BoardTemplateEntry {
                        title: "Drag this card to another column".to_string(),
                        description: "Cards and columns can be reordered as your thinking changes.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: "Open a card and add context".to_string(),
                        description: "Keep the useful details with the thing they describe.".to_string(),
                    },
                ],
            },
            BoardTemplateColumn {
                title: "Done".to_string(),
                entries: vec![BoardTemplateEntry {
                    title: "Make this board yours".to_string(),
                    description: "Rename it, change the columns, save it as a template, or delete it and start fresh.".to_string(),
                }],
            },
        ],
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity::{card, entry};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ColumnTrait, Database, QueryFilter, QueryOrder};

    #[tokio::test]
    async fn seeds_a_docs_file_and_editable_starter_board() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let seeded = seed_fresh_workspace(&db, directory.path())
            .await?
            .context("fresh workspace should be seeded")?;

        assert_eq!(seeded.docs_note.title, DOCS_NOTE_TITLE);
        assert_eq!(seeded.starter_board.title, STARTER_BOARD_TITLE);
        assert_eq!(seeded.docs_path, directory.path().join("notes/docs.md"));
        assert_eq!(fs::read_to_string(&seeded.docs_path)?, DOCS_CONTENT);

        let stored_note = Note::find_by_id(i64::from(seeded.docs_note.id))
            .one(&db)
            .await?
            .context("seeded note should exist")?;
        assert!(stored_note.file_managed_by_app);
        assert_eq!(stored_note.cached_content, DOCS_CONTENT);
        assert!(stored_note.last_opened_at.is_some());

        let columns = card::Entity::find()
            .filter(card::Column::BoardId.eq(i64::from(seeded.starter_board.id)))
            .order_by_asc(card::Column::Position)
            .all(&db)
            .await?;
        assert_eq!(
            columns
                .iter()
                .map(|column| column.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Ideas", "In progress", "Done"]
        );
        let column_ids = columns.iter().map(|column| column.id).collect::<Vec<_>>();
        assert_eq!(
            entry::Entity::find()
                .filter(entry::Column::CardId.is_in(column_ids))
                .all(&db)
                .await?
                .len(),
            4
        );
        Ok(())
    }

    #[tokio::test]
    async fn does_not_seed_a_workspace_that_already_has_content() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        crate::workspace::create_board(&db, None, "Existing".to_string()).await?;

        assert!(seed_fresh_workspace(&db, directory.path()).await?.is_none());
        assert!(!directory.path().join("notes/docs.md").exists());
        assert_eq!(Board::find().all(&db).await?.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn preserves_an_existing_docs_file() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let notes_dir = directory.path().join("notes");
        fs::create_dir_all(&notes_dir)?;
        fs::write(notes_dir.join("docs.md"), "keep me")?;
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let seeded = seed_fresh_workspace(&db, directory.path())
            .await?
            .context("fresh workspace should be seeded")?;

        assert_eq!(fs::read_to_string(notes_dir.join("docs.md"))?, "keep me");
        assert_eq!(seeded.docs_path, notes_dir.join("docs-2.md"));
        Ok(())
    }
}
