use std::collections::{HashMap, HashSet};

use chrono::{Duration, NaiveDate};
use storage::board_properties::{
    BoardViewConfig, DueDatePreset, FilterOperand, FilterOperator, PropertyDefinition, PropertyKey,
    PropertyValue, ViewFilter,
};

use super::due_date::{DueDateStatus, due_date_status};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DueDateFilter {
    Overdue,
    Today,
    NextSevenDays,
    NoDueDate,
}

impl DueDateFilter {
    fn preset(self) -> DueDatePreset {
        match self {
            Self::Overdue => DueDatePreset::Overdue,
            Self::Today => DueDatePreset::Today,
            Self::NextSevenDays => DueDatePreset::NextSevenDays,
            Self::NoDueDate => DueDatePreset::NoDueDate,
        }
    }

    fn from_preset(value: DueDatePreset) -> Self {
        match value {
            DueDatePreset::Overdue => Self::Overdue,
            DueDatePreset::Today => Self::Today,
            DueDatePreset::NextSevenDays => Self::NextSevenDays,
            DueDatePreset::NoDueDate => Self::NoDueDate,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct BoardFilters {
    pub(crate) label_ids: HashSet<u32>,
    pub(crate) due_dates: HashSet<DueDateFilter>,
    pub(crate) custom: Vec<ViewFilter>,
}

impl BoardFilters {
    pub(crate) fn from_config(config: &BoardViewConfig) -> Self {
        let mut filters = Self::default();
        for filter in &config.filters {
            match (&filter.property, &filter.operand) {
                (PropertyKey::Labels, Some(FilterOperand::LabelIds(ids))) => {
                    filters
                        .label_ids
                        .extend(ids.iter().filter_map(|id| u32::try_from(*id).ok()));
                }
                (PropertyKey::DueDate, Some(FilterOperand::DueDatePresets(presets))) => {
                    filters
                        .due_dates
                        .extend(presets.iter().copied().map(DueDateFilter::from_preset));
                }
                (PropertyKey::Custom(_), _) => filters.custom.push(filter.clone()),
                _ => {}
            }
        }
        filters
    }

    pub(crate) fn sync_config(&self, config: &mut BoardViewConfig) {
        config
            .filters
            .retain(|filter| matches!(filter.property, PropertyKey::Custom(_)));
        config.filters.clone_from(&self.custom);
        if !self.label_ids.is_empty() {
            let mut ids = self
                .label_ids
                .iter()
                .copied()
                .map(i64::from)
                .collect::<Vec<_>>();
            ids.sort_unstable();
            config.filters.push(ViewFilter {
                property: PropertyKey::Labels,
                operator: FilterOperator::IsAnyOf,
                operand: Some(FilterOperand::LabelIds(ids)),
            });
        }
        if !self.due_dates.is_empty() {
            let mut presets = self
                .due_dates
                .iter()
                .copied()
                .map(DueDateFilter::preset)
                .collect::<Vec<_>>();
            presets.sort_by_key(|preset| *preset as u8);
            config.filters.push(ViewFilter {
                property: PropertyKey::DueDate,
                operator: FilterOperator::IsAnyOf,
                operand: Some(FilterOperand::DueDatePresets(presets)),
            });
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.label_ids.is_empty() || !self.due_dates.is_empty() || !self.custom.is_empty()
    }

    pub(crate) fn count(&self) -> usize {
        usize::from(!self.label_ids.is_empty())
            + usize::from(!self.due_dates.is_empty())
            + self.custom.len()
    }

    pub(crate) fn clear(&mut self) {
        self.label_ids.clear();
        self.due_dates.clear();
        self.custom.clear();
    }
}

pub(crate) fn default_view_config() -> BoardViewConfig {
    BoardViewConfig {
        visible_properties: vec![PropertyKey::Labels, PropertyKey::DueDate],
        ..Default::default()
    }
}

pub(crate) fn matches_filters(
    card_label_ids: impl IntoIterator<Item = u32>,
    due_on: Option<&str>,
    filters: &BoardFilters,
    today: NaiveDate,
) -> bool {
    let labels_match = filters.label_ids.is_empty()
        || card_label_ids
            .into_iter()
            .any(|label_id| filters.label_ids.contains(&label_id));
    let due_date_matches = filters.due_dates.is_empty()
        || filters
            .due_dates
            .iter()
            .any(|filter| matches_due_date_filter(*filter, due_on, today));

    labels_match && due_date_matches
}

pub(crate) fn matches_custom_filters(
    entry_id: i64,
    filters: &[ViewFilter],
    values: &HashMap<(i64, i64), PropertyValue>,
    definitions: &[PropertyDefinition],
) -> bool {
    filters.iter().all(|filter| {
        let PropertyKey::Custom(property_id) = filter.property else {
            return true;
        };
        let value = values.get(&(entry_id, property_id));
        let definition = definitions
            .iter()
            .find(|property| property.id == property_id);
        matches_custom_filter(filter, value, definition)
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
        FilterOperator::IsChecked => {
            return matches!(value, Some(PropertyValue::Checkbox(true)));
        }
        FilterOperator::IsUnchecked => {
            return matches!(value, Some(PropertyValue::Checkbox(false)));
        }
        _ => {}
    }
    match (value, filter.operand.as_ref()) {
        (Some(PropertyValue::Text(value)), Some(FilterOperand::Text(expected)))
        | (Some(PropertyValue::Url(value)), Some(FilterOperand::Text(expected))) => {
            compare_text(value, expected, filter.operator)
        }
        (Some(PropertyValue::Number(value)), Some(FilterOperand::Number(expected))) => {
            match filter.operator {
                FilterOperator::Equals => value == expected,
                FilterOperator::GreaterThan => value > expected,
                FilterOperator::LessThan => value < expected,
                _ => false,
            }
        }
        (Some(PropertyValue::Date(value)), Some(FilterOperand::Date(expected))) => {
            match filter.operator {
                FilterOperator::Before => value < expected,
                FilterOperator::On | FilterOperator::Equals => value == expected,
                FilterOperator::After => value > expected,
                _ => false,
            }
        }
        (Some(PropertyValue::Select(option_id)), Some(FilterOperand::OptionIds(ids))) => {
            match filter.operator {
                FilterOperator::IsAnyOf | FilterOperator::Equals => ids.contains(option_id),
                FilterOperator::IsNoneOf => !ids.contains(option_id),
                _ => false,
            }
        }
        (Some(PropertyValue::Select(option_id)), Some(FilterOperand::Text(expected))) => {
            let option_name = definition
                .and_then(|property| {
                    property
                        .options
                        .iter()
                        .find(|option| option.id == *option_id)
                })
                .map(|option| option.name.as_str())
                .unwrap_or_default();
            compare_text(option_name, expected, filter.operator)
        }
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

fn matches_due_date_filter(filter: DueDateFilter, due_on: Option<&str>, today: NaiveDate) -> bool {
    match (filter, due_on) {
        (DueDateFilter::NoDueDate, None) => true,
        (DueDateFilter::NoDueDate, Some(_)) => false,
        (_, None) => false,
        (DueDateFilter::Overdue, Some(due_on)) => {
            due_date_status(due_on, today) == DueDateStatus::Overdue
        }
        (DueDateFilter::Today, Some(due_on)) => {
            due_date_status(due_on, today) == DueDateStatus::Today
        }
        (DueDateFilter::NextSevenDays, Some(due_on)) => {
            NaiveDate::parse_from_str(due_on, "%Y-%m-%d")
                .map(|due_on| due_on > today && due_on <= today + Duration::days(7))
                .unwrap_or(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use storage::board_properties::{PropertyDefinition, PropertyKind};

    #[test]
    fn combines_any_label_with_due_date_filters() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 10).expect("valid test date");
        let mut filters = BoardFilters::default();
        filters.label_ids.extend([2, 3]);
        filters.due_dates.insert(DueDateFilter::NextSevenDays);

        assert!(matches_filters([3], Some("2026-07-17"), &filters, today));
        assert!(!matches_filters([1], Some("2026-07-17"), &filters, today));
        assert!(!matches_filters([2], Some("2026-07-18"), &filters, today));
    }

    #[test]
    fn custom_filters_are_anded_and_use_indexed_values() {
        let definitions = vec![PropertyDefinition {
            id: 10,
            board_id: 1,
            name: "Rating".into(),
            kind: PropertyKind::Number,
            position: 0,
            options: vec![],
        }];
        let values = HashMap::from([((5, 10), PropertyValue::Number(4.5))]);
        let filters = vec![ViewFilter {
            property: PropertyKey::Custom(10),
            operator: FilterOperator::GreaterThan,
            operand: Some(FilterOperand::Number(4.0)),
        }];
        assert!(matches_custom_filters(5, &filters, &values, &definitions));
        assert!(!matches_custom_filters(6, &filters, &values, &definitions));
    }
}
