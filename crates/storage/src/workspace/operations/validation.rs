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
    if let Some(due_on) = due_on {
        NaiveDate::parse_from_str(due_on, "%Y-%m-%d")
            .with_context(|| format!("due_on must use YYYY-MM-DD, received {due_on:?}"))?;
    }
    Ok(())
}
