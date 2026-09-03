use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write as _},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result};
use entity::{
    board::Entity as Board, card, card::Entity as BoardList, entry, entry::Entity as BoardCard,
    entry_checklist_item, entry_property_value, note, note::Entity as Note,
    project::Entity as Project,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter,
};

use crate::{
    board::properties::{BoardViewConfig, PropertyKey, PropertyKind},
    board::templates::{
        BoardTemplateColumn, BoardTemplateDefinition, BoardTemplateEntry,
        create_board_from_template_in_transaction,
    },
    workspace::WorkspaceItem,
};

pub const DOCS_NOTE_TITLE: &str = "docs.md";
pub const STARTER_BOARD_TITLE: &str = "Your first board";

const OPEN_CARD_TITLE: &str = "Open this card: there is more inside";
const DRAG_CARD_TITLE: &str = "Drag me to another list";
const PROPERTIES_CARD_TITLE: &str = "Add structure with custom properties";
const VIEW_CARD_TITLE: &str = "Save a focused board view";
const TEMPLATE_CARD_TITLE: &str = "Reuse a board as a template";
const CONNECT_CARD_TITLE: &str = "Turn note text into a connected card";
const MERMAID_CARD_TITLE: &str = "Draw an idea with Mermaid";
const EMBED_CARD_TITLE: &str = "Embed a live board view in a note";
const CAPTURE_CARD_TITLE: &str = "Capture a thought instantly";
const SEARCH_CARD_TITLE: &str = "Find anything with workspace search";
const MARKDOWN_CARD_TITLE: &str = "Edit faster with Markdown tools";
const FILE_CARD_TITLE: &str = "Open files in their native format";
const TRASH_CARD_TITLE: &str = "Recover work from Trash";
const ARCHIVE_CARD_TITLE: &str = "Move a workspace between installs";
const AGENT_CARD_TITLE: &str = "Connect a trusted local agent";
const MAKE_IT_YOURS_CARD_TITLE: &str = "Make this workspace yours";

fn docs_content(starter_board_title: &str, starter_view_name: &str) -> String {
    let starter_board_title = crate::workspace::links::escape_segment(starter_board_title);
    let starter_view_name = crate::workspace::links::escape_segment(starter_view_name);
    format!(
        r#"# Welcome to Castle

Castle is a local-first workspace where notes, files, and boards stay connected. This guide and [[board:{starter_board_title}]] are ordinary workspace items: edit them, move them, or delete them whenever you are ready.

## Start anywhere

Use **Quick Capture** with `Ctrl+Alt+N` to open a small note window from anywhere. Press **Enter** to save a note, **Shift+Enter** for a new line, or **Esc** to close it. You can change this global shortcut in **Settings → General → Tray**.

Press `Ctrl+P` for the command palette. It can create notes and boards, open files, switch themes, search the workspace, open settings, and insert board views. Type `new: Launch`, `new note: Brief`, or `new board: Roadmap` to start with a title.

Use `Ctrl+Shift+F` for full-text workspace search. Results include notes and board content, show a useful preview, and open at the matching item. The **Home** screen also gathers pinned and recent items plus cards due today or overdue.

## See Castle working

Switch this note from **Write** to **Read** in the top-right corner. Castle will render the diagram and live board below while keeping their source as portable Markdown.

```mermaid
flowchart LR
    A["Capture in notes"] --> B["Shape ideas into cards"]
    B --> C["Organize with fields and views"]
    C --> D["Embed the live view in a note"]
```

## A live board inside this note

The projection below is read-only so the board remains the single editable source. Change a card on [[board:{starter_board_title}]], then return here to see the note stay in sync.

![[board:{starter_board_title}#{starter_view_name}]]

Use **Insert board view** from the command palette to embed any board or saved view without writing this reference by hand.

## Notes, files, and Markdown

| Try this | What Castle does |
| --- | --- |
| Type `[[` and choose a note, board, list, or card | Creates a navigable workspace link and tracks it in the **Links** inspector |
| Select a sentence, press `Ctrl+P`, then choose **Create card from selection** | Creates a card and keeps it related to this note |
| Paste an image into a Markdown note | Copies it into local attachments and inserts portable Markdown |
| Add headings, tables, code, or Mermaid fences | Renders them in **Read** mode and builds a navigable outline |
| Open a Markdown, JSON, or text file | Edits the original file with matching syntax and outline support |

Use `Ctrl+Shift+O` for the outline and links inspector. In **Write** mode, `Alt+Shift+F` formats the current document; Markdown also supports smart list and task continuation, task toggling, line movement, Emmet abbreviations, and optional Vim editing. Use **Read**, **Side by side**, or **Write** as the default note view in **Settings → Editor → Markdown**.

The document type menu supports Markdown, JSON, and plain text. Save a file in place with `Ctrl+S`, or use `Ctrl+Shift+S` to choose a new path. Changes made by another process are picked up by the open workspace while unsaved editor changes remain protected.

## Links that stay useful

Type `[[` to complete references to notes, boards, lists, and cards. Castle displays readable labels in **Read** mode, supports explicit labels such as `[[board:Roadmap|Release plan]]`, and remembers previous names when an item is renamed. Use the **Links** inspector to review outbound links and backlinks, and click a resolved link to navigate.

## Boards can model more than tasks

Open [[board:{starter_board_title}|Your first board]] and try the seeded examples. A card can hold Markdown, labels, a checklist, attachments, a due date, an optional reminder, custom fields, and related notes.

Boards also support:

- text, number, checkbox, date, select, and URL properties;
- temporary filters and sorting, configurable fields, and compact cards;
- named views that preserve a useful board perspective;
- reusable templates for workflows, collections, queues, and plans;
- linked notes created from a card or connected later;
- duplicate, move, and reorder actions for cards and lists.

## Recover and move your workspace

Deleting a project, board, list, card, or note moves it to **Trash** first. Use the undo action or open Trash to restore it; permanent deletion and **Empty Trash** are separate, explicit actions. Archived projects from older Castle data are migrated into the same recovery flow.

Use **Settings → General → Workspace** or the command palette to export a `.castle.zip` containing notes, boards, links, attachments, and settings. Import it later in **Merge** mode to keep current work, or **Replace workspace** mode to restore the archive as the current workspace.

## Work with trusted agents

Open **Settings → Agent Access** and enable MCP when you want a trusted local agent to work with Castle. Agents can read and search notes, create and update notes and board cards, manage projects, labels, checklists, due dates, and reminders, and move workspace items. Keep MCP disabled unless you trust the local clients that can access it.

## A five-minute tour

1. Switch this note to **Read** and inspect the diagram and embedded board.
2. Open the starter board, expand **Open this card: there is more inside**, and complete a checklist item.
3. Drag **Drag me to another list** and change its label or custom property.
4. Press `Ctrl+Alt+N` to capture a note, then find it with `Ctrl+Shift+F`.
5. Return here, select a sentence, and run **Create card from selection** from `Ctrl+P`.
6. Create a new note and run **Insert board view** to connect your own dashboard.
7. Move a disposable item to Trash and restore it, then inspect the archive options in Settings.

## Make it yours

Press `Ctrl+P` to create or open anything quickly. Settings includes themes, typography, layout, editor behavior, shortcuts, tray and Quick Capture controls, workspace archives, Agent Access, and optional Vim mode. Castle starts with a tour; what replaces it is entirely yours.
"#
    )
}

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

    let transaction = db.begin().await?;
    let starter_board = create_board_from_template_in_transaction(
        &transaction,
        None,
        STARTER_BOARD_TITLE.to_string(),
        starter_board_definition(),
    )
    .await?;
    seed_starter_board_details(&transaction, &starter_board).await?;

    let docs_content = docs_content(&starter_board.title, "Feature tour");
    let docs_path = write_docs_file(data_dir, &docs_content)?;
    let now = now_ts();
    let note_result = note::ActiveModel {
        title: Set(DOCS_NOTE_TITLE.to_string()),
        project_id: Set(None),
        file_path: Set(Some(docs_path.to_string_lossy().into_owned())),
        file_managed_by_app: Set(true),
        cached_content: Set(docs_content.clone()),
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

    if let Err(err) = transaction.commit().await {
        remove_seed_file(&docs_path);
        return Err(err.into());
    }
    crate::note::links::index_note_links(db, note.id, &docs_content, note.updated_at).await?;

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

fn write_docs_file(data_dir: &Path, content: &str) -> Result<PathBuf> {
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
                if let Err(err) = file.write_all(content.as_bytes()) {
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

async fn seed_starter_board_details(
    transaction: &DatabaseTransaction,
    board: &WorkspaceItem,
) -> Result<i64> {
    let list_ids = BoardList::find()
        .filter(card::Column::BoardId.eq(i64::from(board.id)))
        .all(transaction)
        .await?
        .into_iter()
        .map(|list| list.id)
        .collect::<Vec<_>>();
    let entries = BoardCard::find()
        .filter(entry::Column::CardId.is_in(list_ids))
        .all(transaction)
        .await?;

    let try_me = crate::board::commands::create_label(
        transaction,
        board.id,
        "Try me".to_string(),
        "blue".to_string(),
    )
    .await?;
    let connected = crate::board::commands::create_label(
        transaction,
        board.id,
        "Connected".to_string(),
        "purple".to_string(),
    )
    .await?;

    for title in [OPEN_CARD_TITLE, DRAG_CARD_TITLE] {
        crate::board::commands::set_label_assignment(
            transaction,
            starter_entry_id(&entries, title)?,
            try_me.id,
            true,
        )
        .await?;
    }
    for title in [CONNECT_CARD_TITLE, EMBED_CARD_TITLE] {
        crate::board::commands::set_label_assignment(
            transaction,
            starter_entry_id(&entries, title)?,
            connected.id,
            true,
        )
        .await?;
    }

    let open_card_id = i64::from(starter_entry_id(&entries, OPEN_CARD_TITLE)?);
    for (position, title, checked) in [
        (0, "Open the card details", true),
        (1, "Complete or add a checklist item", false),
        (2, "Try a due date, reminder, or attachment", false),
    ] {
        entry_checklist_item::ActiveModel {
            entry_id: Set(open_card_id),
            title: Set(title.to_string()),
            checked: Set(checked),
            position: Set(position),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
    }

    let area = crate::board::properties::create_property(
        transaction,
        i64::from(board.id),
        "Area".to_string(),
        PropertyKind::Select,
    )
    .await?;
    let notes = crate::board::properties::create_property_option(
        transaction,
        area.id,
        "Notes".to_string(),
        "blue".to_string(),
    )
    .await?;
    let boards = crate::board::properties::create_property_option(
        transaction,
        area.id,
        "Boards".to_string(),
        "green".to_string(),
    )
    .await?;
    let connections = crate::board::properties::create_property_option(
        transaction,
        area.id,
        "Connections".to_string(),
        "purple".to_string(),
    )
    .await?;
    let workflow = crate::board::properties::create_property_option(
        transaction,
        area.id,
        "Workflow".to_string(),
        "orange".to_string(),
    )
    .await?;
    let workspace = crate::board::properties::create_property_option(
        transaction,
        area.id,
        "Workspace".to_string(),
        "red".to_string(),
    )
    .await?;

    for (title, option_id) in [
        (OPEN_CARD_TITLE, boards.id),
        (DRAG_CARD_TITLE, boards.id),
        (PROPERTIES_CARD_TITLE, boards.id),
        (VIEW_CARD_TITLE, boards.id),
        (TEMPLATE_CARD_TITLE, boards.id),
        (CONNECT_CARD_TITLE, connections.id),
        (MERMAID_CARD_TITLE, notes.id),
        (EMBED_CARD_TITLE, connections.id),
        (CAPTURE_CARD_TITLE, notes.id),
        (SEARCH_CARD_TITLE, workspace.id),
        (MARKDOWN_CARD_TITLE, notes.id),
        (FILE_CARD_TITLE, notes.id),
        (TRASH_CARD_TITLE, workspace.id),
        (ARCHIVE_CARD_TITLE, workspace.id),
        (AGENT_CARD_TITLE, workflow.id),
        (MAKE_IT_YOURS_CARD_TITLE, notes.id),
    ] {
        entry_property_value::ActiveModel {
            entry_id: Set(i64::from(starter_entry_id(&entries, title)?)),
            property_id: Set(area.id),
            text_value: Set(None),
            number_value: Set(None),
            boolean_value: Set(None),
            date_value: Set(None),
            option_id: Set(Some(option_id)),
        }
        .insert(transaction)
        .await?;
    }

    let view = crate::board::properties::create_board_view(
        transaction,
        i64::from(board.id),
        "Feature tour".to_string(),
        BoardViewConfig {
            visible_properties: vec![PropertyKey::Custom(area.id)],
            ..Default::default()
        },
    )
    .await?;
    crate::board::properties::set_selected_board_view(
        transaction,
        i64::from(board.id),
        Some(view.id),
    )
    .await?;

    Ok(view.id)
}

fn starter_entry_id(entries: &[entry::Model], title: &str) -> Result<u32> {
    entries
        .iter()
        .find(|entry| entry.title == title)
        .map(|entry| entry.id)
        .and_then(|id| u32::try_from(id).ok())
        .with_context(|| format!("starter card {title:?} was not created"))
}

fn starter_board_definition() -> BoardTemplateDefinition {
    BoardTemplateDefinition {
        columns: vec![
            BoardTemplateColumn {
                title: "Start here".to_string(),
                entries: vec![
                    BoardTemplateEntry {
                        title: OPEN_CARD_TITLE.to_string(),
                        description: "**Cards can carry the work, not just its name.** This one already has a label, a custom property, and a checklist. You can also add a due date, reminder, attachment, and linked note.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: DRAG_CARD_TITLE.to_string(),
                        description: "Drag this card between lists, reorder it within a list, or open the list menu to rename and reshape the board.".to_string(),
                    },
                ],
            },
            BoardTemplateColumn {
                title: "Shape a workflow".to_string(),
                entries: vec![
                    BoardTemplateEntry {
                        title: PROPERTIES_CARD_TITLE.to_string(),
                        description: "Use **Properties** to add text, number, checkbox, date, select, or URL fields. Choose **Fields** to place the useful ones directly on cards.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: VIEW_CARD_TITLE.to_string(),
                        description: "Filter or sort this board, choose visible fields and compact cards, then save the result as a named view. This board opens in its saved **Feature tour** view.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: TEMPLATE_CARD_TITLE.to_string(),
                        description: "Use **Template** to save any useful board structure and reuse it for another project, collection, queue, or plan.".to_string(),
                    },
                ],
            },
            BoardTemplateColumn {
                title: "Connect notes + boards".to_string(),
                entries: vec![
                    BoardTemplateEntry {
                        title: CONNECT_CARD_TITLE.to_string(),
                        description: "Select text in a note, press `Ctrl+P`, and run **Create card from selection**. Castle creates the card and keeps its source note related.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: MERMAID_CARD_TITLE.to_string(),
                        description: "Markdown preview renders flowcharts and other supported Mermaid diagrams. Open **docs.md** and switch to **Read** to see the seeded example.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: EMBED_CARD_TITLE.to_string(),
                        description: "In a Markdown note, run **Insert board view**. Castle inserts a read-only projection that stays synced with the editable source board.".to_string(),
                    },
                ],
            },
            BoardTemplateColumn {
                title: "Work faster".to_string(),
                entries: vec![
                    BoardTemplateEntry {
                        title: CAPTURE_CARD_TITLE.to_string(),
                        description: "Use the global Quick Capture shortcut (`Ctrl+Alt+N` by default) to save a focused note without leaving your current work. Change the shortcut in **Settings → General → Tray**.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: SEARCH_CARD_TITLE.to_string(),
                        description: "Open workspace search with `Ctrl+Shift+F` to search note and board content, inspect a preview, and jump directly to the matching item.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: MARKDOWN_CARD_TITLE.to_string(),
                        description: "In a Markdown source editor, format with `Alt+Shift+F`, continue lists and tasks automatically, move lines with `Alt+Up` or `Alt+Down`, or enable optional Vim mode in Settings.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: FILE_CARD_TITLE.to_string(),
                        description: "Open Markdown, JSON, and plain text files directly. Castle preserves the source file, supports **Write**, **Read**, and **Side by side** views, and can save a copy to a new path.".to_string(),
                    },
                ],
            },
            BoardTemplateColumn {
                title: "Keep it safe".to_string(),
                entries: vec![
                    BoardTemplateEntry {
                        title: TRASH_CARD_TITLE.to_string(),
                        description: "Deleted workspace items go to **Trash** first. Restore them with undo or from the Trash view, then use permanent delete only when you are sure.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: ARCHIVE_CARD_TITLE.to_string(),
                        description: "Export notes, boards, links, attachments, and settings to a `.castle.zip` archive. Import it later by merging with current work or replacing the current workspace.".to_string(),
                    },
                    BoardTemplateEntry {
                        title: AGENT_CARD_TITLE.to_string(),
                        description: "Enable **Settings → Agent Access** to let a trusted local MCP client read and update Castle. Keep agent access disabled when you do not need it.".to_string(),
                    },
                ],
            },
            BoardTemplateColumn {
                title: "Make it yours".to_string(),
                entries: vec![BoardTemplateEntry {
                    title: MAKE_IT_YOURS_CARD_TITLE.to_string(),
                    description: "Rename this board, change its lists, add your own cards, or delete the tour. Starter content is ordinary editable workspace data.".to_string(),
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
        let seeded_docs = fs::read_to_string(&seeded.docs_path)?;
        assert!(seeded_docs.contains("```mermaid\nflowchart LR"));
        for section in [
            "## Start anywhere",
            "## Notes, files, and Markdown",
            "## Links that stay useful",
            "## Recover and move your workspace",
            "## Work with trusted agents",
        ] {
            assert!(
                seeded_docs.contains(section),
                "missing onboarding section: {section}"
            );
        }
        let embeds = crate::board::projection::parse_board_view_embeds(&seeded_docs);
        assert_eq!(embeds.len(), 1);
        assert_eq!(embeds[0].board_path, vec![STARTER_BOARD_TITLE]);
        assert_eq!(embeds[0].view_name.as_deref(), Some("Feature tour"));

        let stored_note = Note::find_by_id(i64::from(seeded.docs_note.id))
            .one(&db)
            .await?
            .context("seeded note should exist")?;
        assert!(stored_note.file_managed_by_app);
        assert_eq!(stored_note.cached_content, seeded_docs);
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
            vec![
                "Start here",
                "Shape a workflow",
                "Connect notes + boards",
                "Work faster",
                "Keep it safe",
                "Make it yours"
            ]
        );
        let column_ids = columns.iter().map(|column| column.id).collect::<Vec<_>>();
        assert_eq!(
            entry::Entity::find()
                .filter(entry::Column::CardId.is_in(column_ids))
                .all(&db)
                .await?
                .len(),
            16
        );

        let snapshot = crate::board::load_board_snapshot(&db, seeded.starter_board.id).await?;
        assert_eq!(
            snapshot
                .labels
                .iter()
                .map(|label| label.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Try me", "Connected"]
        );
        let open_card = snapshot
            .cards
            .iter()
            .flat_map(|list| &list.entries)
            .find(|card| card.title == OPEN_CARD_TITLE)
            .context("starter checklist card should exist")?;
        assert_eq!(open_card.checklist_items.len(), 3);
        assert!(open_card.checklist_items[0].checked);
        assert_eq!(open_card.labels[0].name, "Try me");

        let properties = crate::board::properties::load_board_properties(
            &db,
            i64::from(seeded.starter_board.id),
        )
        .await?;
        assert_eq!(properties.definitions.len(), 1);
        assert_eq!(properties.definitions[0].name, "Area");
        assert_eq!(properties.definitions[0].kind, PropertyKind::Select);
        assert_eq!(properties.definitions[0].options.len(), 5);
        assert_eq!(properties.values.len(), 16);

        let views =
            crate::board::properties::load_board_views(&db, i64::from(seeded.starter_board.id))
                .await?;
        assert_eq!(views.views.len(), 1);
        assert_eq!(views.views[0].name, "Feature tour");
        assert_eq!(views.selected_view_id, Some(views.views[0].id));
        assert_eq!(embeds[0].raw_target, "board:Your first board#Feature tour");
        assert_eq!(
            stored_note.cached_content,
            docs_content(&seeded.starter_board.title, &views.views[0].name)
        );
        assert_eq!(
            views.views[0].config.visible_properties,
            vec![PropertyKey::Custom(properties.definitions[0].id)]
        );

        let crate::board::projection::BoardViewProjectionResult::Available(projection) =
            crate::board::projection::load_board_view_projection(
                &db,
                i64::from(seeded.starter_board.id),
                Some(views.views[0].id),
            )
            .await?
        else {
            anyhow::bail!("starter board view should produce an embedded projection");
        };
        assert_eq!(projection.view_name.as_deref(), Some("Feature tour"));
        assert_eq!(projection.matching_card_count, 16);
        assert!(
            projection
                .lists
                .iter()
                .flat_map(|list| &list.cards)
                .all(|card| card.custom_properties.len() == 1)
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
