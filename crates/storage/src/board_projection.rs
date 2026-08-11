use std::{cmp::Ordering, collections::HashMap};

use anyhow::{Result, bail};
use chrono::{Duration, Local, NaiveDate};
use entity::{board, board::Entity as Board};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{
    board::{BoardCardRecord, LabelRecord},
    board_properties::{
        BoardViewConfig, DueDatePreset, FilterOperand, FilterOperator, PropertyDefinition,
        PropertyKey, PropertyValue, SortDirection, ViewFilter,
    },
};

pub const BOARD_VIEW_PROJECTION_LIMIT: usize = 100;

#[derive(Clone, Debug, PartialEq)]
pub struct BoardViewProjection {
    pub board_id: i64,
    pub board_title: String,
    pub view_id: Option<i64>,
    pub view_name: Option<String>,
    pub compact_cards: bool,
    pub visible_properties: Vec<PropertyKey>,
    pub lists: Vec<ProjectedList>,
    pub matching_card_count: usize,
    pub remaining_card_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedList {
    pub id: i64,
    pub title: String,
    pub cards: Vec<ProjectedCard>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedCard {
    pub id: i64,
    pub title: String,
    pub due_on: Option<String>,
    pub labels: Vec<String>,
    pub related_note_count: usize,
    pub custom_properties: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoardViewProjectionResult {
    Available(BoardViewProjection),
    MissingBoard,
    MissingView,
}

pub trait BoardViewEntry {
    fn view_id(&self) -> i64;
    fn view_position(&self) -> i32;
    fn view_due_on(&self) -> Option<&str>;
    fn view_has_labels(&self) -> bool;
    fn view_has_any_label(&self, label_ids: &[i64]) -> bool;
    fn view_has_no_labels(&self, label_ids: &[i64]) -> bool;
    fn view_label_sort_key(&self) -> String;
    fn view_related_note_count(&self) -> usize;
}

impl BoardViewEntry for BoardCardRecord {
    fn view_id(&self) -> i64 {
        i64::from(self.id)
    }

    fn view_position(&self) -> i32 {
        self.position
    }

    fn view_due_on(&self) -> Option<&str> {
        self.due_on.as_deref()
    }

    fn view_has_labels(&self) -> bool {
        !self.labels.is_empty()
    }

    fn view_has_any_label(&self, label_ids: &[i64]) -> bool {
        self.labels
            .iter()
            .any(|label| label_ids.contains(&i64::from(label.id)))
    }

    fn view_has_no_labels(&self, label_ids: &[i64]) -> bool {
        self.labels
            .iter()
            .all(|label| !label_ids.contains(&i64::from(label.id)))
    }

    fn view_label_sort_key(&self) -> String {
        self.labels
            .iter()
            .map(|label| label.name.to_lowercase())
            .collect::<Vec<_>>()
            .join("\0")
    }

    fn view_related_note_count(&self) -> usize {
        self.related_notes.len()
    }
}

pub async fn load_board_view_projection(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    board_id: i64,
    view_id: Option<i64>,
) -> Result<BoardViewProjectionResult> {
    let Some(board) = Board::find_by_id(board_id)
        .filter(board::Column::DeletedAt.is_null())
        .one(db)
        .await?
    else {
        return Ok(BoardViewProjectionResult::MissingBoard);
    };
    if board.project_id.is_some() {
        let catalog = crate::workspace_links::load_workspace_link_catalog(db).await?;
        if !catalog.iter().any(|entry| {
            entry.item.kind == crate::workspace_links::WorkspaceItemKind::Board
                && entry.item.id == board_id
        }) {
            return Ok(BoardViewProjectionResult::MissingBoard);
        }
    }
    let views = crate::board_properties::load_board_views(db, board_id).await?;
    let selected_view = match view_id {
        Some(view_id) => {
            let Some(view) = views.views.into_iter().find(|view| view.id == view_id) else {
                return Ok(BoardViewProjectionResult::MissingView);
            };
            Some(view)
        }
        None => None,
    };
    let config = selected_view
        .as_ref()
        .map(|view| view.config.clone())
        .unwrap_or_else(default_projection_config);
    let board_id_u32 = u32::try_from(board_id).map_err(|_| anyhow::anyhow!("invalid board id"))?;
    let snapshot = crate::board::load_board_snapshot(db, board_id_u32).await?;
    let properties = crate::board_properties::load_board_properties(db, board_id).await?;
    let values = properties
        .values
        .iter()
        .map(|value| ((value.entry_id, value.property_id), value.value.clone()))
        .collect::<HashMap<_, _>>();
    let mut matching_card_count = 0usize;
    let mut projected_count = 0usize;
    let mut lists = Vec::new();
    let today = Local::now().date_naive();
    for list in snapshot.cards {
        let mut entries = list
            .entries
            .into_iter()
            .filter(|entry| {
                entry_matches_view(entry, &config, &values, &properties.definitions, today)
            })
            .collect::<Vec<_>>();
        if let Some(sort) = config.sort.as_ref() {
            entries.sort_by(|left, right| {
                compare_entries_for_view(left, right, sort, &values, &properties.definitions)
            });
        }
        matching_card_count = matching_card_count.saturating_add(entries.len());
        let cards = entries
            .into_iter()
            .filter_map(|entry| {
                if projected_count >= BOARD_VIEW_PROJECTION_LIMIT {
                    return None;
                }
                projected_count = projected_count.saturating_add(1);
                Some(projected_card(
                    entry,
                    &config.visible_properties,
                    &values,
                    &properties.definitions,
                ))
            })
            .collect();
        lists.push(ProjectedList {
            id: i64::from(list.id),
            title: list.title,
            cards,
        });
    }
    Ok(BoardViewProjectionResult::Available(BoardViewProjection {
        board_id,
        board_title: board.title,
        view_id,
        view_name: selected_view.map(|view| view.name),
        compact_cards: config.compact_cards,
        visible_properties: config.visible_properties,
        lists,
        matching_card_count,
        remaining_card_count: matching_card_count.saturating_sub(projected_count),
    }))
}

fn default_projection_config() -> BoardViewConfig {
    BoardViewConfig {
        visible_properties: vec![PropertyKey::Labels, PropertyKey::DueDate],
        ..Default::default()
    }
}

pub fn entry_matches_view(
    entry: &impl BoardViewEntry,
    config: &BoardViewConfig,
    values: &HashMap<(i64, i64), PropertyValue>,
    definitions: &[PropertyDefinition],
    today: NaiveDate,
) -> bool {
    config.filters.iter().all(|filter| match &filter.property {
        PropertyKey::Labels => match (&filter.operator, &filter.operand) {
            (FilterOperator::IsEmpty, None) => !entry.view_has_labels(),
            (FilterOperator::IsNotEmpty, None) => entry.view_has_labels(),
            (FilterOperator::IsAnyOf, Some(FilterOperand::LabelIds(ids))) => {
                entry.view_has_any_label(ids)
            }
            (FilterOperator::IsNoneOf, Some(FilterOperand::LabelIds(ids))) => {
                entry.view_has_no_labels(ids)
            }
            _ => false,
        },
        PropertyKey::DueDate => match (&filter.operator, &filter.operand) {
            (FilterOperator::IsEmpty, None) => entry.view_due_on().is_none(),
            (FilterOperator::IsNotEmpty, None) => entry.view_due_on().is_some(),
            (operator, Some(FilterOperand::Date(expected))) => entry
                .view_due_on()
                .is_some_and(|value| compare_date(value, expected, *operator)),
            (FilterOperator::IsAnyOf, Some(FilterOperand::DueDatePresets(presets))) => presets
                .iter()
                .any(|preset| matches_due_preset(*preset, entry.view_due_on(), today)),
            (FilterOperator::IsNoneOf, Some(FilterOperand::DueDatePresets(presets))) => presets
                .iter()
                .all(|preset| !matches_due_preset(*preset, entry.view_due_on(), today)),
            _ => false,
        },
        PropertyKey::RelatedNotes => match filter.operator {
            FilterOperator::IsEmpty => entry.view_related_note_count() == 0,
            FilterOperator::IsNotEmpty => entry.view_related_note_count() > 0,
            _ => false,
        },
        PropertyKey::Custom(property_id) => matches_custom_filter(
            filter,
            values.get(&(entry.view_id(), *property_id)),
            definitions
                .iter()
                .find(|definition| definition.id == *property_id),
        ),
    })
}

fn matches_custom_filter(
    filter: &ViewFilter,
    value: Option<&PropertyValue>,
    definition: Option<&PropertyDefinition>,
) -> bool {
    match filter.operator {
        FilterOperator::IsEmpty => return value.is_none(),
        FilterOperator::IsNotEmpty => return value.is_some(),
        FilterOperator::IsChecked => return matches!(value, Some(PropertyValue::Checkbox(true))),
        FilterOperator::IsUnchecked => {
            return matches!(value, Some(PropertyValue::Checkbox(false)));
        }
        _ => {}
    }
    match (value, filter.operand.as_ref()) {
        (
            Some(PropertyValue::Text(value) | PropertyValue::Url(value)),
            Some(FilterOperand::Text(expected)),
        ) => compare_text(value, expected, filter.operator),
        (Some(PropertyValue::Number(value)), Some(FilterOperand::Number(expected))) => {
            match filter.operator {
                FilterOperator::Equals => value == expected,
                FilterOperator::GreaterThan => value > expected,
                FilterOperator::LessThan => value < expected,
                _ => false,
            }
        }
        (Some(PropertyValue::Date(value)), Some(FilterOperand::Date(expected))) => {
            compare_date(value, expected, filter.operator)
        }
        (Some(PropertyValue::Select(option_id)), Some(FilterOperand::OptionIds(ids))) => {
            match filter.operator {
                FilterOperator::IsAnyOf | FilterOperator::Equals => ids.contains(option_id),
                FilterOperator::IsNoneOf => !ids.contains(option_id),
                _ => false,
            }
        }
        (Some(PropertyValue::Select(option_id)), Some(FilterOperand::Text(expected))) => definition
            .and_then(|definition| {
                definition
                    .options
                    .iter()
                    .find(|option| option.id == *option_id)
            })
            .is_some_and(|option| compare_text(&option.name, expected, filter.operator)),
        _ => false,
    }
}

fn compare_text(value: &str, expected: &str, operator: FilterOperator) -> bool {
    let value = value.to_lowercase();
    let expected = expected.to_lowercase();
    match operator {
        FilterOperator::Contains => value.contains(&expected),
        FilterOperator::DoesNotContain => !value.contains(&expected),
        FilterOperator::Equals => value == expected,
        _ => false,
    }
}

fn compare_date(value: &str, expected: &str, operator: FilterOperator) -> bool {
    match operator {
        FilterOperator::Before => value < expected,
        FilterOperator::On | FilterOperator::Equals => value == expected,
        FilterOperator::After => value > expected,
        _ => false,
    }
}

fn matches_due_preset(preset: DueDatePreset, due_on: Option<&str>, today: NaiveDate) -> bool {
    match (preset, due_on) {
        (DueDatePreset::NoDueDate, None) => true,
        (DueDatePreset::NoDueDate, Some(_)) | (_, None) => false,
        (DueDatePreset::Overdue, Some(value)) => parse_date(value).is_some_and(|date| date < today),
        (DueDatePreset::Today, Some(value)) => parse_date(value) == Some(today),
        (DueDatePreset::NextSevenDays, Some(value)) => {
            parse_date(value).is_some_and(|date| date > today && date <= today + Duration::days(7))
        }
    }
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

pub fn compare_entries_for_view(
    left: &impl BoardViewEntry,
    right: &impl BoardViewEntry,
    sort: &crate::board_properties::ViewSort,
    values: &HashMap<(i64, i64), PropertyValue>,
    definitions: &[PropertyDefinition],
) -> Ordering {
    let left_value = sort_value(left, &sort.property, values, definitions);
    let right_value = sort_value(right, &sort.property, values, definitions);
    let ordering = match (&left_value, &right_value) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(left), Some(right)) => left.compare(right),
    };
    let ordering = if sort.direction == SortDirection::Descending
        && left_value.is_some()
        && right_value.is_some()
    {
        ordering.reverse()
    } else {
        ordering
    };
    ordering.then_with(|| {
        (left.view_position(), left.view_id()).cmp(&(right.view_position(), right.view_id()))
    })
}

enum SortValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

impl SortValue {
    fn compare(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Text(left), Self::Text(right)) => left.cmp(right),
            (Self::Number(left), Self::Number(right)) => {
                left.partial_cmp(right).unwrap_or(Ordering::Equal)
            }
            (Self::Bool(left), Self::Bool(right)) => left.cmp(right),
            _ => Ordering::Equal,
        }
    }
}

fn sort_value(
    entry: &impl BoardViewEntry,
    property: &PropertyKey,
    values: &HashMap<(i64, i64), PropertyValue>,
    definitions: &[PropertyDefinition],
) -> Option<SortValue> {
    match property {
        PropertyKey::DueDate => entry
            .view_due_on()
            .map(|value| SortValue::Text(value.to_string())),
        PropertyKey::Labels => entry
            .view_has_labels()
            .then(|| SortValue::Text(entry.view_label_sort_key())),
        PropertyKey::RelatedNotes => {
            Some(SortValue::Number(entry.view_related_note_count() as f64))
        }
        PropertyKey::Custom(property_id) => {
            values
                .get(&(entry.view_id(), *property_id))
                .map(|value| match value {
                    PropertyValue::Text(value)
                    | PropertyValue::Date(value)
                    | PropertyValue::Url(value) => SortValue::Text(value.to_lowercase()),
                    PropertyValue::Number(value) => SortValue::Number(*value),
                    PropertyValue::Checkbox(value) => SortValue::Bool(*value),
                    PropertyValue::Select(option_id) => SortValue::Text(
                        definitions
                            .iter()
                            .flat_map(|definition| definition.options.iter())
                            .find(|option| option.id == *option_id)
                            .map(|option| option.name.to_lowercase())
                            .unwrap_or_default(),
                    ),
                })
        }
    }
}

fn projected_card(
    entry: BoardCardRecord,
    visible: &[PropertyKey],
    values: &HashMap<(i64, i64), PropertyValue>,
    definitions: &[PropertyDefinition],
) -> ProjectedCard {
    let custom_properties = visible
        .iter()
        .filter_map(|key| {
            let PropertyKey::Custom(property_id) = key else {
                return None;
            };
            let definition = definitions
                .iter()
                .find(|definition| definition.id == *property_id)?;
            let value = values.get(&(i64::from(entry.id), *property_id))?;
            Some((
                definition.name.clone(),
                property_value_label(value, definition),
            ))
        })
        .collect();
    ProjectedCard {
        id: i64::from(entry.id),
        title: entry.title,
        due_on: entry.due_on,
        labels: entry
            .labels
            .into_iter()
            .map(|label: LabelRecord| label.name)
            .collect(),
        related_note_count: entry.related_notes.len(),
        custom_properties,
    }
}

fn property_value_label(value: &PropertyValue, definition: &PropertyDefinition) -> String {
    match value {
        PropertyValue::Text(value) | PropertyValue::Date(value) | PropertyValue::Url(value) => {
            value.clone()
        }
        PropertyValue::Number(value) => value.to_string(),
        PropertyValue::Checkbox(value) => if *value { "Checked" } else { "Unchecked" }.to_string(),
        PropertyValue::Select(option_id) => definition
            .options
            .iter()
            .find(|option| option.id == *option_id)
            .map(|option| option.name.clone())
            .unwrap_or_else(|| "Unavailable".to_string()),
    }
}

pub fn parse_embed_config(source: &str) -> Result<(i64, Option<i64>, Option<String>)> {
    let mut board_id = None;
    let mut view_id = None;
    let mut title = None;
    for line in source.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "board" => board_id = Some(value.trim().parse()?),
            "view" => view_id = Some(value.trim().parse()?),
            "title" => {
                let value = value.trim();
                title = Some(
                    value
                        .strip_prefix('"')
                        .and_then(|value| value.strip_suffix('"'))
                        .unwrap_or(value)
                        .to_string(),
                );
            }
            _ => {}
        }
    }
    let Some(board_id) = board_id else {
        bail!("castle-board-view embed requires board");
    };
    Ok((board_id, view_id, title))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardViewEmbed {
    pub board_id: i64,
    pub view_id: Option<i64>,
    pub fallback_title: Option<String>,
    pub start_byte: usize,
    pub end_byte: usize,
}

pub fn parse_board_view_embeds(content: &str) -> Vec<BoardViewEmbed> {
    let lines = content.split_inclusive('\n').collect::<Vec<_>>();
    let mut embeds = Vec::new();
    let mut offset = 0usize;
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if line.trim_end_matches(['\r', '\n']) != "```castle-board-view" {
            offset = offset.saturating_add(line.len());
            index += 1;
            continue;
        }
        let start_byte = offset;
        let mut source = String::new();
        offset = offset.saturating_add(line.len());
        index += 1;
        let mut closed = false;
        while index < lines.len() {
            let line = lines[index];
            offset = offset.saturating_add(line.len());
            index += 1;
            if line.trim_end_matches(['\r', '\n']) == "```" {
                closed = true;
                break;
            }
            source.push_str(line);
        }
        if closed && let Ok((board_id, view_id, fallback_title)) = parse_embed_config(&source) {
            embeds.push(BoardViewEmbed {
                board_id,
                view_id,
                fallback_title,
                start_byte,
                end_byte: offset,
            });
        }
    }
    embeds
}

#[cfg(test)]
mod tests {
    use super::{
        BOARD_VIEW_PROJECTION_LIMIT, BoardViewProjectionResult, load_board_view_projection,
        parse_embed_config,
    };
    use anyhow::Result;
    use entity::{board, card, entry, note};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};

    #[test]
    fn parses_readable_embed_config() {
        assert_eq!(
            parse_embed_config("board = 12\nview = 4\ntitle = \"Roadmap · Current\"")
                .expect("config should parse"),
            (12, Some(4), Some("Roadmap · Current".to_string()))
        );
    }

    #[test]
    fn recognizes_only_exact_closed_castle_fences() {
        let content = "before\n```castle-board-view\nboard = 12\n```\n```rust\nboard = 1\n```\n";
        let embeds = super::parse_board_view_embeds(content);
        assert_eq!(embeds.len(), 1);
        assert_eq!(embeds[0].board_id, 12);
        assert_eq!(
            &content[embeds[0].start_byte..embeds[0].end_byte],
            "```castle-board-view\nboard = 12\n```\n"
        );
    }

    #[tokio::test]
    async fn projection_reports_missing_targets_and_bounds_results() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        assert_eq!(
            load_board_view_projection(&db, 404, None).await?,
            BoardViewProjectionResult::MissingBoard
        );

        let board = board::ActiveModel {
            title: Set("Roadmap".to_string()),
            last_selected_view_id: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let list = card::ActiveModel {
            title: Set("Ideas".to_string()),
            board_id: Set(board.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let mut first_entry_id = None;
        for position in 0..=BOARD_VIEW_PROJECTION_LIMIT {
            let inserted = entry::ActiveModel {
                title: Set(format!("Card {position}")),
                description: Set(String::new()),
                card_id: Set(list.id),
                position: Set(i32::try_from(position)?),
                reminder_enabled: Set(false),
                ..Default::default()
            }
            .insert(&db)
            .await?;
            first_entry_id.get_or_insert(inserted.id);
        }

        assert_eq!(
            load_board_view_projection(&db, board.id, Some(404)).await?,
            BoardViewProjectionResult::MissingView
        );
        let BoardViewProjectionResult::Available(projection) =
            load_board_view_projection(&db, board.id, None).await?
        else {
            anyhow::bail!("projection should be available");
        };
        assert_eq!(
            projection.matching_card_count,
            BOARD_VIEW_PROJECTION_LIMIT + 1
        );
        assert_eq!(projection.remaining_card_count, 1);
        assert_eq!(projection.lists[0].cards.len(), BOARD_VIEW_PROJECTION_LIMIT);

        let note = note::ActiveModel {
            title: Set("Research".to_string()),
            cached_content: Set(String::new()),
            file_managed_by_app: Set(false),
            created_at: Set(0),
            updated_at: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let first_entry_id = first_entry_id.ok_or_else(|| anyhow::anyhow!("missing test card"))?;
        crate::workspace_links::link_note_to_item(
            &db,
            note.id,
            crate::workspace_links::WorkspaceItemRef {
                kind: crate::workspace_links::WorkspaceItemKind::Card,
                id: first_entry_id,
            },
            0,
        )
        .await?;
        let related_view = crate::board_properties::create_board_view(
            &db,
            board.id,
            "With notes".to_string(),
            crate::board_properties::BoardViewConfig {
                filters: vec![crate::board_properties::ViewFilter {
                    property: crate::board_properties::PropertyKey::RelatedNotes,
                    operator: crate::board_properties::FilterOperator::IsNotEmpty,
                    operand: None,
                }],
                ..Default::default()
            },
        )
        .await?;
        let BoardViewProjectionResult::Available(related_projection) =
            load_board_view_projection(&db, board.id, Some(related_view.id)).await?
        else {
            anyhow::bail!("saved view projection should be available");
        };
        assert_eq!(related_projection.matching_card_count, 1);
        assert_eq!(related_projection.lists[0].cards[0].related_note_count, 1);
        Ok(())
    }
}
