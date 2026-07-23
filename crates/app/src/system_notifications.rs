use std::{sync::Arc, sync::OnceLock, time::Duration};

use chrono::Local;
use sea_orm::{ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use tokio::sync::Notify;

use entity::entry;

static REMINDER_WAKE: OnceLock<Arc<Notify>> = OnceLock::new();

struct DueReminder {
    entry_id: i64,
    title: String,
    due_on: String,
    board_title: String,
    list_title: String,
}

pub fn start(db: Arc<DatabaseConnection>) {
    let wake = REMINDER_WAKE
        .get_or_init(|| Arc::new(Notify::new()))
        .clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = wake.notified() => {}
            }
            if let Err(error) = deliver_due_reminders(db.as_ref()).await {
                eprintln!("Failed to deliver card reminders: {error}");
            }
        }
    });
}

pub(crate) fn wake() {
    if let Some(wake) = REMINDER_WAKE.get() {
        wake.notify_one();
    }
}

async fn deliver_due_reminders(db: &DatabaseConnection) -> anyhow::Result<()> {
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let rows = db
        .query_all_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT e.id, e.title, e.due_on, b.title AS board_title, c.title AS list_title
            FROM entry e
            JOIN card c ON c.id = e.card_id AND c.deleted_at IS NULL
            JOIN board b ON b.id = c.board_id AND b.deleted_at IS NULL
            LEFT JOIN project p ON p.id = b.project_id
            WHERE e.deleted_at IS NULL
              AND (p.id IS NULL OR p.deleted_at IS NULL)
              AND e.reminder_enabled = 1
              AND e.due_on IS NOT NULL
              AND e.due_on <= ?
              AND (e.reminder_notified_for IS NULL OR e.reminder_notified_for <> e.due_on)
            ORDER BY e.due_on, e.id
            "#,
            [today.into()],
        ))
        .await?;

    for row in rows {
        let reminder = DueReminder {
            entry_id: row.try_get("", "id")?,
            title: row.try_get("", "title")?,
            due_on: row.try_get("", "due_on")?,
            board_title: row.try_get("", "board_title")?,
            list_title: row.try_get("", "list_title")?,
        };
        show_system_notification(&reminder)?;
        entry::ActiveModel {
            id: sea_orm::ActiveValue::Set(reminder.entry_id),
            reminder_notified_for: sea_orm::ActiveValue::Set(Some(reminder.due_on)),
            ..Default::default()
        }
        .update(db)
        .await?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn show_system_notification(reminder: &DueReminder) -> anyhow::Result<()> {
    use winrt_notification::{Scenario, Sound, Toast};

    fn build(app_id: &str, reminder: &DueReminder) -> Toast {
        Toast::new(app_id)
            .title("Castle · Card due")
            .text1(&reminder.title)
            .text2(&format!(
                "{} · {} · due {}",
                reminder.board_title, reminder.list_title, reminder.due_on
            ))
            .scenario(Scenario::Reminder)
            .sound(Some(Sound::Reminder))
    }

    match build("Castle.App", reminder).show() {
        Ok(()) => Ok(()),
        Err(_) => build(Toast::POWERSHELL_APP_ID, reminder)
            .show()
            .map_err(|error| anyhow::anyhow!(error.to_string())),
    }
}

#[cfg(not(target_os = "windows"))]
fn show_system_notification(_: &DueReminder) -> anyhow::Result<()> {
    anyhow::bail!("system notifications are not implemented for this platform")
}
