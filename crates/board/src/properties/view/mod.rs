use super::*;
use storage::board::properties::{
    FilterOperand, FilterOperator, PropertyDefinition, PropertyKey, PropertyKind, PropertyValue,
    SortDirection,
};

mod card_values;
mod entry_values;
mod fields;
mod filters;
mod manager;
mod sorting;
mod views;

impl BoardView {
    pub(crate) fn property_key_label(&self, key: &PropertyKey) -> String {
        match key {
            PropertyKey::DueDate => "Due date".to_string(),
            PropertyKey::Labels => "Labels".to_string(),
            PropertyKey::RelatedNotes => "Related notes".to_string(),
            PropertyKey::Custom(property_id) => self
                .properties
                .data
                .definitions
                .iter()
                .find(|property| property.id == *property_id)
                .map(|property| property.name.clone())
                .unwrap_or_else(|| "Unavailable property".to_string()),
        }
    }
}

fn property_kind_label(kind: PropertyKind) -> &'static str {
    match kind {
        PropertyKind::Text => "Text",
        PropertyKind::Number => "Number",
        PropertyKind::Checkbox => "Checkbox",
        PropertyKind::Date => "Date",
        PropertyKind::Select => "Select",
        PropertyKind::Url => "URL",
    }
}

fn property_kind_description(kind: PropertyKind) -> &'static str {
    match kind {
        PropertyKind::Text => "Short text for names, references, or notes.",
        PropertyKind::Number => "A sortable numeric value such as effort or cost.",
        PropertyKind::Checkbox => "An optional true or false flag.",
        PropertyKind::Date => "A calendar date independent from Castle's due date.",
        PropertyKind::Select => "One choice from a board-specific list of colored options.",
        PropertyKind::Url => "A validated web address that can be opened from Castle.",
    }
}

fn property_value_label(
    property: &PropertyDefinition,
    value: Option<&PropertyValue>,
) -> SharedString {
    match value {
        Some(PropertyValue::Text(value))
        | Some(PropertyValue::Date(value))
        | Some(PropertyValue::Url(value)) => value.clone().into(),
        Some(PropertyValue::Number(value)) => value.to_string().into(),
        Some(PropertyValue::Checkbox(value)) => if *value { "Checked" } else { "Unchecked" }.into(),
        Some(PropertyValue::Select(option_id)) => property
            .options
            .iter()
            .find(|option| option.id == *option_id)
            .map(|option| SharedString::from(option.name.clone()))
            .unwrap_or_else(|| SharedString::from("Unavailable option")),
        None => match property.kind {
            PropertyKind::Checkbox => SharedString::from("Unchecked"),
            _ => SharedString::from("Empty"),
        },
    }
}

fn property_card_text_row(name: &str, value: &str, muted: Hsla) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .text_xs()
        .child(
            div()
                .flex_shrink_0()
                .text_color(muted)
                .child(name.to_string()),
        )
        .child(div().min_w_0().truncate().child(value.to_string()))
        .into_any_element()
}

fn display_url(value: &str) -> String {
    value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(value)
        .trim_end_matches('/')
        .to_string()
}

fn property_key_id(key: &PropertyKey) -> String {
    match key {
        PropertyKey::DueDate => "due-date".to_string(),
        PropertyKey::Labels => "labels".to_string(),
        PropertyKey::RelatedNotes => "related-notes".to_string(),
        PropertyKey::Custom(id) => format!("custom-{id}"),
    }
}

fn filter_operator_label(operator: FilterOperator) -> &'static str {
    match operator {
        FilterOperator::Contains => "Contains",
        FilterOperator::DoesNotContain => "Does not contain",
        FilterOperator::Equals => "Equals",
        FilterOperator::GreaterThan => "Greater than",
        FilterOperator::LessThan => "Less than",
        FilterOperator::Before => "Before",
        FilterOperator::On => "On",
        FilterOperator::After => "After",
        FilterOperator::IsAnyOf => "Is any of",
        FilterOperator::IsNoneOf => "Is none of",
        FilterOperator::IsEmpty => "Is empty",
        FilterOperator::IsNotEmpty => "Is not empty",
        FilterOperator::IsChecked => "Checked",
        FilterOperator::IsUnchecked => "Unchecked",
    }
}

fn filter_operand_label(operand: &FilterOperand) -> String {
    match operand {
        FilterOperand::Text(value) | FilterOperand::Date(value) => value.clone(),
        FilterOperand::Number(value) => value.to_string(),
        FilterOperand::OptionIds(values) => format!("{} option(s)", values.len()),
        FilterOperand::LabelIds(values) => format!("{} label(s)", values.len()),
        FilterOperand::DueDatePresets(values) => format!("{} date range(s)", values.len()),
    }
}
