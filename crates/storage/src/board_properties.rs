use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result, bail};
use chrono::{NaiveDate, Utc};
use entity::{
    board, board::Entity as Board, board_label, board_label::Entity as BoardLabel, board_property,
    board_property::Entity as BoardProperty, board_property_option,
    board_property_option::Entity as BoardPropertyOption, card, card::Entity as Card, entry,
    entry::Entity as Entry, entry_property_value,
    entry_property_value::Entity as EntryPropertyValue, saved_board_view,
    saved_board_view::Entity as SavedBoardView,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, TransactionTrait,
};
use serde::{Deserialize, Serialize};

pub const BOARD_VIEW_CONFIG_VERSION: i32 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyKind {
    Text,
    Number,
    Checkbox,
    Date,
    Select,
    Url,
}

impl PropertyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Checkbox => "checkbox",
            Self::Date => "date",
            Self::Select => "select",
            Self::Url => "url",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "text" => Ok(Self::Text),
            "number" => Ok(Self::Number),
            "checkbox" => Ok(Self::Checkbox),
            "date" => Ok(Self::Date),
            "select" => Ok(Self::Select),
            "url" => Ok(Self::Url),
            _ => bail!("unknown board property kind {value:?}"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PropertyValue {
    Text(String),
    Number(f64),
    Checkbox(bool),
    Date(String),
    Select(i64),
    Url(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyDefinition {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub kind: PropertyKind,
    pub position: i32,
    pub options: Vec<PropertyOption>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyOption {
    pub id: i64,
    pub property_id: i64,
    pub name: String,
    pub color: String,
    pub position: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntryProperty {
    pub entry_id: i64,
    pub property_id: i64,
    pub value: PropertyValue,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardProperties {
    pub definitions: Vec<PropertyDefinition>,
    pub values: Vec<EntryProperty>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum PropertyKey {
    DueDate,
    Labels,
    Custom(i64),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Contains,
    DoesNotContain,
    Equals,
    GreaterThan,
    LessThan,
    Before,
    On,
    After,
    IsAnyOf,
    IsNoneOf,
    IsEmpty,
    IsNotEmpty,
    IsChecked,
    IsUnchecked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DueDatePreset {
    Overdue,
    Today,
    NextSevenDays,
    NoDueDate,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum FilterOperand {
    Text(String),
    Number(f64),
    Date(String),
    OptionIds(Vec<i64>),
    LabelIds(Vec<i64>),
    DueDatePresets(Vec<DueDatePreset>),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ViewFilter {
    pub property: PropertyKey,
    pub operator: FilterOperator,
    pub operand: Option<FilterOperand>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ViewSort {
    pub property: PropertyKey,
    pub direction: SortDirection,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BoardViewConfig {
    #[serde(default)]
    pub filters: Vec<ViewFilter>,
    pub sort: Option<ViewSort>,
    #[serde(default)]
    pub visible_properties: Vec<PropertyKey>,
    #[serde(default)]
    pub compact_cards: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoardView {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub position: i32,
    pub is_default: bool,
    pub config: BoardViewConfig,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardViews {
    pub views: Vec<BoardView>,
    pub selected_view_id: Option<i64>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeletionImpact {
    pub value_count: usize,
    pub view_count: usize,
}

pub async fn load_board_properties(
    db: &DatabaseConnection,
    board_id: i64,
) -> Result<BoardProperties> {
    active_board(db, board_id).await?;
    let definitions = BoardProperty::find()
        .filter(board_property::Column::BoardId.eq(board_id))
        .filter(board_property::Column::DeletedAt.is_null())
        .order_by_asc(board_property::Column::Position)
        .order_by_asc(board_property::Column::Id)
        .all(db)
        .await?;

    let property_ids = definitions
        .iter()
        .map(|property| property.id)
        .collect::<Vec<_>>();

    let options = if property_ids.is_empty() {
        Vec::new()
    } else {
        BoardPropertyOption::find()
            .filter(board_property_option::Column::PropertyId.is_in(property_ids.clone()))
            .filter(board_property_option::Column::DeletedAt.is_null())
            .order_by_asc(board_property_option::Column::Position)
            .order_by_asc(board_property_option::Column::Id)
            .all(db)
            .await?
    };

    let mut options_by_property: HashMap<i64, Vec<PropertyOption>> = HashMap::new();
    for option in options {
        options_by_property
            .entry(option.property_id)
            .or_default()
            .push(property_option(option));
    }

    let definitions = definitions
        .into_iter()
        .map(|property| {
            Ok(PropertyDefinition {
                id: property.id,
                board_id: property.board_id,
                name: property.name,
                kind: PropertyKind::parse(&property.kind)?,
                position: property.position,
                options: options_by_property.remove(&property.id).unwrap_or_default(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let entry_ids = active_entry_ids(db, board_id).await?;
    let value_models = if entry_ids.is_empty() || property_ids.is_empty() {
        Vec::new()
    } else {
        EntryPropertyValue::find()
            .filter(entry_property_value::Column::EntryId.is_in(entry_ids))
            .filter(entry_property_value::Column::PropertyId.is_in(property_ids))
            .all(db)
            .await?
    };

    let kinds = definitions
        .iter()
        .map(|property| (property.id, property.kind))
        .collect::<HashMap<_, _>>();

    let mut values = Vec::with_capacity(value_models.len());
    let mut warnings = Vec::new();
    for value in value_models {
        let Some(kind) = kinds.get(&value.property_id).copied() else {
            warnings.push(format!(
                "Property value for card {} references unavailable property {}",
                value.entry_id, value.property_id
            ));
            continue;
        };
        let entry_id = value.entry_id;
        let property_id = value.property_id;
        match decode_value(value, kind) {
            Ok(value) => values.push(value),
            Err(error) => warnings.push(format!(
                "Could not read property {property_id} on card {entry_id}: {error}"
            )),
        }
    }

    Ok(BoardProperties {
        definitions,
        values,
        warnings,
    })
}

pub async fn create_property(
    db: &DatabaseConnection,
    board_id: i64,
    name: String,
    kind: PropertyKind,
) -> Result<PropertyDefinition> {
    active_board(db, board_id).await?;
    let name = required_text(name, "property name")?;
    let existing = BoardProperty::find()
        .filter(board_property::Column::BoardId.eq(board_id))
        .filter(board_property::Column::DeletedAt.is_null())
        .all(db)
        .await?;

    if existing
        .iter()
        .any(|property| property.name.eq_ignore_ascii_case(&name))
    {
        bail!("board property {name:?} already exists");
    }
    let position = existing.len() as i32;
    let property = board_property::ActiveModel {
        board_id: Set(board_id),
        name: Set(name),
        kind: Set(kind.as_str().to_string()),
        position: Set(position),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(PropertyDefinition {
        id: property.id,
        board_id,
        name: property.name,
        kind,
        position,
        options: Vec::new(),
    })
}

pub async fn create_property_option(
    db: &DatabaseConnection,
    property_id: i64,
    name: String,
    color: String,
) -> Result<PropertyOption> {
    let property = active_property(db, property_id).await?;
    if PropertyKind::parse(&property.kind)? != PropertyKind::Select {
        bail!("only select properties can contain options");
    }
    let name = required_text(name, "option name")?;
    let color = required_text(color, "option color")?;
    let existing = BoardPropertyOption::find()
        .filter(board_property_option::Column::PropertyId.eq(property_id))
        .filter(board_property_option::Column::DeletedAt.is_null())
        .all(db)
        .await?;

    if existing
        .iter()
        .any(|option| option.name.eq_ignore_ascii_case(&name))
    {
        bail!("select option {name:?} already exists");
    }

    let option = board_property_option::ActiveModel {
        property_id: Set(property_id),
        name: Set(name),
        color: Set(color),
        position: Set(existing.len() as i32),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await?;

    Ok(property_option(option))
}

pub async fn rename_property(
    db: &DatabaseConnection,
    property_id: i64,
    name: String,
) -> Result<PropertyDefinition> {
    let property = active_property(db, property_id).await?;
    let name = required_text(name, "property name")?;
    let duplicates = BoardProperty::find()
        .filter(board_property::Column::BoardId.eq(property.board_id))
        .filter(board_property::Column::DeletedAt.is_null())
        .all(db)
        .await?;
    if duplicates
        .iter()
        .any(|candidate| candidate.id != property_id && candidate.name.eq_ignore_ascii_case(&name))
    {
        bail!("board property {name:?} already exists");
    }
    let mut active = property.into_active_model();
    active.name = Set(name);
    let property = active.update(db).await?;
    let mut loaded = load_board_properties(db, property.board_id).await?;
    loaded
        .definitions
        .drain(..)
        .find(|definition| definition.id == property_id)
        .with_context(|| format!("updated property {property_id} was not found"))
}

pub async fn reorder_properties(
    db: &DatabaseConnection,
    board_id: i64,
    ordered_ids: &[i64],
) -> Result<()> {
    active_board(db, board_id).await?;
    let properties = BoardProperty::find()
        .filter(board_property::Column::BoardId.eq(board_id))
        .filter(board_property::Column::DeletedAt.is_null())
        .all(db)
        .await?;
    validate_reorder_ids(
        properties.iter().map(|property| property.id),
        ordered_ids,
        "properties",
    )?;
    let transaction = db.begin().await?;
    let by_id = properties
        .into_iter()
        .map(|property| (property.id, property))
        .collect::<HashMap<_, _>>();
    for (position, id) in ordered_ids.iter().enumerate() {
        let property = by_id
            .get(id)
            .cloned()
            .with_context(|| format!("property {id} was not found"))?;
        let mut active = property.into_active_model();
        active.position = Set(position as i32);
        active.update(&transaction).await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn property_deletion_impact(
    db: &DatabaseConnection,
    property_id: i64,
) -> Result<DeletionImpact> {
    let property = active_property(db, property_id).await?;
    let value_count = EntryPropertyValue::find()
        .filter(entry_property_value::Column::PropertyId.eq(property_id))
        .all(db)
        .await?
        .len();
    let view_count = load_board_views(db, property.board_id)
        .await?
        .views
        .iter()
        .filter(|view| view_references_property(&view.config, property_id))
        .count();
    Ok(DeletionImpact {
        value_count,
        view_count,
    })
}

pub async fn delete_property(db: &DatabaseConnection, property_id: i64) -> Result<()> {
    let property = active_property(db, property_id).await?;
    let transaction = db.begin().await?;
    let mut active = property.clone().into_active_model();
    active.deleted_at = Set(Some(Utc::now().timestamp()));
    active.update(&transaction).await?;
    clean_property_from_views(&transaction, property.board_id, property_id).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn rename_property_option(
    db: &DatabaseConnection,
    option_id: i64,
    name: String,
) -> Result<PropertyOption> {
    let option = active_property_option(db, option_id).await?;
    let name = required_text(name, "option name")?;
    let duplicates = BoardPropertyOption::find()
        .filter(board_property_option::Column::PropertyId.eq(option.property_id))
        .filter(board_property_option::Column::DeletedAt.is_null())
        .all(db)
        .await?;
    if duplicates
        .iter()
        .any(|candidate| candidate.id != option_id && candidate.name.eq_ignore_ascii_case(&name))
    {
        bail!("select option {name:?} already exists");
    }
    let mut active = option.into_active_model();
    active.name = Set(name);
    Ok(property_option(active.update(db).await?))
}

pub async fn update_property_option_color(
    db: &DatabaseConnection,
    option_id: i64,
    color: String,
) -> Result<PropertyOption> {
    let option = active_property_option(db, option_id).await?;
    let color = required_text(color, "option color")?;
    let mut active = option.into_active_model();
    active.color = Set(color);
    Ok(property_option(active.update(db).await?))
}

pub async fn reorder_property_options(
    db: &DatabaseConnection,
    property_id: i64,
    ordered_ids: &[i64],
) -> Result<()> {
    let property = active_property(db, property_id).await?;
    if PropertyKind::parse(&property.kind)? != PropertyKind::Select {
        bail!("only select properties can contain options");
    }
    let options = BoardPropertyOption::find()
        .filter(board_property_option::Column::PropertyId.eq(property_id))
        .filter(board_property_option::Column::DeletedAt.is_null())
        .all(db)
        .await?;
    validate_reorder_ids(
        options.iter().map(|option| option.id),
        ordered_ids,
        "property options",
    )?;
    let transaction = db.begin().await?;
    let by_id = options
        .into_iter()
        .map(|option| (option.id, option))
        .collect::<HashMap<_, _>>();
    for (position, id) in ordered_ids.iter().enumerate() {
        let option = by_id
            .get(id)
            .cloned()
            .with_context(|| format!("property option {id} was not found"))?;
        let mut active = option.into_active_model();
        active.position = Set(position as i32);
        active.update(&transaction).await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn delete_property_option(db: &DatabaseConnection, option_id: i64) -> Result<()> {
    let option = active_property_option(db, option_id).await?;
    let property = active_property(db, option.property_id).await?;
    let transaction = db.begin().await?;
    EntryPropertyValue::delete_many()
        .filter(entry_property_value::Column::PropertyId.eq(option.property_id))
        .filter(entry_property_value::Column::OptionId.eq(option_id))
        .exec(&transaction)
        .await?;
    let mut active = option.into_active_model();
    active.deleted_at = Set(Some(Utc::now().timestamp()));
    active.update(&transaction).await?;
    clean_option_from_views(&transaction, property.board_id, option_id).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn set_entry_property(
    db: &DatabaseConnection,
    entry_id: i64,
    property_id: i64,
    value: PropertyValue,
) -> Result<EntryProperty> {
    let property = active_property(db, property_id).await?;
    let entry_board_id = entry_board_id(db, entry_id).await?;
    if property.board_id != entry_board_id {
        bail!(
            "property {} belongs to board {}, but entry {} belongs to board {}",
            property.id,
            property.board_id,
            entry_id,
            entry_board_id
        );
    }
    let kind = PropertyKind::parse(&property.kind)?;
    validate_value(db, property_id, kind, &value).await?;
    let mut active = entry_property_value::ActiveModel {
        entry_id: Set(entry_id),
        property_id: Set(property_id),
        text_value: Set(None),
        number_value: Set(None),
        boolean_value: Set(None),
        date_value: Set(None),
        option_id: Set(None),
    };
    match &value {
        PropertyValue::Text(value) | PropertyValue::Url(value) => {
            active.text_value = Set(Some(value.clone()));
        }
        PropertyValue::Number(value) => active.number_value = Set(Some(*value)),
        PropertyValue::Checkbox(value) => active.boolean_value = Set(Some(*value)),
        PropertyValue::Date(value) => active.date_value = Set(Some(value.clone())),
        PropertyValue::Select(option_id) => active.option_id = Set(Some(*option_id)),
    }
    let transaction = db.begin().await?;
    EntryPropertyValue::delete_by_id((entry_id, property_id))
        .exec(&transaction)
        .await?;
    active.insert(&transaction).await?;
    transaction.commit().await?;
    Ok(EntryProperty {
        entry_id,
        property_id,
        value,
    })
}

pub async fn clear_entry_property(
    db: &DatabaseConnection,
    entry_id: i64,
    property_id: i64,
) -> Result<()> {
    let property = active_property(db, property_id).await?;
    if property.board_id != entry_board_id(db, entry_id).await? {
        bail!("entry and property belong to different boards");
    }
    EntryPropertyValue::delete_by_id((entry_id, property_id))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn load_board_views(db: &DatabaseConnection, board_id: i64) -> Result<BoardViews> {
    let board = active_board(db, board_id).await?;
    let models = SavedBoardView::find()
        .filter(saved_board_view::Column::BoardId.eq(board_id))
        .filter(saved_board_view::Column::DeletedAt.is_null())
        .order_by_asc(saved_board_view::Column::Position)
        .order_by_asc(saved_board_view::Column::Id)
        .all(db)
        .await?;
    let mut views = Vec::with_capacity(models.len());
    let mut warnings = Vec::new();
    for model in models {
        let name = model.name.clone();
        match decode_board_view(model) {
            Ok(view) => views.push(view),
            Err(error) => warnings.push(format!("Could not load view {name:?}: {error}")),
        }
    }
    let selected_view_id = match board.last_selected_view_id {
        0 => None,
        view_id if views.iter().any(|view| view.id == view_id) => Some(view_id),
        _ => views
            .iter()
            .find(|view| view.is_default)
            .or_else(|| views.first())
            .map(|view| view.id),
    };
    Ok(BoardViews {
        views,
        selected_view_id,
        warnings,
    })
}

pub async fn set_selected_board_view(
    db: &DatabaseConnection,
    board_id: i64,
    view_id: Option<i64>,
) -> Result<()> {
    let board = active_board(db, board_id).await?;
    if let Some(view_id) = view_id {
        let view = active_board_view(db, view_id).await?;
        if view.board_id != board_id {
            bail!("board view {view_id} does not belong to board {board_id}");
        }
    }
    let mut active = board.into_active_model();
    active.last_selected_view_id = Set(view_id.map_or(0, |view_id| view_id));
    active.update(db).await?;
    Ok(())
}

pub async fn create_board_view(
    db: &DatabaseConnection,
    board_id: i64,
    name: String,
    config: BoardViewConfig,
) -> Result<BoardView> {
    active_board(db, board_id).await?;
    validate_view_config(db, board_id, &config).await?;
    let name = required_text(name, "view name")?;
    let existing = SavedBoardView::find()
        .filter(saved_board_view::Column::BoardId.eq(board_id))
        .filter(saved_board_view::Column::DeletedAt.is_null())
        .all(db)
        .await?;
    if existing
        .iter()
        .any(|view| view.name.eq_ignore_ascii_case(&name))
    {
        bail!("board view {name:?} already exists");
    }
    let model = saved_board_view::ActiveModel {
        board_id: Set(board_id),
        name: Set(name),
        position: Set(existing.len() as i32),
        is_default: Set(false),
        config_version: Set(BOARD_VIEW_CONFIG_VERSION),
        config_json: Set(serde_json::to_string(&config)?),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await?;
    decode_board_view(model)
}

pub async fn rename_board_view(
    db: &DatabaseConnection,
    view_id: i64,
    name: String,
) -> Result<BoardView> {
    let view = active_board_view(db, view_id).await?;
    let name = required_text(name, "view name")?;
    ensure_unique_view_name(db, view.board_id, Some(view_id), &name).await?;
    let mut active = view.into_active_model();
    active.name = Set(name);
    decode_board_view(active.update(db).await?)
}

pub async fn update_board_view(
    db: &DatabaseConnection,
    view_id: i64,
    config: BoardViewConfig,
) -> Result<BoardView> {
    let view = active_board_view(db, view_id).await?;
    validate_view_config(db, view.board_id, &config).await?;
    let mut active = view.into_active_model();
    active.config_version = Set(BOARD_VIEW_CONFIG_VERSION);
    active.config_json = Set(serde_json::to_string(&config)?);
    decode_board_view(active.update(db).await?)
}

pub async fn delete_board_view(db: &DatabaseConnection, view_id: i64) -> Result<()> {
    let view = active_board_view(db, view_id).await?;
    let was_default = view.is_default;
    let board_id = view.board_id;
    let mut active = view.into_active_model();
    active.deleted_at = Set(Some(Utc::now().timestamp()));
    active.is_default = Set(false);
    active.update(db).await?;
    let board = active_board(db, board_id).await?;
    if board.last_selected_view_id == view_id {
        set_selected_board_view(db, board_id, None).await?;
    }
    if was_default
        && let Some(next) = SavedBoardView::find()
            .filter(saved_board_view::Column::BoardId.eq(board_id))
            .filter(saved_board_view::Column::DeletedAt.is_null())
            .order_by_asc(saved_board_view::Column::Position)
            .order_by_asc(saved_board_view::Column::Id)
            .one(db)
            .await?
    {
        set_default_board_view(db, next.id).await?;
    }
    Ok(())
}

pub async fn reorder_board_views(
    db: &DatabaseConnection,
    board_id: i64,
    ordered_ids: &[i64],
) -> Result<()> {
    active_board(db, board_id).await?;
    let views = SavedBoardView::find()
        .filter(saved_board_view::Column::BoardId.eq(board_id))
        .filter(saved_board_view::Column::DeletedAt.is_null())
        .all(db)
        .await?;
    validate_reorder_ids(views.iter().map(|view| view.id), ordered_ids, "board views")?;
    let transaction = db.begin().await?;
    let by_id = views
        .into_iter()
        .map(|view| (view.id, view))
        .collect::<HashMap<_, _>>();
    for (position, id) in ordered_ids.iter().enumerate() {
        let view = by_id
            .get(id)
            .cloned()
            .with_context(|| format!("board view {id} was not found"))?;
        let mut active = view.into_active_model();
        active.position = Set(position as i32);
        active.update(&transaction).await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn set_default_board_view(db: &DatabaseConnection, view_id: i64) -> Result<BoardView> {
    let view = active_board_view(db, view_id).await?;
    let transaction = db.begin().await?;
    let views = SavedBoardView::find()
        .filter(saved_board_view::Column::BoardId.eq(view.board_id))
        .filter(saved_board_view::Column::DeletedAt.is_null())
        .all(&transaction)
        .await?;
    for candidate in views {
        let should_be_default = candidate.id == view_id;
        if candidate.is_default != should_be_default {
            let mut active = candidate.into_active_model();
            active.is_default = Set(should_be_default);
            active.update(&transaction).await?;
        }
    }
    transaction.commit().await?;
    let updated = active_board_view(db, view_id).await?;
    decode_board_view(updated)
}

async fn validate_value(
    db: &DatabaseConnection,
    property_id: i64,
    kind: PropertyKind,
    value: &PropertyValue,
) -> Result<()> {
    match (kind, value) {
        (PropertyKind::Text, PropertyValue::Text(_))
        | (PropertyKind::Checkbox, PropertyValue::Checkbox(_))
        | (PropertyKind::Url, PropertyValue::Url(_)) => Ok(()),
        (PropertyKind::Number, PropertyValue::Number(value)) if value.is_finite() => Ok(()),
        (PropertyKind::Date, PropertyValue::Date(value)) => {
            NaiveDate::parse_from_str(value, "%Y-%m-%d").with_context(|| {
                format!("property date must use YYYY-MM-DD, received {value:?}")
            })?;
            Ok(())
        }
        (PropertyKind::Select, PropertyValue::Select(option_id)) => {
            let valid = BoardPropertyOption::find_by_id(*option_id)
                .filter(board_property_option::Column::PropertyId.eq(property_id))
                .filter(board_property_option::Column::DeletedAt.is_null())
                .one(db)
                .await?
                .is_some();
            if !valid {
                bail!("select option {option_id} does not belong to property {property_id}");
            }
            Ok(())
        }
        _ => bail!("property value does not match {} property", kind.as_str()),
    }
}

async fn validate_view_config(
    db: &DatabaseConnection,
    board_id: i64,
    config: &BoardViewConfig,
) -> Result<()> {
    if config.visible_properties.len() > 3 {
        bail!("a board view can show at most three card fields");
    }
    let active_properties = BoardProperty::find()
        .filter(board_property::Column::BoardId.eq(board_id))
        .filter(board_property::Column::DeletedAt.is_null())
        .all(db)
        .await?;
    let active_ids = active_properties
        .iter()
        .map(|property| property.id)
        .collect::<HashSet<_>>();
    let referenced = config
        .visible_properties
        .iter()
        .chain(config.filters.iter().map(|filter| &filter.property))
        .chain(config.sort.iter().map(|sort| &sort.property));
    for key in referenced {
        if let PropertyKey::Custom(property_id) = key
            && !active_ids.contains(property_id)
        {
            bail!("view references unavailable property {property_id}");
        }
    }
    let property_kinds = active_properties
        .iter()
        .map(|property| Ok((property.id, PropertyKind::parse(&property.kind)?)))
        .collect::<Result<HashMap<_, _>>>()?;
    let active_options = BoardPropertyOption::find()
        .filter(board_property_option::Column::DeletedAt.is_null())
        .all(db)
        .await?
        .into_iter()
        .map(|option| (option.id, option.property_id))
        .collect::<HashMap<_, _>>();
    let active_labels = BoardLabel::find()
        .filter(board_label::Column::BoardId.eq(board_id))
        .all(db)
        .await?
        .into_iter()
        .map(|label| label.id)
        .collect::<HashSet<_>>();
    for filter in &config.filters {
        validate_view_filter(filter, &property_kinds, &active_options, &active_labels)?;
    }
    Ok(())
}

fn validate_view_filter(
    filter: &ViewFilter,
    property_kinds: &HashMap<i64, PropertyKind>,
    active_options: &HashMap<i64, i64>,
    active_labels: &HashSet<i64>,
) -> Result<()> {
    let empty_operator = matches!(
        filter.operator,
        FilterOperator::IsEmpty | FilterOperator::IsNotEmpty
    );
    match &filter.property {
        PropertyKey::Labels => match (&filter.operator, &filter.operand) {
            (
                FilterOperator::IsAnyOf | FilterOperator::IsNoneOf,
                Some(FilterOperand::LabelIds(ids)),
            ) if ids.iter().all(|id| active_labels.contains(id)) => Ok(()),
            (FilterOperator::IsEmpty | FilterOperator::IsNotEmpty, None) => Ok(()),
            _ => bail!("labels filter has an incompatible operator or value"),
        },
        PropertyKey::DueDate => match (&filter.operator, &filter.operand) {
            (
                FilterOperator::IsAnyOf | FilterOperator::IsNoneOf,
                Some(FilterOperand::DueDatePresets(_)),
            ) => Ok(()),
            (
                FilterOperator::Before | FilterOperator::On | FilterOperator::After,
                Some(FilterOperand::Date(value)),
            ) => {
                NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
                Ok(())
            }
            (FilterOperator::IsEmpty | FilterOperator::IsNotEmpty, None) => Ok(()),
            _ => bail!("due date filter has an incompatible operator or value"),
        },
        PropertyKey::Custom(property_id) => {
            let kind = property_kinds
                .get(property_id)
                .with_context(|| format!("property {property_id} is unavailable"))?;
            match kind {
                PropertyKind::Text | PropertyKind::Url => match (&filter.operator, &filter.operand)
                {
                    (
                        FilterOperator::Contains
                        | FilterOperator::DoesNotContain
                        | FilterOperator::Equals,
                        Some(FilterOperand::Text(_)),
                    ) => Ok(()),
                    (_, None) if empty_operator => Ok(()),
                    _ => bail!("text filter has an incompatible operator or value"),
                },
                PropertyKind::Number => match (&filter.operator, &filter.operand) {
                    (
                        FilterOperator::Equals
                        | FilterOperator::GreaterThan
                        | FilterOperator::LessThan,
                        Some(FilterOperand::Number(value)),
                    ) if value.is_finite() => Ok(()),
                    (_, None) if empty_operator => Ok(()),
                    _ => bail!("number filter has an incompatible operator or value"),
                },
                PropertyKind::Date => match (&filter.operator, &filter.operand) {
                    (
                        FilterOperator::Before | FilterOperator::On | FilterOperator::After,
                        Some(FilterOperand::Date(value)),
                    ) => {
                        NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
                        Ok(())
                    }
                    (_, None) if empty_operator => Ok(()),
                    _ => bail!("date filter has an incompatible operator or value"),
                },
                PropertyKind::Checkbox => match (&filter.operator, &filter.operand) {
                    (
                        FilterOperator::IsChecked
                        | FilterOperator::IsUnchecked
                        | FilterOperator::IsEmpty,
                        None,
                    ) => Ok(()),
                    _ => bail!("checkbox filter has an incompatible operator or value"),
                },
                PropertyKind::Select => match (&filter.operator, &filter.operand) {
                    (
                        FilterOperator::IsAnyOf | FilterOperator::IsNoneOf,
                        Some(FilterOperand::OptionIds(ids)),
                    ) if ids
                        .iter()
                        .all(|id| active_options.get(id) == Some(property_id)) =>
                    {
                        Ok(())
                    }
                    (_, None) if empty_operator => Ok(()),
                    _ => bail!("select filter has an incompatible operator or value"),
                },
            }
        }
    }
}

async fn active_board(db: &DatabaseConnection, board_id: i64) -> Result<board::Model> {
    Board::find_by_id(board_id)
        .filter(board::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .with_context(|| format!("active board {board_id} was not found"))
}

async fn active_property(
    db: &DatabaseConnection,
    property_id: i64,
) -> Result<board_property::Model> {
    BoardProperty::find_by_id(property_id)
        .filter(board_property::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .with_context(|| format!("active board property {property_id} was not found"))
}

async fn active_property_option(
    db: &DatabaseConnection,
    option_id: i64,
) -> Result<board_property_option::Model> {
    BoardPropertyOption::find_by_id(option_id)
        .filter(board_property_option::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .with_context(|| format!("active property option {option_id} was not found"))
}

async fn active_board_view(
    db: &DatabaseConnection,
    view_id: i64,
) -> Result<saved_board_view::Model> {
    SavedBoardView::find_by_id(view_id)
        .filter(saved_board_view::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .with_context(|| format!("active board view {view_id} was not found"))
}

async fn ensure_unique_view_name(
    db: &DatabaseConnection,
    board_id: i64,
    except_id: Option<i64>,
    name: &str,
) -> Result<()> {
    let duplicate = SavedBoardView::find()
        .filter(saved_board_view::Column::BoardId.eq(board_id))
        .filter(saved_board_view::Column::DeletedAt.is_null())
        .all(db)
        .await?
        .into_iter()
        .any(|view| Some(view.id) != except_id && view.name.eq_ignore_ascii_case(name));
    if duplicate {
        bail!("board view {name:?} already exists");
    }
    Ok(())
}

fn validate_reorder_ids(
    current_ids: impl IntoIterator<Item = i64>,
    ordered_ids: &[i64],
    label: &str,
) -> Result<()> {
    let current = current_ids.into_iter().collect::<HashSet<_>>();
    let ordered = ordered_ids.iter().copied().collect::<HashSet<_>>();
    if current.len() != ordered_ids.len() || current != ordered {
        bail!("reordered {label} must contain every active item exactly once");
    }
    Ok(())
}

fn view_references_property(config: &BoardViewConfig, property_id: i64) -> bool {
    let key = PropertyKey::Custom(property_id);
    config.visible_properties.contains(&key)
        || config.filters.iter().any(|filter| filter.property == key)
        || config
            .sort
            .as_ref()
            .is_some_and(|sort| sort.property == key)
}

async fn clean_property_from_views(
    transaction: &sea_orm::DatabaseTransaction,
    board_id: i64,
    property_id: i64,
) -> Result<()> {
    let models = SavedBoardView::find()
        .filter(saved_board_view::Column::BoardId.eq(board_id))
        .filter(saved_board_view::Column::DeletedAt.is_null())
        .all(transaction)
        .await?;
    let key = PropertyKey::Custom(property_id);
    for model in models {
        let mut config = decode_board_view(model.clone())?.config;
        if !view_references_property(&config, property_id) {
            continue;
        }
        config
            .visible_properties
            .retain(|candidate| candidate != &key);
        config.filters.retain(|filter| filter.property != key);
        if config
            .sort
            .as_ref()
            .is_some_and(|sort| sort.property == key)
        {
            config.sort = None;
        }
        let mut active = model.into_active_model();
        active.config_version = Set(BOARD_VIEW_CONFIG_VERSION);
        active.config_json = Set(serde_json::to_string(&config)?);
        active.update(transaction).await?;
    }
    Ok(())
}

async fn clean_option_from_views(
    transaction: &sea_orm::DatabaseTransaction,
    board_id: i64,
    option_id: i64,
) -> Result<()> {
    let models = SavedBoardView::find()
        .filter(saved_board_view::Column::BoardId.eq(board_id))
        .filter(saved_board_view::Column::DeletedAt.is_null())
        .all(transaction)
        .await?;
    for model in models {
        let mut config = decode_board_view(model.clone())?.config;
        let mut changed = false;
        config.filters.retain_mut(|filter| {
            let Some(FilterOperand::OptionIds(option_ids)) = filter.operand.as_mut() else {
                return true;
            };
            let original_len = option_ids.len();
            option_ids.retain(|candidate| *candidate != option_id);
            changed |= option_ids.len() != original_len;
            !option_ids.is_empty()
        });
        if !changed {
            continue;
        }
        let mut active = model.into_active_model();
        active.config_version = Set(BOARD_VIEW_CONFIG_VERSION);
        active.config_json = Set(serde_json::to_string(&config)?);
        active.update(transaction).await?;
    }
    Ok(())
}

async fn entry_board_id(db: &DatabaseConnection, entry_id: i64) -> Result<i64> {
    let entry = Entry::find_by_id(entry_id)
        .filter(entry::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .with_context(|| format!("active board entry {entry_id} was not found"))?;
    let list = Card::find_by_id(entry.card_id)
        .filter(card::Column::DeletedAt.is_null())
        .one(db)
        .await?
        .with_context(|| format!("active list {} was not found", entry.card_id))?;
    active_board(db, list.board_id).await?;
    Ok(list.board_id)
}

async fn active_entry_ids(db: &DatabaseConnection, board_id: i64) -> Result<Vec<i64>> {
    let list_ids = Card::find()
        .filter(card::Column::BoardId.eq(board_id))
        .filter(card::Column::DeletedAt.is_null())
        .all(db)
        .await?
        .into_iter()
        .map(|list| list.id)
        .collect::<Vec<_>>();
    if list_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(Entry::find()
        .filter(entry::Column::CardId.is_in(list_ids))
        .filter(entry::Column::DeletedAt.is_null())
        .all(db)
        .await?
        .into_iter()
        .map(|entry| entry.id)
        .collect())
}

fn decode_value(model: entry_property_value::Model, kind: PropertyKind) -> Result<EntryProperty> {
    let value = match kind {
        PropertyKind::Text => PropertyValue::Text(model.text_value.context("missing text value")?),
        PropertyKind::Number => {
            PropertyValue::Number(model.number_value.context("missing number value")?)
        }
        PropertyKind::Checkbox => {
            PropertyValue::Checkbox(model.boolean_value.context("missing checkbox value")?)
        }
        PropertyKind::Date => PropertyValue::Date(model.date_value.context("missing date value")?),
        PropertyKind::Select => {
            PropertyValue::Select(model.option_id.context("missing select option")?)
        }
        PropertyKind::Url => PropertyValue::Url(model.text_value.context("missing URL value")?),
    };
    Ok(EntryProperty {
        entry_id: model.entry_id,
        property_id: model.property_id,
        value,
    })
}

fn property_option(model: board_property_option::Model) -> PropertyOption {
    PropertyOption {
        id: model.id,
        property_id: model.property_id,
        name: model.name,
        color: model.color,
        position: model.position,
    }
}

fn decode_board_view(model: saved_board_view::Model) -> Result<BoardView> {
    let mut config = match model.config_version {
        BOARD_VIEW_CONFIG_VERSION => serde_json::from_str(&model.config_json)?,
        1 => migrate_v1_config(serde_json::from_str(&model.config_json)?),
        version => bail!("unsupported board view config version {version}"),
    };
    config.visible_properties.truncate(3);
    Ok(BoardView {
        id: model.id,
        board_id: model.board_id,
        name: model.name,
        position: model.position,
        is_default: model.is_default,
        config,
    })
}

#[derive(Deserialize)]
struct BoardViewConfigV1 {
    #[serde(default)]
    filters: Vec<ViewFilterV1>,
    sort: Option<ViewSort>,
    #[serde(default)]
    visible_properties: Vec<PropertyKey>,
    #[serde(default)]
    compact_cards: bool,
}

#[derive(Deserialize)]
struct ViewFilterV1 {
    property: PropertyKey,
    operator: FilterOperator,
    value: Option<PropertyValue>,
}

fn migrate_v1_config(config: BoardViewConfigV1) -> BoardViewConfig {
    BoardViewConfig {
        filters: config
            .filters
            .into_iter()
            .map(|filter| ViewFilter {
                property: filter.property,
                operator: filter.operator,
                operand: filter.value.map(|value| match value {
                    PropertyValue::Text(value) | PropertyValue::Url(value) => {
                        FilterOperand::Text(value)
                    }
                    PropertyValue::Number(value) => FilterOperand::Number(value),
                    PropertyValue::Date(value) => FilterOperand::Date(value),
                    PropertyValue::Select(value) => FilterOperand::OptionIds(vec![value]),
                    PropertyValue::Checkbox(_) => FilterOperand::OptionIds(Vec::new()),
                }),
            })
            .collect(),
        sort: config.sort,
        visible_properties: config.visible_properties,
        compact_cards: config.compact_cards,
    }
}

fn required_text(value: String, field: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity::{board, card, entry};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};

    #[tokio::test]
    async fn typed_properties_are_board_scoped_and_bulk_loaded() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let (board, entry) = board_with_entry(&db, "Collection").await?;
        let (other_board, other_entry) = board_with_entry(&db, "Other").await?;
        let rating = create_property(&db, board.id, "Rating".into(), PropertyKind::Number).await?;
        set_entry_property(&db, entry.id, rating.id, PropertyValue::Number(4.5)).await?;
        assert!(
            set_entry_property(&db, other_entry.id, rating.id, PropertyValue::Number(3.0))
                .await
                .is_err()
        );
        assert!(
            create_property(&db, other_board.id, "Rating".into(), PropertyKind::Number)
                .await
                .is_ok()
        );
        let loaded = load_board_properties(&db, board.id).await?;
        assert_eq!(loaded.definitions.len(), 1);
        assert_eq!(loaded.values[0].value, PropertyValue::Number(4.5));
        Ok(())
    }

    #[tokio::test]
    async fn saved_view_rejects_foreign_property_references() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let (board, _) = board_with_entry(&db, "First").await?;
        let (other, _) = board_with_entry(&db, "Second").await?;
        let foreign = create_property(&db, other.id, "Stage".into(), PropertyKind::Select).await?;
        let config = BoardViewConfig {
            visible_properties: vec![PropertyKey::Custom(foreign.id)],
            ..Default::default()
        };
        assert!(
            create_board_view(&db, board.id, "Invalid".into(), config)
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn deleting_options_and_properties_cleans_values_and_views() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let (board, entry) = board_with_entry(&db, "Collection").await?;
        let stage = create_property(&db, board.id, "Stage".into(), PropertyKind::Select).await?;
        let draft = create_property_option(&db, stage.id, "Draft".into(), "blue".into()).await?;
        set_entry_property(&db, entry.id, stage.id, PropertyValue::Select(draft.id)).await?;
        let config = BoardViewConfig {
            filters: vec![ViewFilter {
                property: PropertyKey::Custom(stage.id),
                operator: FilterOperator::IsAnyOf,
                operand: Some(FilterOperand::OptionIds(vec![draft.id])),
            }],
            sort: Some(ViewSort {
                property: PropertyKey::Custom(stage.id),
                direction: SortDirection::Ascending,
            }),
            visible_properties: vec![PropertyKey::Custom(stage.id)],
            compact_cards: false,
        };
        create_board_view(&db, board.id, "Drafts".into(), config).await?;

        assert_eq!(
            property_deletion_impact(&db, stage.id).await?,
            DeletionImpact {
                value_count: 1,
                view_count: 1,
            }
        );
        delete_property_option(&db, draft.id).await?;
        let loaded = load_board_properties(&db, board.id).await?;
        assert!(loaded.values.is_empty());
        assert!(loaded.definitions[0].options.is_empty());
        let views = load_board_views(&db, board.id).await?;
        assert!(views.views[0].config.filters.is_empty());

        delete_property(&db, stage.id).await?;
        let views = load_board_views(&db, board.id).await?;
        assert!(views.views[0].config.visible_properties.is_empty());
        assert!(views.views[0].config.sort.is_none());
        assert!(
            load_board_properties(&db, board.id)
                .await?
                .definitions
                .is_empty()
        );
        Ok(())
    }

    #[tokio::test]
    async fn saved_view_crud_preserves_one_default() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let (board, _) = board_with_entry(&db, "Views").await?;
        let first = create_board_view(&db, board.id, "First".into(), Default::default()).await?;
        let second = create_board_view(&db, board.id, "Second".into(), Default::default()).await?;

        set_default_board_view(&db, first.id).await?;
        set_default_board_view(&db, second.id).await?;
        let views = load_board_views(&db, board.id).await?;
        assert_eq!(views.views.iter().filter(|view| view.is_default).count(), 1);
        assert!(
            views
                .views
                .iter()
                .find(|view| view.id == second.id)
                .is_some_and(|view| view.is_default)
        );

        let renamed = rename_board_view(&db, second.id, "Current".into()).await?;
        assert_eq!(renamed.name, "Current");
        let updated = update_board_view(
            &db,
            second.id,
            BoardViewConfig {
                compact_cards: true,
                ..Default::default()
            },
        )
        .await?;
        assert!(updated.config.compact_cards);
        delete_board_view(&db, second.id).await?;
        let views = load_board_views(&db, board.id).await?;
        assert_eq!(views.views.len(), 1);
        assert!(views.views[0].is_default);
        Ok(())
    }

    #[tokio::test]
    async fn selected_board_view_is_restored_independently_from_default() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let (board, _) = board_with_entry(&db, "Views").await?;
        let first = create_board_view(&db, board.id, "First".into(), Default::default()).await?;
        let second = create_board_view(&db, board.id, "Second".into(), Default::default()).await?;

        set_default_board_view(&db, first.id).await?;
        set_selected_board_view(&db, board.id, Some(second.id)).await?;

        let views = load_board_views(&db, board.id).await?;
        assert_eq!(views.selected_view_id, Some(second.id));
        assert!(
            views
                .views
                .iter()
                .find(|view| view.id == first.id)
                .is_some_and(|view| view.is_default)
        );

        set_selected_board_view(&db, board.id, None).await?;
        assert_eq!(
            load_board_views(&db, board.id).await?.selected_view_id,
            None
        );

        set_selected_board_view(&db, board.id, Some(second.id)).await?;
        delete_board_view(&db, second.id).await?;
        assert_eq!(
            load_board_views(&db, board.id).await?.selected_view_id,
            None
        );

        let (other_board, _) = board_with_entry(&db, "Other").await?;
        let other_view =
            create_board_view(&db, other_board.id, "Other".into(), Default::default()).await?;
        assert!(
            set_selected_board_view(&db, board.id, Some(other_view.id))
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn version_one_views_are_migrated_without_blocking_board_load() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let (board, _) = board_with_entry(&db, "Legacy").await?;
        let rating = create_property(&db, board.id, "Rating".into(), PropertyKind::Number).await?;
        saved_board_view::ActiveModel {
            board_id: Set(board.id),
            name: Set("Legacy view".into()),
            position: Set(0),
            is_default: Set(true),
            config_version: Set(1),
            config_json: Set(serde_json::json!({
                "filters": [{
                    "property": { "kind": "custom", "id": rating.id },
                    "operator": "greater_than",
                    "value": { "kind": "number", "value": 3.0 }
                }],
                "sort": null,
                "visible_properties": [{ "kind": "custom", "id": rating.id }],
                "compact_cards": false
            })
            .to_string()),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let views = load_board_views(&db, board.id).await?;
        assert!(views.warnings.is_empty());
        assert_eq!(
            views.views[0].config.filters[0].operand,
            Some(FilterOperand::Number(3.0))
        );
        Ok(())
    }

    #[tokio::test]
    async fn malformed_property_values_are_reported_as_warnings() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let (board, entry) = board_with_entry(&db, "Warnings").await?;
        let rating = create_property(&db, board.id, "Rating".into(), PropertyKind::Number).await?;
        entry_property_value::ActiveModel {
            entry_id: Set(entry.id),
            property_id: Set(rating.id),
            text_value: Set(None),
            number_value: Set(None),
            boolean_value: Set(None),
            date_value: Set(None),
            option_id: Set(None),
        }
        .insert(&db)
        .await?;

        let loaded = load_board_properties(&db, board.id).await?;
        assert!(loaded.values.is_empty());
        assert_eq!(loaded.warnings.len(), 1);
        Ok(())
    }

    async fn board_with_entry(
        db: &DatabaseConnection,
        title: &str,
    ) -> Result<(board::Model, entry::Model)> {
        let board = board::ActiveModel {
            title: Set(title.to_string()),
            project_id: Set(None),
            ..Default::default()
        }
        .insert(db)
        .await?;
        let list = card::ActiveModel {
            title: Set("Any list".to_string()),
            board_id: Set(board.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(db)
        .await?;
        let entry = entry::ActiveModel {
            title: Set("An entry".to_string()),
            description: Set(String::new()),
            card_id: Set(list.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(db)
        .await?;
        Ok((board, entry))
    }
}
