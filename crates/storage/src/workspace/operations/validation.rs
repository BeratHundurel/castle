use anyhow::{Context as _, Result, bail};
use chrono::NaiveDate;

pub(super) fn required_text(value: String, field: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(value.to_string())
}

pub(super) fn validate_due_on(due_on: Option<&str>) -> Result<()> {
    validate_date(due_on, "due_on")
}

pub(super) fn validate_start_on(start_on: Option<&str>) -> Result<()> {
    validate_date(start_on, "start_on")
}

fn validate_date(value: Option<&str>, field: &str) -> Result<()> {
    if let Some(value) = value {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .with_context(|| format!("{field} must use YYYY-MM-DD, received {value:?}"))?;
    }
    Ok(())
}
