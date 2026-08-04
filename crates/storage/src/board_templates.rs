use anyhow::{Context as _, Result, bail};
use chrono::Utc;
use entity::{
    board::Entity as Board, board_template, board_template::Entity as BoardTemplateEntity, card,
    card::Entity as Card, entry, entry::Entity as Entry,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
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
            "A simple flow for everyday work.",
            &["To do", "In progress", "Done"],
        ),
        built_in(
            "personal-tasks",
            "Personal tasks",
            "Capture tasks, choose what matters, and finish the day clearly.",
            &["Inbox", "This week", "Today", "Done"],
        ),
        built_in(
            "project-plan",
            "Project plan",
            "Move scoped work from planning through review.",
            &["Backlog", "Planned", "In progress", "Review", "Done"],
        ),
        built_in(
            "content-calendar",
            "Content calendar",
            "Track ideas through drafting and publication.",
            &["Ideas", "Drafting", "Review", "Scheduled", "Published"],
        ),
        built_in(
            "bug-triage",
            "Bug triage",
            "Keep reported issues visible from confirmation to resolution.",
            &["Reported", "Confirmed", "In progress", "Verify", "Resolved"],
        ),
    ]
}

pub async fn load_custom_templates(db: &DatabaseConnection) -> Result<Vec<BoardTemplate>> {
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
    db: &DatabaseConnection,
    project_id: Option<u32>,
    title: String,
    definition: BoardTemplateDefinition,
) -> Result<WorkspaceItem> {
    let title = title.trim();
    if title.is_empty() {
        bail!("board name cannot be empty");
    }
    validate_definition(&definition)?;

    let transaction = db.begin().await?;
    let board = entity::board::ActiveModel {
        title: Set(title.to_string()),
        project_id: Set(project_id.map(i64::from)),
        ..Default::default()
    }
    .insert(&transaction)
    .await?;

    for (column_position, template_column) in definition.columns.into_iter().enumerate() {
        let column = card::ActiveModel {
            title: Set(template_column.title),
            board_id: Set(board.id),
            position: Set(column_position as i32),
            ..Default::default()
        }
        .insert(&transaction)
        .await?;

        for (entry_position, template_entry) in template_column.entries.into_iter().enumerate() {
            entry::ActiveModel {
                title: Set(template_entry.title),
                description: Set(template_entry.description),
                card_id: Set(column.id),
                position: Set(entry_position as i32),
                ..Default::default()
            }
            .insert(&transaction)
            .await?;
        }
    }

    transaction.commit().await?;
    Ok(WorkspaceItem {
        id: board.id as u32,
        title: board.title,
    })
}

pub async fn save_board_as_template(
    db: &DatabaseConnection,
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

pub async fn delete_custom_template(db: &DatabaseConnection, template_id: i64) -> Result<()> {
    BoardTemplateEntity::delete_by_id(template_id)
        .exec(db)
        .await?;
    Ok(())
}

fn built_in(id: &'static str, name: &str, description: &str, columns: &[&str]) -> BoardTemplate {
    BoardTemplate {
        id: BoardTemplateId::BuiltIn(id),
        name: name.to_string(),
        description: description.to_string(),
        definition: BoardTemplateDefinition {
            columns: columns
                .iter()
                .map(|title| BoardTemplateColumn {
                    title: (*title).to_string(),
                    entries: Vec::new(),
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
        assert!(templates.iter().any(|template| {
            template.id == BoardTemplateId::BuiltIn("blank") && template.column_count() == 0
        }));
        assert!(templates.iter().any(|template| {
            template.id == BoardTemplateId::BuiltIn("kanban")
                && template
                    .definition
                    .columns
                    .iter()
                    .map(|column| column.title.as_str())
                    .eq(["To do", "In progress", "Done"])
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
