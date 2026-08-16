use anyhow::{Context as _, Result, bail};
use chrono::Utc;
use entity::{
    board::Entity as Board, board_template, board_template::Entity as BoardTemplateEntity, card,
    card::Entity as Card, entry, entry::Entity as Entry,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter,
    QueryOrder,
};
use serde::{Deserialize, Serialize};

use crate::workspace::WorkspaceItem;

const MAX_TEMPLATE_COLUMNS: usize = 100;
const MAX_TEMPLATE_ENTRIES: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardTemplateId {
    BuiltIn(&'static str),
    Custom(i64),
}

impl BoardTemplateId {
    pub fn key(&self) -> String {
        match self {
            Self::BuiltIn(id) => format!("built-in:{id}"),
            Self::Custom(id) => format!("custom:{id}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardTemplate {
    pub id: BoardTemplateId,
    pub name: String,
    pub description: String,
    pub definition: BoardTemplateDefinition,
}

impl BoardTemplate {
    pub fn column_count(&self) -> usize {
        self.definition.columns.len()
    }

    pub fn entry_count(&self) -> usize {
        self.definition
            .columns
            .iter()
            .map(|column| column.entries.len())
            .sum()
    }

    pub fn summary(&self) -> String {
        template_summary(self.column_count(), self.entry_count())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardTemplateDefinition {
    pub columns: Vec<BoardTemplateColumn>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardTemplateColumn {
    pub title: String,
    #[serde(default)]
    pub entries: Vec<BoardTemplateEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardTemplateEntry {
    pub title: String,
    #[serde(default)]
    pub description: String,
}

pub fn built_in_templates() -> Vec<BoardTemplate> {
    vec![
        built_in(
            "blank",
            "Blank board",
            "Start with an empty canvas and add only what you need.",
            &[],
        ),
        built_in(
            "kanban",
            "Kanban",
            "A focused workflow with starter cards for planning, doing, and reflecting.",
            &[
                (
                    "To do",
                    &[
                        (
                            "Clarify the next outcome",
                            "Write down what success looks like before starting the work.",
                        ),
                        (
                            "Break work into small steps",
                            "Turn the outcome into cards that can move independently.",
                        ),
                    ],
                ),
                (
                    "In progress",
                    &[(
                        "Focus on one active item",
                        "Keep work in progress small so blocked work stays visible.",
                    )],
                ),
                (
                    "Done",
                    &[(
                        "Review what shipped",
                        "Capture follow-ups and move completed work here.",
                    )],
                ),
            ],
        ),
        built_in(
            "personal-tasks",
            "Personal tasks",
            "Capture loose ends, plan the week, and keep today's commitments realistic.",
            &[
                (
                    "Inbox",
                    &[
                        (
                            "Capture loose ends",
                            "Add anything competing for your attention without sorting it yet.",
                        ),
                        (
                            "Collect errands and reminders",
                            "Keep quick obligations here until the next planning pass.",
                        ),
                    ],
                ),
                (
                    "This week",
                    &[
                        (
                            "Plan the week",
                            "Choose a few outcomes worth finishing before the week ends.",
                        ),
                        (
                            "Make time for something restorative",
                            "Protect time for rest, learning, or people you care about.",
                        ),
                    ],
                ),
                (
                    "Today",
                    &[(
                        "Choose today's focus",
                        "Pick the smallest set of commitments that would make today feel complete.",
                    )],
                ),
                (
                    "Done",
                    &[(
                        "Review completed work",
                        "Move finished cards here and clear the column during your weekly reset.",
                    )],
                ),
            ],
        ),
        built_in(
            "project-plan",
            "Project plan",
            "Shape a project from its first questions through delivery and follow-up.",
            &[
                (
                    "Backlog",
                    &[
                        (
                            "Define the problem",
                            "Describe who is affected, what is happening, and why it matters.",
                        ),
                        (
                            "Collect requirements",
                            "Record essential needs, constraints, open questions, and non-goals.",
                        ),
                    ],
                ),
                (
                    "Planned",
                    &[
                        (
                            "Set milestones",
                            "Split delivery into meaningful checkpoints with clear outcomes.",
                        ),
                        (
                            "Identify risks and dependencies",
                            "Note what could block progress and who can help resolve it.",
                        ),
                    ],
                ),
                (
                    "In progress",
                    &[(
                        "Build the first milestone",
                        "Keep implementation notes, decisions, and links together on this card.",
                    )],
                ),
                (
                    "Review",
                    &[(
                        "Review with stakeholders",
                        "Confirm the outcome meets the agreed scope and capture requested changes.",
                    )],
                ),
                (
                    "Done",
                    &[(
                        "Share the outcome",
                        "Document what changed, where to find it, and any follow-up work.",
                    )],
                ),
            ],
        ),
        built_in(
            "content-calendar",
            "Content calendar",
            "Develop a balanced publishing pipeline from idea to performance review.",
            &[
                (
                    "Ideas",
                    &[
                        (
                            "Answer a recurring audience question",
                            "List the question, the audience, and the useful takeaway.",
                        ),
                        (
                            "Tell a customer story",
                            "Capture the challenge, turning point, result, and permission needed.",
                        ),
                    ],
                ),
                (
                    "Drafting",
                    &[(
                        "Create the first draft",
                        "Add an outline, working headline, key examples, and call to action.",
                    )],
                ),
                (
                    "Review",
                    &[(
                        "Editorial review",
                        "Check accuracy, voice, accessibility, links, and supporting visuals.",
                    )],
                ),
                (
                    "Scheduled",
                    &[(
                        "Prepare distribution",
                        "Set the publish date and adapt the message for each channel.",
                    )],
                ),
                (
                    "Published",
                    &[(
                        "Review performance",
                        "Record what resonated, what to improve, and ideas worth reusing.",
                    )],
                ),
            ],
        ),
        built_in(
            "bug-triage",
            "Bug triage",
            "Turn incoming reports into reproducible, verified fixes with clear context.",
            &[
                (
                    "Reported",
                    &[
                        (
                            "Capture a new report",
                            "Include the observed behavior, expected behavior, environment, and impact.",
                        ),
                        (
                            "Request missing details",
                            "Ask for reproduction steps, screenshots, logs, or a sample file.",
                        ),
                    ],
                ),
                (
                    "Confirmed",
                    &[(
                        "Reproduce and assess",
                        "Record the smallest reproduction, severity, affected area, and likely owner.",
                    )],
                ),
                (
                    "In progress",
                    &[(
                        "Implement a focused fix",
                        "Note the root cause and add regression coverage for the failing behavior.",
                    )],
                ),
                (
                    "Verify",
                    &[(
                        "Verify the original scenario",
                        "Retest the reproduction and check neighboring edge cases for regressions.",
                    )],
                ),
                (
                    "Resolved",
                    &[(
                        "Record the resolution",
                        "Summarize the fix, validation performed, and any remaining follow-up.",
                    )],
                ),
            ],
        ),
    ]
}

pub async fn load_custom_templates(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
) -> Result<Vec<BoardTemplate>> {
    let models = BoardTemplateEntity::find()
        .order_by_desc(board_template::Column::CreatedAt)
        .order_by_desc(board_template::Column::Id)
        .all(db)
        .await?;

    models
        .into_iter()
        .map(|model| {
            let definition = serde_json::from_str(&model.definition_json)
                .with_context(|| format!("template {} contains invalid data", model.id))?;
            validate_definition(&definition)?;
            Ok(BoardTemplate {
                id: BoardTemplateId::Custom(model.id),
                name: model.name,
                description: model.description,
                definition,
            })
        })
        .collect()
}

pub async fn create_board_from_template(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    project_id: Option<u32>,
    title: String,
    definition: BoardTemplateDefinition,
) -> Result<WorkspaceItem> {
    let transaction = db.begin().await?;
    let board =
        create_board_from_template_in_transaction(&transaction, project_id, title, definition)
            .await?;
    transaction.commit().await?;
    let snapshot = crate::board::load_board_snapshot(db, board.id).await?;
    let indexed_at = Utc::now().timestamp();
    for entry in snapshot.cards.into_iter().flat_map(|list| list.entries) {
        crate::workspace::links::index_entry_workspace_links(
            db,
            i64::from(entry.id),
            &entry.description,
            indexed_at,
        )
        .await?;
    }
    Ok(board)
}

pub(crate) async fn create_board_from_template_in_transaction(
    transaction: &DatabaseTransaction,
    project_id: Option<u32>,
    title: String,
    definition: BoardTemplateDefinition,
) -> Result<WorkspaceItem> {
    let title = title.trim();
    if title.is_empty() {
        bail!("board name cannot be empty");
    }
    validate_definition(&definition)?;

    let board = entity::board::ActiveModel {
        title: Set(title.to_string()),
        project_id: Set(project_id.map(i64::from)),
        ..Default::default()
    }
    .insert(transaction)
    .await?;

    for (column_position, template_column) in definition.columns.into_iter().enumerate() {
        let column = card::ActiveModel {
            title: Set(template_column.title),
            board_id: Set(board.id),
            position: Set(column_position as i32),
            ..Default::default()
        }
        .insert(transaction)
        .await?;

        for (entry_position, template_entry) in template_column.entries.into_iter().enumerate() {
            entry::ActiveModel {
                title: Set(template_entry.title),
                description: Set(template_entry.description),
                card_id: Set(column.id),
                position: Set(entry_position as i32),
                ..Default::default()
            }
            .insert(transaction)
            .await?;
        }
    }

    Ok(WorkspaceItem {
        id: board.id as u32,
        title: board.title,
    })
}

pub async fn save_board_as_template(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    board_id: u32,
    name: String,
) -> Result<BoardTemplate> {
    let name = name.trim();
    if name.is_empty() {
        bail!("template name cannot be empty");
    }

    let transaction = db.begin().await?;
    Board::find_by_id(i64::from(board_id))
        .one(&transaction)
        .await?
        .with_context(|| format!("board {board_id} was not found"))?;

    let columns = Card::find()
        .filter(card::Column::BoardId.eq(i64::from(board_id)))
        .filter(card::Column::DeletedAt.is_null())
        .order_by_asc(card::Column::Position)
        .order_by_asc(card::Column::Id)
        .all(&transaction)
        .await?;
    let column_ids = columns.iter().map(|column| column.id).collect::<Vec<_>>();
    let entries = if column_ids.is_empty() {
        Vec::new()
    } else {
        Entry::find()
            .filter(entry::Column::CardId.is_in(column_ids))
            .filter(entry::Column::DeletedAt.is_null())
            .order_by_asc(entry::Column::Position)
            .order_by_asc(entry::Column::Id)
            .all(&transaction)
            .await?
    };

    let definition = BoardTemplateDefinition {
        columns: columns
            .into_iter()
            .map(|column| BoardTemplateColumn {
                title: column.title,
                entries: entries
                    .iter()
                    .filter(|entry| entry.card_id == column.id)
                    .map(|entry| BoardTemplateEntry {
                        title: entry.title.clone(),
                        description: entry.description.clone(),
                    })
                    .collect(),
            })
            .collect(),
    };
    validate_definition(&definition)?;
    let description = "Saved from one of your boards.".to_string();
    let definition_json = serde_json::to_string(&definition)?;
    let model = board_template::ActiveModel {
        name: Set(name.to_string()),
        description: Set(description.clone()),
        definition_json: Set(definition_json),
        created_at: Set(Utc::now().timestamp()),
        ..Default::default()
    }
    .insert(&transaction)
    .await?;
    transaction.commit().await?;

    Ok(BoardTemplate {
        id: BoardTemplateId::Custom(model.id),
        name: model.name,
        description,
        definition,
    })
}

pub async fn delete_custom_template(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    template_id: i64,
) -> Result<()> {
    BoardTemplateEntity::delete_by_id(template_id)
        .exec(db)
        .await?;
    Ok(())
}

fn built_in(
    id: &'static str,
    name: &str,
    description: &str,
    columns: &[(&str, &[(&str, &str)])],
) -> BoardTemplate {
    BoardTemplate {
        id: BoardTemplateId::BuiltIn(id),
        name: name.to_string(),
        description: description.to_string(),
        definition: BoardTemplateDefinition {
            columns: columns
                .iter()
                .map(|(title, entries)| BoardTemplateColumn {
                    title: (*title).to_string(),
                    entries: entries
                        .iter()
                        .map(|(title, description)| BoardTemplateEntry {
                            title: (*title).to_string(),
                            description: (*description).to_string(),
                        })
                        .collect(),
                })
                .collect(),
        },
    }
}

fn validate_definition(definition: &BoardTemplateDefinition) -> Result<()> {
    if definition.columns.len() > MAX_TEMPLATE_COLUMNS {
        bail!("template has too many columns");
    }
    let entry_count = definition
        .columns
        .iter()
        .map(|column| column.entries.len())
        .sum::<usize>();
    if entry_count > MAX_TEMPLATE_ENTRIES {
        bail!("template has too many cards");
    }
    if definition
        .columns
        .iter()
        .any(|column| column.title.trim().is_empty())
    {
        bail!("template column names cannot be empty");
    }
    if definition.columns.iter().any(|column| {
        column
            .entries
            .iter()
            .any(|entry| entry.title.trim().is_empty())
    }) {
        bail!("template card names cannot be empty");
    }
    Ok(())
}

fn template_summary(column_count: usize, entry_count: usize) -> String {
    let column_label = if column_count == 1 {
        "column"
    } else {
        "columns"
    };
    let entry_label = if entry_count == 1 { "card" } else { "cards" };
    format!("{column_count} {column_label} · {entry_count} {entry_label}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::Database;

    #[test]
    fn built_ins_keep_blank_and_structured_starting_points() {
        let templates = built_in_templates();
        let blank = templates
            .iter()
            .find(|template| template.id == BoardTemplateId::BuiltIn("blank"));
        assert!(blank.is_some_and(|template| template.column_count() == 0));

        let kanban = templates
            .iter()
            .find(|template| template.id == BoardTemplateId::BuiltIn("kanban"));
        assert!(kanban.is_some_and(|template| {
            template
                .definition
                .columns
                .iter()
                .map(|column| column.title.as_str())
                .eq(["To do", "In progress", "Done"])
                && template.entry_count() == 4
        }));
    }

    #[test]
    fn structured_built_ins_include_described_starter_cards() {
        let templates = built_in_templates();
        let structured = templates
            .iter()
            .filter(|template| template.id != BoardTemplateId::BuiltIn("blank"))
            .collect::<Vec<_>>();

        assert_eq!(structured.len(), 5);
        assert!(
            structured
                .iter()
                .all(|template| template.entry_count() >= 4)
        );
        assert!(structured.iter().all(|template| {
            template.definition.columns.iter().all(|column| {
                !column.entries.is_empty()
                    && column
                        .entries
                        .iter()
                        .all(|entry| !entry.description.trim().is_empty())
            })
        }));
    }

    #[tokio::test]
    async fn custom_template_round_trip_preserves_ordered_board_content() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let source = create_board_from_template(
            &db,
            None,
            "Launch".to_string(),
            BoardTemplateDefinition {
                columns: vec![
                    BoardTemplateColumn {
                        title: "Ideas".to_string(),
                        entries: vec![BoardTemplateEntry {
                            title: "Write announcement".to_string(),
                            description: "Keep it concise".to_string(),
                        }],
                    },
                    BoardTemplateColumn {
                        title: "Published".to_string(),
                        entries: Vec::new(),
                    },
                ],
            },
        )
        .await?;

        let saved = save_board_as_template(&db, source.id, "Launch flow".to_string()).await?;
        assert_eq!(saved.column_count(), 2);
        assert_eq!(saved.entry_count(), 1);

        let loaded = load_custom_templates(&db).await?;
        assert_eq!(loaded, vec![saved.clone()]);

        let copy =
            create_board_from_template(&db, None, "Next launch".to_string(), saved.definition)
                .await?;
        let copy_columns = Card::find()
            .filter(card::Column::BoardId.eq(i64::from(copy.id)))
            .order_by_asc(card::Column::Position)
            .all(&db)
            .await?;
        assert_eq!(
            copy_columns
                .iter()
                .map(|column| column.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Ideas", "Published"]
        );

        let BoardTemplateId::Custom(template_id) = saved.id else {
            bail!("saved template should be custom");
        };
        delete_custom_template(&db, template_id).await?;
        assert!(load_custom_templates(&db).await?.is_empty());
        Ok(())
    }
}
