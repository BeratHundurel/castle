use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::CalendarDate;
use crate::date::{CalendarDateError, is_leap_year};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecurrenceFrequency {
    #[default]
    Daily,
    Weekly,
    Monthly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    pub const ALL: [Self; 7] = [
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
        Self::Sunday,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "mon" | "monday" => Some(Self::Monday),
            "tue" | "tues" | "tuesday" => Some(Self::Tuesday),
            "wed" | "wednesday" => Some(Self::Wednesday),
            "thu" | "thur" | "thurs" | "thursday" => Some(Self::Thursday),
            "fri" | "friday" => Some(Self::Friday),
            "sat" | "saturday" => Some(Self::Saturday),
            "sun" | "sunday" => Some(Self::Sunday),
            _ => None,
        }
    }

    pub(crate) fn offset_from_monday(self) -> i64 {
        match self {
            Self::Monday => 0,
            Self::Tuesday => 1,
            Self::Wednesday => 2,
            Self::Thursday => 3,
            Self::Friday => 4,
            Self::Saturday => 5,
            Self::Sunday => 6,
        }
    }

    fn from_chrono(value: chrono::Weekday) -> Self {
        match value {
            chrono::Weekday::Mon => Self::Monday,
            chrono::Weekday::Tue => Self::Tuesday,
            chrono::Weekday::Wed => Self::Wednesday,
            chrono::Weekday::Thu => Self::Thursday,
            chrono::Weekday::Fri => Self::Friday,
            chrono::Weekday::Sat => Self::Saturday,
            chrono::Weekday::Sun => Self::Sunday,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationMode {
    #[default]
    OnCompletion,
    OnSchedule,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurrenceRule {
    pub frequency: RecurrenceFrequency,
    #[serde(default = "default_interval")]
    pub interval: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weekdays: Vec<Weekday>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_of_month: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecurrenceRuleError {
    IntervalMustBePositive,
    WeeklyDaysRequired,
    DuplicateWeekday,
    MonthlyDayOutOfRange,
    UnexpectedField,
    MissingDayOfMonth,
    InvalidWeekday(String),
    InvalidInterval(String),
    InvalidSyntax(String),
}

impl fmt::Display for RecurrenceRuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IntervalMustBePositive => {
                formatter.write_str("recurrence interval must be positive")
            }
            Self::WeeklyDaysRequired => {
                formatter.write_str("weekly recurrence needs at least one weekday")
            }
            Self::DuplicateWeekday => {
                formatter.write_str("weekly recurrence cannot repeat a weekday")
            }
            Self::MonthlyDayOutOfRange => {
                formatter.write_str("monthly day must be between 1 and 31")
            }
            Self::UnexpectedField => {
                formatter.write_str("recurrence rule has fields for another frequency")
            }
            Self::MissingDayOfMonth => {
                formatter.write_str("monthly recurrence needs a day of the month")
            }
            Self::InvalidWeekday(value) => write!(formatter, "invalid weekday: {value}"),
            Self::InvalidInterval(value) => {
                write!(formatter, "invalid recurrence interval: {value}")
            }
            Self::InvalidSyntax(value) => write!(formatter, "invalid recurrence rule: {value}"),
        }
    }
}

impl std::error::Error for RecurrenceRuleError {}

impl RecurrenceRule {
    pub fn daily(interval: u32) -> Result<Self, RecurrenceRuleError> {
        let rule = Self {
            frequency: RecurrenceFrequency::Daily,
            interval,
            weekdays: Vec::new(),
            day_of_month: None,
        };
        rule.validate().map(|()| rule)
    }

    pub fn weekly(interval: u32, mut weekdays: Vec<Weekday>) -> Result<Self, RecurrenceRuleError> {
        weekdays.sort_unstable();
        let rule = Self {
            frequency: RecurrenceFrequency::Weekly,
            interval,
            weekdays,
            day_of_month: None,
        };
        rule.validate().map(|()| rule)
    }

    pub fn monthly(interval: u32, day_of_month: u8) -> Result<Self, RecurrenceRuleError> {
        let rule = Self {
            frequency: RecurrenceFrequency::Monthly,
            interval,
            weekdays: Vec::new(),
            day_of_month: Some(day_of_month),
        };
        rule.validate().map(|()| rule)
    }

    pub fn validate(&self) -> Result<(), RecurrenceRuleError> {
        if self.interval == 0 {
            return Err(RecurrenceRuleError::IntervalMustBePositive);
        }

        match self.frequency {
            RecurrenceFrequency::Daily => {
                if !self.weekdays.is_empty() || self.day_of_month.is_some() {
                    return Err(RecurrenceRuleError::UnexpectedField);
                }
            }
            RecurrenceFrequency::Weekly => {
                if self.weekdays.is_empty() {
                    return Err(RecurrenceRuleError::WeeklyDaysRequired);
                }
                if self.day_of_month.is_some() {
                    return Err(RecurrenceRuleError::UnexpectedField);
                }
                for (index, weekday) in self.weekdays.iter().enumerate() {
                    if self.weekdays[index + 1..].contains(weekday) {
                        return Err(RecurrenceRuleError::DuplicateWeekday);
                    }
                }
            }
            RecurrenceFrequency::Monthly => {
                if !self.weekdays.is_empty() {
                    return Err(RecurrenceRuleError::UnexpectedField);
                }
                match self.day_of_month {
                    Some(day @ 1..=31) => {
                        let _ = day;
                    }
                    Some(_) => return Err(RecurrenceRuleError::MonthlyDayOutOfRange),
                    None => return Err(RecurrenceRuleError::MissingDayOfMonth),
                }
            }
        }

        Ok(())
    }

    pub fn parse(value: &str) -> Result<Self, RecurrenceRuleError> {
        let normalized = value.trim().to_ascii_lowercase();
        let tokens: Vec<&str> = normalized.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(RecurrenceRuleError::InvalidSyntax(value.to_string()));
        }

        if tokens.len() == 1 {
            match tokens[0] {
                "daily" => return Self::daily(1),
                "weekly" => {
                    return Err(RecurrenceRuleError::WeeklyDaysRequired);
                }
                "monthly" => return Err(RecurrenceRuleError::MissingDayOfMonth),
                _ => {}
            }
        }

        let (interval, unit_index) = if tokens[0] == "every" {
            if tokens.len() < 2 {
                return Err(RecurrenceRuleError::InvalidSyntax(normalized));
            }
            match tokens[1].parse::<u32>() {
                Ok(interval) => {
                    if interval == 0 {
                        return Err(RecurrenceRuleError::IntervalMustBePositive);
                    }
                    (interval, 2)
                }
                Err(_) => (1, 1),
            }
        } else if matches!(tokens[0], "daily" | "weekly" | "monthly") {
            (1, 0)
        } else {
            return Err(RecurrenceRuleError::InvalidSyntax(normalized));
        };

        let unit = tokens
            .get(unit_index)
            .ok_or_else(|| RecurrenceRuleError::InvalidSyntax(normalized.clone()))?;
        let unit = unit.trim_end_matches(',');
        match unit {
            "day" | "days" => {
                if tokens.len() != unit_index + 1 {
                    return Err(RecurrenceRuleError::InvalidSyntax(normalized));
                }
                Self::daily(interval)
            }
            "week" | "weeks" => {
                let on_index = unit_index + 1;
                if tokens.get(on_index) != Some(&"on") {
                    return Err(RecurrenceRuleError::WeeklyDaysRequired);
                }
                parse_weekdays(&tokens[on_index + 1..])
                    .and_then(|weekdays| Self::weekly(interval, weekdays))
            }
            "month" | "months" => {
                let on_index = unit_index + 1;
                if tokens.get(on_index) != Some(&"on") {
                    return Err(RecurrenceRuleError::MissingDayOfMonth);
                }
                let day_tokens: Vec<&str> = tokens[on_index + 1..]
                    .iter()
                    .copied()
                    .filter(|token| *token != "the")
                    .collect();
                if day_tokens.len() != 1 {
                    return Err(RecurrenceRuleError::InvalidSyntax(normalized));
                }
                let day = parse_month_day(day_tokens[0])?;
                Self::monthly(interval, day)
            }
            _ => Err(RecurrenceRuleError::InvalidSyntax(normalized)),
        }
    }

    pub fn first_on_or_after(
        &self,
        start_on: CalendarDate,
        target: CalendarDate,
    ) -> Result<CalendarDate, RecurrenceError> {
        self.validate()?;
        let target = target.max(start_on);
        match self.frequency {
            RecurrenceFrequency::Daily => self.first_daily(start_on, target),
            RecurrenceFrequency::Weekly => self.first_weekly(start_on, target),
            RecurrenceFrequency::Monthly => self.first_monthly(start_on, target),
        }
    }

    pub fn next_occurrence_after(
        &self,
        start_on: CalendarDate,
        after: CalendarDate,
    ) -> Result<CalendarDate, RecurrenceError> {
        self.validate()?;
        if after < start_on {
            return self.first_on_or_after(start_on, start_on);
        }
        let target = after
            .checked_add_days(1)
            .ok_or(RecurrenceError::NoFutureOccurrence)?;
        self.first_on_or_after(start_on, target)
    }

    pub fn occurrence_number(
        &self,
        start_on: CalendarDate,
        occurrence_on: CalendarDate,
    ) -> Result<Option<u64>, RecurrenceError> {
        self.validate()?;
        if occurrence_on < start_on {
            return Ok(None);
        }

        match self.frequency {
            RecurrenceFrequency::Daily => {
                let days = start_on.days_until(occurrence_on);
                let interval = i64::from(self.interval);
                if days % interval != 0 {
                    return Ok(None);
                }
                Ok(Some((days / interval) as u64 + 1))
            }
            RecurrenceFrequency::Weekly => self.weekly_occurrence_number(start_on, occurrence_on),
            RecurrenceFrequency::Monthly => self.monthly_occurrence_number(start_on, occurrence_on),
        }
    }

    fn first_daily(
        &self,
        start_on: CalendarDate,
        target: CalendarDate,
    ) -> Result<CalendarDate, RecurrenceError> {
        let days = start_on.days_until(target);
        let interval = i64::from(self.interval);
        let steps = if days <= 0 {
            0
        } else {
            (days - 1) / interval + 1
        };
        add_days(
            start_on,
            steps
                .checked_mul(interval)
                .ok_or(RecurrenceError::DateOverflow)?,
        )
    }

    fn first_weekly(
        &self,
        start_on: CalendarDate,
        target: CalendarDate,
    ) -> Result<CalendarDate, RecurrenceError> {
        let anchor = start_of_week(start_on)?;
        let target_week = start_of_week(target)?;
        let weeks_since_anchor = anchor.days_until(target_week) / 7;
        let interval = i64::from(self.interval);
        let mut cycle = weeks_since_anchor.div_euclid(interval);
        let mut weekdays = self.weekdays.clone();
        weekdays.sort_unstable();

        loop {
            let week_offset = cycle
                .checked_mul(interval)
                .and_then(|weeks| weeks.checked_mul(7))
                .ok_or(RecurrenceError::DateOverflow)?;
            let week_start = add_days(anchor, week_offset)?;
            let mut result = None;
            for weekday in &weekdays {
                let candidate = add_days(week_start, weekday.offset_from_monday())?;
                if candidate >= start_on
                    && candidate >= target
                    && result.is_none_or(|current| candidate < current)
                {
                    result = Some(candidate);
                }
            }
            if let Some(result) = result {
                return Ok(result);
            }
            cycle = cycle.checked_add(1).ok_or(RecurrenceError::DateOverflow)?;
        }
    }

    fn first_monthly(
        &self,
        start_on: CalendarDate,
        target: CalendarDate,
    ) -> Result<CalendarDate, RecurrenceError> {
        let day = self.day_of_month.ok_or(RecurrenceError::Rule(
            RecurrenceRuleError::MissingDayOfMonth,
        ))?;
        let month_delta = month_index(start_on, target);
        let interval = i64::from(self.interval);
        let mut cycle = if month_delta <= 0 {
            0
        } else {
            month_delta.div_euclid(interval)
        };

        loop {
            let months = cycle
                .checked_mul(interval)
                .ok_or(RecurrenceError::DateOverflow)?;
            let candidate = add_months_on_day(start_on, months, day)?;
            if candidate >= start_on && candidate >= target {
                return Ok(candidate);
            }
            cycle = cycle.checked_add(1).ok_or(RecurrenceError::DateOverflow)?;
        }
    }

    fn weekly_occurrence_number(
        &self,
        start_on: CalendarDate,
        occurrence_on: CalendarDate,
    ) -> Result<Option<u64>, RecurrenceError> {
        let anchor = start_of_week(start_on)?;
        let occurrence_week = start_of_week(occurrence_on)?;
        let weeks = anchor.days_until(occurrence_week) / 7;
        let interval = i64::from(self.interval);
        if weeks < 0 || weeks % interval != 0 {
            return Ok(None);
        }

        let weekday = Weekday::from_chrono(occurrence_on.chrono_weekday());
        let mut weekdays = self.weekdays.clone();
        weekdays.sort_unstable();
        let Some(position) = weekdays.iter().position(|value| *value == weekday) else {
            return Ok(None);
        };
        let expected = add_days(occurrence_week, weekday.offset_from_monday())?;
        if expected != occurrence_on {
            return Ok(None);
        }

        let cycle = weeks / interval;
        let first_cycle_count = weekdays
            .iter()
            .filter_map(|value| add_days(anchor, value.offset_from_monday()).ok())
            .filter(|date| *date >= start_on)
            .count();
        let first_cycle = if first_cycle_count > 0 { 0_i64 } else { 1_i64 };
        if cycle < first_cycle {
            return Ok(None);
        }

        let position = if cycle == 0 && first_cycle == 0 {
            let filtered_position = weekdays
                .iter()
                .filter(|value| match add_days(anchor, value.offset_from_monday()) {
                    Ok(date) => date >= start_on,
                    Err(_) => false,
                })
                .position(|value| *value == weekday);
            match filtered_position {
                Some(position) => position,
                None => return Ok(None),
            }
        } else {
            position
        };
        let prior_cycles = (cycle - first_cycle) as u64;
        let first_count = if first_cycle == 0 {
            first_cycle_count as u64
        } else {
            weekdays.len() as u64
        };
        if cycle == first_cycle {
            Ok(Some(position as u64 + 1))
        } else {
            Ok(Some(
                first_count + (prior_cycles - 1) * weekdays.len() as u64 + position as u64 + 1,
            ))
        }
    }

    fn monthly_occurrence_number(
        &self,
        start_on: CalendarDate,
        occurrence_on: CalendarDate,
    ) -> Result<Option<u64>, RecurrenceError> {
        let day = self.day_of_month.ok_or(RecurrenceError::Rule(
            RecurrenceRuleError::MissingDayOfMonth,
        ))?;
        let delta = month_index(start_on, occurrence_on);
        let interval = i64::from(self.interval);
        if delta < 0 || delta % interval != 0 {
            return Ok(None);
        }
        let cycle = delta / interval;
        let expected = add_months_on_day(start_on, delta, day)?;
        if expected != occurrence_on {
            return Ok(None);
        }
        let first_candidate = add_months_on_day(start_on, 0, day)?;
        let first_cycle = if first_candidate >= start_on {
            0_i64
        } else {
            1_i64
        };
        if cycle < first_cycle {
            return Ok(None);
        }
        Ok(Some((cycle - first_cycle) as u64 + 1))
    }
}

impl FromStr for RecurrenceRule {
    type Err = RecurrenceRuleError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurrenceSeries {
    pub series_id: i64,
    pub template_entry_id: i64,
    pub board_id: i64,
    pub list_id: i64,
    pub start_on: CalendarDate,
    pub rule: RecurrenceRule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_on: Option<CalendarDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence_limit: Option<u32>,
    #[serde(default)]
    pub generation_mode: GenerationMode,
    #[serde(default = "default_active")]
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecurrenceError {
    Rule(RecurrenceRuleError),
    EndBeforeStart,
    OccurrenceLimitMustBePositive,
    WindowEndBeforeStart,
    DateOverflow,
    NoFutureOccurrence,
}

impl fmt::Display for RecurrenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rule(error) => error.fmt(formatter),
            Self::EndBeforeStart => {
                formatter.write_str("recurrence end date cannot be before its start date")
            }
            Self::OccurrenceLimitMustBePositive => {
                formatter.write_str("occurrence limit must be positive")
            }
            Self::WindowEndBeforeStart => {
                formatter.write_str("projection end date cannot be before its start date")
            }
            Self::DateOverflow => {
                formatter.write_str("recurrence date is outside the supported date range")
            }
            Self::NoFutureOccurrence => formatter.write_str("recurrence has no future occurrence"),
        }
    }
}

impl std::error::Error for RecurrenceError {}

impl From<RecurrenceRuleError> for RecurrenceError {
    fn from(value: RecurrenceRuleError) -> Self {
        Self::Rule(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurrenceOccurrence {
    pub series_id: i64,
    pub template_entry_id: i64,
    pub occurrence_key: String,
    pub occurrence_on: CalendarDate,
}

impl RecurrenceSeries {
    pub fn new(
        series_id: i64,
        template_entry_id: i64,
        board_id: i64,
        list_id: i64,
        start_on: CalendarDate,
        rule: RecurrenceRule,
    ) -> Result<Self, RecurrenceError> {
        let series = Self {
            series_id,
            template_entry_id,
            board_id,
            list_id,
            start_on,
            rule,
            end_on: None,
            occurrence_limit: None,
            generation_mode: GenerationMode::OnCompletion,
            active: true,
        };
        series.validate().map(|()| series)
    }

    pub fn validate(&self) -> Result<(), RecurrenceError> {
        self.rule.validate()?;
        if self.end_on.is_some_and(|end_on| end_on < self.start_on) {
            return Err(RecurrenceError::EndBeforeStart);
        }
        if self.occurrence_limit == Some(0) {
            return Err(RecurrenceError::OccurrenceLimitMustBePositive);
        }
        Ok(())
    }

    pub fn first_occurrence(&self) -> Result<Option<RecurrenceOccurrence>, RecurrenceError> {
        self.validate()?;
        if !self.active || self.occurrence_limit == Some(0) {
            return Ok(None);
        }
        let occurrence_on = self.rule.first_on_or_after(self.start_on, self.start_on)?;
        if !self.is_allowed_occurrence(occurrence_on, 1) {
            return Ok(None);
        }
        Ok(Some(self.occurrence(occurrence_on)))
    }

    pub fn next_occurrence_after(
        &self,
        after: CalendarDate,
    ) -> Result<Option<RecurrenceOccurrence>, RecurrenceError> {
        self.validate()?;
        if !self.active {
            return Ok(None);
        }
        let occurrence_on = match self.rule.next_occurrence_after(self.start_on, after) {
            Ok(date) => date,
            Err(RecurrenceError::NoFutureOccurrence) => return Ok(None),
            Err(error) => return Err(error),
        };
        let number = self
            .rule
            .occurrence_number(self.start_on, occurrence_on)?
            .ok_or(RecurrenceError::DateOverflow)?;
        if !self.is_allowed_occurrence(occurrence_on, number) {
            return Ok(None);
        }
        Ok(Some(self.occurrence(occurrence_on)))
    }

    pub fn occurrences_between(
        &self,
        from: CalendarDate,
        through: CalendarDate,
        max_occurrences: usize,
    ) -> Result<Vec<RecurrenceOccurrence>, RecurrenceError> {
        self.validate()?;
        if through < from {
            return Err(RecurrenceError::WindowEndBeforeStart);
        }
        if !self.active || max_occurrences == 0 {
            return Ok(Vec::new());
        }

        let target = from.max(self.start_on);
        let mut occurrence_on = self.rule.first_on_or_after(self.start_on, target)?;
        let mut occurrences = Vec::new();
        while occurrence_on <= through && occurrences.len() < max_occurrences {
            let number = self
                .rule
                .occurrence_number(self.start_on, occurrence_on)?
                .ok_or(RecurrenceError::DateOverflow)?;
            if self.is_allowed_occurrence(occurrence_on, number) {
                if occurrence_on >= from {
                    occurrences.push(self.occurrence(occurrence_on));
                }
            } else if self.has_reached_end(occurrence_on, number) {
                break;
            }

            occurrence_on = match self
                .rule
                .next_occurrence_after(self.start_on, occurrence_on)
            {
                Ok(date) => date,
                Err(RecurrenceError::NoFutureOccurrence) => break,
                Err(error) => return Err(error),
            };
        }

        Ok(occurrences)
    }

    fn occurrence(&self, occurrence_on: CalendarDate) -> RecurrenceOccurrence {
        RecurrenceOccurrence {
            series_id: self.series_id,
            template_entry_id: self.template_entry_id,
            occurrence_key: occurrence_on.to_string(),
            occurrence_on,
        }
    }

    fn is_allowed_occurrence(&self, occurrence_on: CalendarDate, number: u64) -> bool {
        !self.has_reached_end(occurrence_on, number)
    }

    fn has_reached_end(&self, occurrence_on: CalendarDate, number: u64) -> bool {
        self.end_on.is_some_and(|end_on| occurrence_on > end_on)
            || self
                .occurrence_limit
                .is_some_and(|limit| number > u64::from(limit))
    }
}

pub fn project_occurrences(
    series: &RecurrenceSeries,
    from: CalendarDate,
    through: CalendarDate,
    max_occurrences: usize,
) -> Result<Vec<RecurrenceOccurrence>, RecurrenceError> {
    series.occurrences_between(from, through, max_occurrences)
}

pub fn expand_series(
    series: &RecurrenceSeries,
    from: CalendarDate,
    through: CalendarDate,
    max_occurrences: usize,
) -> Result<Vec<RecurrenceOccurrence>, RecurrenceError> {
    project_occurrences(series, from, through, max_occurrences)
}

fn parse_weekdays(tokens: &[&str]) -> Result<Vec<Weekday>, RecurrenceRuleError> {
    let joined = tokens.join(" ");
    let mut weekdays = Vec::new();
    for token in joined.split(|character: char| {
        character == ',' || character == '/' || character == ';' || character.is_whitespace()
    }) {
        if token.is_empty() {
            continue;
        }
        let weekday = Weekday::parse(token)
            .ok_or_else(|| RecurrenceRuleError::InvalidWeekday(token.to_string()))?;
        if weekdays.contains(&weekday) {
            return Err(RecurrenceRuleError::DuplicateWeekday);
        }
        weekdays.push(weekday);
    }
    if weekdays.is_empty() {
        return Err(RecurrenceRuleError::WeeklyDaysRequired);
    }
    Ok(weekdays)
}

fn parse_month_day(value: &str) -> Result<u8, RecurrenceRuleError> {
    let trimmed = value
        .trim()
        .trim_end_matches(['s', 't', 'n', 'd', 'r', 'h']);
    let day = trimmed
        .parse::<u16>()
        .map_err(|_| RecurrenceRuleError::InvalidSyntax(value.to_string()))?;
    if !(1..=31).contains(&day) {
        return Err(RecurrenceRuleError::MonthlyDayOutOfRange);
    }
    Ok(day as u8)
}

fn add_days(date: CalendarDate, days: i64) -> Result<CalendarDate, RecurrenceError> {
    date.checked_add_days(days)
        .ok_or(RecurrenceError::DateOverflow)
}

fn start_of_week(date: CalendarDate) -> Result<CalendarDate, RecurrenceError> {
    let weekday = Weekday::from_chrono(date.chrono_weekday());
    add_days(date, -weekday.offset_from_monday())
}

fn month_index(start_on: CalendarDate, target: CalendarDate) -> i64 {
    (i64::from(target.year()) * 12 + i64::from(target.month0()))
        - (i64::from(start_on.year()) * 12 + i64::from(start_on.month0()))
}

fn add_months_on_day(
    base: CalendarDate,
    months: i64,
    day: u8,
) -> Result<CalendarDate, RecurrenceError> {
    let base_index = i64::from(base.year()) * 12 + i64::from(base.month0());
    let target_index = base_index
        .checked_add(months)
        .ok_or(RecurrenceError::DateOverflow)?;
    let year = target_index.div_euclid(12);
    if year < i64::from(i32::MIN) || year > i64::from(i32::MAX) {
        return Err(RecurrenceError::DateOverflow);
    }
    let month = target_index.rem_euclid(12) as u32 + 1;
    let day = u32::from(day).min(days_in_month(year as i32, month));
    CalendarDate::from_ymd(year as i32, month, day).map_err(|error| match error {
        CalendarDateError::InvalidFormat | CalendarDateError::InvalidDate => {
            RecurrenceError::DateOverflow
        }
    })
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

const fn default_interval() -> u32 {
    1
}

const fn default_active() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use serde_json::{from_str, to_string};

    use super::*;
    use crate::CalendarEvent;

    fn date(value: &str) -> CalendarDate {
        match CalendarDate::parse(value) {
            Ok(value) => value,
            Err(error) => panic!("invalid test date {value}: {error}"),
        }
    }

    fn series(rule: RecurrenceRule) -> RecurrenceSeries {
        match RecurrenceSeries::new(7, 42, 3, 9, date("2024-01-01"), rule) {
            Ok(series) => series,
            Err(error) => panic!("invalid test series: {error}"),
        }
    }

    #[test]
    fn parses_daily_weekly_and_monthly_rules() {
        assert_eq!(RecurrenceRule::parse("daily"), RecurrenceRule::daily(1));
        assert_eq!(
            RecurrenceRule::parse("every 2 weeks on mon, wed"),
            RecurrenceRule::weekly(2, vec![Weekday::Monday, Weekday::Wednesday])
        );
        assert_eq!(
            RecurrenceRule::parse("every month on the 31st"),
            RecurrenceRule::monthly(1, 31)
        );
    }

    #[test]
    fn rejects_invalid_rules() {
        assert_eq!(
            RecurrenceRule::daily(0),
            Err(RecurrenceRuleError::IntervalMustBePositive)
        );
        assert_eq!(
            RecurrenceRule::weekly(1, Vec::new()),
            Err(RecurrenceRuleError::WeeklyDaysRequired)
        );
        assert_eq!(
            RecurrenceRule::monthly(1, 0),
            Err(RecurrenceRuleError::MonthlyDayOutOfRange)
        );
        assert_eq!(
            RecurrenceRule::parse("every 2 weeks on monday, monday"),
            Err(RecurrenceRuleError::DuplicateWeekday)
        );
        assert!(RecurrenceRule::parse("every 4 months").is_err());
    }

    #[test]
    fn calculates_daily_and_weekly_occurrences() {
        let daily = ok(RecurrenceRule::daily(2));
        assert_eq!(
            daily.first_on_or_after(date("2024-01-01"), date("2024-01-04")),
            Ok(date("2024-01-05"))
        );

        let weekly = ok(RecurrenceRule::weekly(
            1,
            vec![Weekday::Monday, Weekday::Wednesday],
        ));
        assert_eq!(
            weekly.first_on_or_after(date("2024-01-01"), date("2024-01-02")),
            Ok(date("2024-01-03"))
        );
        assert_eq!(
            weekly.next_occurrence_after(date("2024-01-01"), date("2024-01-03")),
            Ok(date("2024-01-08"))
        );
    }

    #[test]
    fn clamps_monthly_days_and_preserves_leap_year_behavior() {
        let monthly = ok(RecurrenceRule::monthly(1, 29));
        assert_eq!(
            monthly.first_on_or_after(date("2024-02-01"), date("2024-02-01")),
            Ok(date("2024-02-29"))
        );
        assert_eq!(
            monthly.next_occurrence_after(date("2024-02-01"), date("2024-02-29")),
            Ok(date("2024-03-29"))
        );
        assert_eq!(
            monthly.first_on_or_after(date("2024-02-01"), date("2025-02-01")),
            Ok(date("2025-02-28"))
        );
        assert_eq!(
            monthly.first_on_or_after(date("2024-02-01"), date("2028-02-01")),
            Ok(date("2028-02-29"))
        );
    }

    #[test]
    fn projects_bounded_occurrences_with_series_limits() {
        let rule = ok(RecurrenceRule::daily(1));
        let mut recurrence = series(rule);
        recurrence.end_on = Some(date("2024-01-05"));
        recurrence.occurrence_limit = Some(3);

        let occurrences = ok(project_occurrences(
            &recurrence,
            date("2024-01-02"),
            date("2024-01-10"),
            20,
        ));
        assert_eq!(
            occurrences
                .iter()
                .map(|occurrence| occurrence.occurrence_key.as_str())
                .collect::<Vec<_>>(),
            vec!["2024-01-02", "2024-01-03"]
        );
    }

    #[test]
    fn projection_limit_is_independent_from_schedule_limit() {
        let rule = ok(RecurrenceRule::daily(1));
        let recurrence = series(rule);
        let occurrences = ok(expand_series(
            &recurrence,
            date("2024-01-01"),
            date("2024-01-31"),
            3,
        ));
        assert_eq!(occurrences.len(), 3);
        assert_eq!(occurrences[2].occurrence_on, date("2024-01-03"));
    }

    #[test]
    fn serializes_dates_and_series_for_storage_boundaries() {
        let recurrence = series(ok(RecurrenceRule::weekly(
            1,
            vec![Weekday::Tuesday, Weekday::Thursday],
        )));
        let encoded = ok(to_string(&recurrence));
        assert!(encoded.contains("2024-01-01"));
        let decoded: RecurrenceSeries = ok(from_str(&encoded));
        assert_eq!(decoded, recurrence);
    }

    #[test]
    fn calendar_event_rejects_inverted_date_ranges() {
        let event = CalendarEvent {
            entry_id: 1,
            board_id: 2,
            list_id: 3,
            title: "Planning".to_string(),
            start_on: Some(date("2024-05-10")),
            due_on: Some(date("2024-05-09")),
            completed: false,
            archived: false,
            recurrence_series_id: None,
        };
        assert_eq!(
            event.validate(),
            Err(crate::CalendarEventError::DueBeforeStart)
        );
        assert!(!event.occurs_on(date("2024-05-09")));
    }

    #[test]
    fn calendar_dates_are_strict_and_leap_aware() {
        assert_eq!(CalendarDate::parse("2024-02-29"), Ok(date("2024-02-29")));
        assert_eq!(
            CalendarDate::parse("2023-02-29"),
            Err(CalendarDateError::InvalidDate)
        );
        assert_eq!(
            CalendarDate::parse("2024/02/29"),
            Err(CalendarDateError::InvalidFormat)
        );
    }

    fn ok<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("unexpected error: {error}"),
        }
    }
}
