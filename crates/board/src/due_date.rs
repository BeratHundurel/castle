use chrono::NaiveDate;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DueDateStatus {
    Future,
    Today,
    Overdue,
    Invalid,
}

pub(crate) fn due_date_status(value: &str, today: NaiveDate) -> DueDateStatus {
    match NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        Ok(date) if date < today => DueDateStatus::Overdue,
        Ok(date) if date == today => DueDateStatus::Today,
        Ok(_) => DueDateStatus::Future,
        Err(_) => DueDateStatus::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{DueDateStatus, due_date_status};

    #[test]
    fn classifies_due_dates_relative_to_the_local_day() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 10).expect("valid test date");

        assert_eq!(due_date_status("2026-07-09", today), DueDateStatus::Overdue);
        assert_eq!(due_date_status("2026-07-10", today), DueDateStatus::Today);
        assert_eq!(due_date_status("2026-07-11", today), DueDateStatus::Future);
        assert_eq!(due_date_status("not-a-date", today), DueDateStatus::Invalid);
    }
}
