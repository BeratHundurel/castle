use std::collections::HashSet;

use chrono::{Duration, NaiveDate};

use super::due_date::{DueDateStatus, due_date_status};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DueDateFilter {
    Overdue,
    Today,
    NextSevenDays,
    NoDueDate,
}

#[derive(Debug, Default)]
pub(crate) struct BoardFilters {
    pub(crate) label_ids: HashSet<u32>,
    pub(crate) due_dates: HashSet<DueDateFilter>,
}

impl BoardFilters {
    pub(crate) fn is_active(&self) -> bool {
        !self.label_ids.is_empty() || !self.due_dates.is_empty()
    }

    pub(crate) fn count(&self) -> usize {
        self.label_ids.len() + self.due_dates.len()
    }

    pub(crate) fn clear(&mut self) {
        self.label_ids.clear();
        self.due_dates.clear();
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
    use chrono::NaiveDate;

    use super::{BoardFilters, DueDateFilter, matches_filters};

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
    fn matches_no_due_date_without_excluding_matching_labels() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 10).expect("valid test date");
        let mut filters = BoardFilters::default();
        filters.label_ids.insert(4);
        filters.due_dates.insert(DueDateFilter::NoDueDate);

        assert!(matches_filters([4], None, &filters, today));
        assert!(!matches_filters([4], Some("2026-07-10"), &filters, today));
    }
}
