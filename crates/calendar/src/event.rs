use serde::{Deserialize, Serialize};

use crate::CalendarDate;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub entry_id: i64,
    pub board_id: i64,
    pub list_id: i64,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_on: Option<CalendarDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_on: Option<CalendarDate>,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurrence_series_id: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalendarEventError {
    DueBeforeStart,
}

impl std::fmt::Display for CalendarEventError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DueBeforeStart => formatter.write_str("due date cannot be before the start date"),
        }
    }
}

impl std::error::Error for CalendarEventError {}

impl CalendarEvent {
    pub fn validate(&self) -> Result<(), CalendarEventError> {
        if let (Some(start_on), Some(due_on)) = (self.start_on, self.due_on)
            && due_on < start_on
        {
            return Err(CalendarEventError::DueBeforeStart);
        }

        Ok(())
    }

    pub fn is_scheduled(&self) -> bool {
        self.start_on.is_some() || self.due_on.is_some()
    }

    pub fn occurs_on(&self, date: CalendarDate) -> bool {
        match (self.start_on, self.due_on) {
            (Some(start_on), Some(due_on)) => date >= start_on && date <= due_on,
            (Some(start_on), None) => date == start_on,
            (None, Some(due_on)) => date == due_on,
            (None, None) => false,
        }
    }
}
