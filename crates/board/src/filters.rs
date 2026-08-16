use std::collections::HashSet;

use storage::board::properties::{
    BoardViewConfig, DueDatePreset, FilterOperand, FilterOperator, PropertyKey, ViewFilter,
};

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
    pub(crate) related_notes: Option<bool>,
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
                (PropertyKey::RelatedNotes, None) => match filter.operator {
                    FilterOperator::IsNotEmpty => filters.related_notes = Some(true),
                    FilterOperator::IsEmpty => filters.related_notes = Some(false),
                    _ => {}
                },
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

        if let Some(has_related_notes) = self.related_notes {
            config.filters.push(ViewFilter {
                property: PropertyKey::RelatedNotes,
                operator: if has_related_notes {
                    FilterOperator::IsNotEmpty
                } else {
                    FilterOperator::IsEmpty
                },
                operand: None,
            });
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.label_ids.is_empty()
            || !self.due_dates.is_empty()
            || !self.custom.is_empty()
            || self.related_notes.is_some()
    }

    pub(crate) fn clear(&mut self) {
        self.label_ids.clear();
        self.due_dates.clear();
        self.custom.clear();
        self.related_notes = None;
    }
}

pub(crate) fn default_view_config() -> BoardViewConfig {
    BoardViewConfig {
        visible_properties: vec![PropertyKey::Labels, PropertyKey::DueDate],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn related_note_filter_round_trips_through_saved_view_config() {
        let config = BoardViewConfig {
            filters: vec![ViewFilter {
                property: PropertyKey::RelatedNotes,
                operator: FilterOperator::IsNotEmpty,
                operand: None,
            }],
            ..Default::default()
        };
        let filters = BoardFilters::from_config(&config);
        assert_eq!(filters.related_notes, Some(true));

        let mut saved = BoardViewConfig::default();
        filters.sync_config(&mut saved);
        assert_eq!(saved.filters, config.filters);
    }
}
