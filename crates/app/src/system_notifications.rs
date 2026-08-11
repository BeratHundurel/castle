use std::{sync::Arc, sync::OnceLock, time::Duration};

use chrono::Local;
use sea_orm::DatabaseConnection;
use storage::reminders::DueReminder;
use tokio::sync::Notify;

static REMINDER_WAKE: OnceLock<Arc<Notify>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NotificationAvailability {
    Enabled,
    DisabledForApplication,
    DisabledForUser,
    DisabledByPolicy,
    Unsupported,
    Unavailable,
}

impl NotificationAvailability {
    pub(crate) fn can_open_settings(self) -> bool {
        matches!(self, Self::DisabledForApplication | Self::DisabledForUser)
    }
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

#[cfg(target_os = "windows")]
pub(crate) fn availability() -> NotificationAvailability {
    use windows::{
        UI::Notifications::{NotificationSetting, ToastNotificationManager},
        core::HSTRING,
    };

    let Ok(notifier) =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(notification_app_id()))
    else {
        return NotificationAvailability::Unavailable;
    };
    let Ok(setting) = notifier.Setting() else {
        return NotificationAvailability::Unavailable;
    };

    match setting {
        NotificationSetting::Enabled => NotificationAvailability::Enabled,
        NotificationSetting::DisabledForApplication => {
            NotificationAvailability::DisabledForApplication
        }
        NotificationSetting::DisabledForUser => NotificationAvailability::DisabledForUser,
        NotificationSetting::DisabledByGroupPolicy => NotificationAvailability::DisabledByPolicy,
        NotificationSetting::DisabledByManifest => NotificationAvailability::Unsupported,
        _ => NotificationAvailability::Unavailable,
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn availability() -> NotificationAvailability {
    NotificationAvailability::Unsupported
}

pub(crate) fn show_test_notification() -> anyhow::Result<()> {
    ensure_notifications_available()?;
    show_toast(
        "Castle notifications are working",
        "Test notification",
        "Card reminders will appear here when they become due.",
        false,
    )
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
    ensure_notifications_available()?;
    show_toast(
        "Castle · Card due",
        &reminder.title,
        &format!(
            "{} · {} · due {}",
            reminder.board_title, reminder.list_title, reminder.due_on
        ),
        true,
    )
}

#[cfg(not(target_os = "windows"))]
fn show_system_notification(_: &DueReminder) -> anyhow::Result<()> {
    anyhow::bail!("system notifications are not implemented for this platform")
}

fn ensure_notifications_available() -> anyhow::Result<()> {
    match availability() {
        NotificationAvailability::Enabled => Ok(()),
        NotificationAvailability::DisabledForApplication => {
            anyhow::bail!("Windows notifications are disabled for Castle")
        }
        NotificationAvailability::DisabledForUser => {
            anyhow::bail!("Windows notifications are turned off")
        }
        NotificationAvailability::DisabledByPolicy => {
            anyhow::bail!("Windows notifications are blocked by system policy")
        }
        NotificationAvailability::Unsupported => {
            anyhow::bail!("system notifications are not supported on this platform")
        }
        NotificationAvailability::Unavailable => {
            anyhow::bail!("Castle could not access the system notification service")
        }
    }
}

#[cfg(target_os = "windows")]
fn show_toast(title: &str, text1: &str, text2: &str, is_reminder: bool) -> anyhow::Result<()> {
    use tauri_winrt_notification::{Scenario, Sound, Toast};

    let toast = Toast::new(notification_app_id())
        .title(title)
        .text1(text1)
        .text2(text2);
    let toast = if is_reminder {
        toast
            .scenario(Scenario::Reminder)
            .sound(Some(Sound::Reminder))
    } else {
        toast
    };

    toast
        .show()
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

#[cfg(not(target_os = "windows"))]
fn show_toast(_: &str, _: &str, _: &str, _: bool) -> anyhow::Result<()> {
    anyhow::bail!("system notifications are not implemented for this platform")
}

#[cfg(target_os = "windows")]
fn notification_app_id() -> &'static str {
    notification_app_id_for_registration(castle_shortcut_is_registered())
}

#[cfg(target_os = "windows")]
fn notification_app_id_for_registration(registered: bool) -> &'static str {
    if registered {
        "Castle.App"
    } else {
        tauri_winrt_notification::Toast::POWERSHELL_APP_ID
    }
}

#[cfg(target_os = "windows")]
fn castle_shortcut_is_registered() -> bool {
    ["APPDATA", "PROGRAMDATA"].into_iter().any(|variable| {
        std::env::var_os(variable).is_some_and(|directory| {
            std::path::PathBuf::from(directory)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Castle.lnk")
                .is_file()
        })
    })
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn portable_builds_use_a_registered_notification_identity() {
        assert_eq!(
            notification_app_id_for_registration(false),
            tauri_winrt_notification::Toast::POWERSHELL_APP_ID
        );
        assert_eq!(notification_app_id_for_registration(true), "Castle.App");
    }
}
