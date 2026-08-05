use std::{sync::Arc, sync::OnceLock, time::Duration};

use chrono::Local;
use sea_orm::DatabaseConnection;
use storage::reminders::DueReminder;
use tokio::sync::Notify;

static REMINDER_WAKE: OnceLock<Arc<Notify>> = OnceLock::new();

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
    for reminder in storage::reminders::load_due_reminders(db, &today).await? {
        show_system_notification(&reminder)?;
        storage::reminders::mark_reminder_notified(db, reminder.entry_id, reminder.due_on).await?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn show_system_notification(reminder: &DueReminder) -> anyhow::Result<()> {
    use tauri_winrt_notification::{Scenario, Sound, Toast};

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
