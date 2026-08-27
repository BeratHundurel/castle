use anyhow::{Result, bail};
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
    RelatedNotes,
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
