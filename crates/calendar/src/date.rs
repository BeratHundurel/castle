use std::{fmt, str::FromStr};

use chrono::{Datelike, NaiveDate, Weekday as ChronoWeekday};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CalendarDate(NaiveDate);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalendarDateError {
    InvalidFormat,
    InvalidDate,
}

impl fmt::Display for CalendarDateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => formatter.write_str("date must use YYYY-MM-DD format"),
            Self::InvalidDate => formatter.write_str("date is not a valid calendar date"),
        }
    }
}

impl std::error::Error for CalendarDateError {}

impl CalendarDate {
    pub fn parse(value: &str) -> Result<Self, CalendarDateError> {
        let bytes = value.as_bytes();
        let valid_shape = bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());

        if !valid_shape {
            return Err(CalendarDateError::InvalidFormat);
        }

        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(Self)
            .map_err(|_| CalendarDateError::InvalidDate)
    }

    pub fn from_ymd(year: i32, month: u32, day: u32) -> Result<Self, CalendarDateError> {
        NaiveDate::from_ymd_opt(year, month, day)
            .map(Self)
            .ok_or(CalendarDateError::InvalidDate)
    }

    pub fn year(self) -> i32 {
        self.0.year()
    }

    pub fn month(self) -> u32 {
        self.0.month()
    }

    pub(crate) fn month0(self) -> u32 {
        self.0.month0()
    }

    pub fn day(self) -> u32 {
        self.0.day()
    }

    pub fn checked_add_days(self, days: i64) -> Option<Self> {
        self.0
            .checked_add_signed(chrono::Duration::days(days))
            .map(Self)
    }

    pub fn days_until(self, other: Self) -> i64 {
        other.0.signed_duration_since(self.0).num_days()
    }

    pub fn is_leap_year(self) -> bool {
        is_leap_year(self.year())
    }

    pub(crate) fn as_naive(self) -> NaiveDate {
        self.0
    }

    pub(crate) fn from_naive(value: NaiveDate) -> Self {
        Self(value)
    }

    pub(crate) fn chrono_weekday(self) -> ChronoWeekday {
        self.0.weekday()
    }
}

impl fmt::Display for CalendarDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.format("%Y-%m-%d").fmt(formatter)
    }
}

impl FromStr for CalendarDate {
    type Err = CalendarDateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl From<NaiveDate> for CalendarDate {
    fn from(value: NaiveDate) -> Self {
        Self::from_naive(value)
    }
}

impl From<CalendarDate> for NaiveDate {
    fn from(value: CalendarDate) -> Self {
        value.as_naive()
    }
}

impl Serialize for CalendarDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CalendarDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

pub(crate) fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
