use std::{path::Path, sync::Arc, sync::OnceLock, time::Duration};

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use notify::Watcher;
use storage::Store;
use storage::note::reminders::DueReminder;
use tokio::sync::Notify;

use ::board::{NotificationAvailability, NotificationGateway};

static REMINDER_WAKE: OnceLock<Arc<Notify>> = OnceLock::new();

struct SystemNotificationGateway;

impl NotificationGateway for SystemNotificationGateway {
    fn availability(&self) -> NotificationAvailability {
        availability()
    }

    fn wake(&self) {
        wake();
    }

    fn show_test_notification(&self) -> anyhow::Result<()> {
        show_test_notification()
    }
}

pub fn install_board_gateway(cx: &mut gpui_kit::App) {
    ::board::init_with_notification_gateway(cx, Arc::new(SystemNotificationGateway));
}

const RETRY_DELAY: Duration = Duration::from_secs(300);
const MAX_DEADLINE_SLEEP: Duration = Duration::from_secs(12 * 60 * 60);

type ReminderPresenter = dyn Fn(&DueReminder) -> anyhow::Result<()> + Send + Sync;

pub fn start(store: Store, database_path: &Path) {
    let wake = REMINDER_WAKE
        .get_or_init(|| Arc::new(Notify::new()))
        .clone();

    let database_watcher = match watch_database_changes(database_path, wake.clone()) {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            eprintln!("Failed to watch Castle database for reminder changes: {error}");
            None
        }
    };

    let fallback_rescan = database_watcher.is_none().then_some(RETRY_DELAY);
    tokio::spawn(async move {
        let _database_watcher = database_watcher;
        run_reminder_scheduler(
            store,
            wake,
            Arc::new(show_system_notification),
            fallback_rescan,
        )
        .await;
    });
}

fn watch_database_changes(
    database_path: &Path,
    wake: Arc<Notify>,
) -> notify::Result<notify::RecommendedWatcher> {
    let database_path = database_path.canonicalize().map_err(notify::Error::io)?;
    let watched_path = database_path.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|event| is_database_change(&event, &watched_path)) {
            wake.notify_one();
        }
    })?;
    let directory = database_path.parent().unwrap_or_else(|| Path::new("."));
    watcher.watch(directory, notify::RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

fn is_database_change(event: &notify::Event, database_path: &Path) -> bool {
    let wal_path = database_path.with_file_name(format!(
        "{}-wal",
        database_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    ));
    event
        .paths
        .iter()
        .any(|path| path == database_path || path == &wal_path)
}

async fn run_reminder_scheduler(
    store: Store,
    wake: Arc<Notify>,
    present: Arc<ReminderPresenter>,
    fallback_rescan: Option<Duration>,
) {
    loop {
        let now = Local::now();
        let today = now.date_naive().format("%Y-%m-%d").to_string();
        if let Err(error) = deliver_due_reminders(&store, &today, present.as_ref()).await {
            eprintln!("Failed to deliver card reminders: {error}");
            wait_for_wake_or_retry(&wake).await;
            continue;
        }

        match storage::note::reminders::next_pending_reminder_due_on(&store).await {
            Ok(Some(due_on)) => match NaiveDate::parse_from_str(&due_on, "%Y-%m-%d") {
                Ok(date) => {
                    let delay = duration_until_due_date(date, Local::now());
                    if delay.is_zero() {
                        continue;
                    }
                    let delay = delay.min(MAX_DEADLINE_SLEEP);
                    let delay = fallback_rescan.map_or(delay, |fallback| delay.min(fallback));
                    wait_for_deadline_or_wake(&wake, Some(delay)).await;
                }
                Err(error) => {
                    eprintln!("Invalid card reminder due date {due_on}: {error}");
                    wait_for_wake_or_retry(&wake).await;
                }
            },
            Ok(None) => wait_for_deadline_or_wake(&wake, fallback_rescan).await,
            Err(error) => {
                eprintln!("Failed to schedule card reminders: {error}");
                wait_for_wake_or_retry(&wake).await;
            }
        }
    }
}

fn duration_until_due_date(date: NaiveDate, now: DateTime<Local>) -> Duration {
    for hour in 0..24 {
        let Some(local_time) = date.and_hms_opt(hour, 0, 0) else {
            continue;
        };
        if let Some(deadline) = Local.from_local_datetime(&local_time).earliest() {
            return deadline
                .signed_duration_since(now)
                .to_std()
                .unwrap_or_default();
        }
    }
    RETRY_DELAY
}

async fn wait_for_wake_or_retry(wake: &Notify) {
    wait_for_deadline_or_wake(wake, Some(RETRY_DELAY)).await;
}

async fn wait_for_deadline_or_wake(wake: &Notify, delay: Option<Duration>) {
    if let Some(delay) = delay {
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = wake.notified() => {}
        }
    } else {
        wake.notified().await;
    }
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

async fn deliver_due_reminders(
    store: &Store,
    today: &str,
    present: &ReminderPresenter,
) -> anyhow::Result<()> {
    let due = storage::note::reminders::load_due_reminders(store, today).await?;
    let mut notified = Vec::with_capacity(due.len());
    for reminder in &due {
        if let Err(error) = present(reminder) {
            if !notified.is_empty() {
                storage::note::reminders::mark_many_reminders_notified(store, &notified).await?;
            }
            return Err(error);
        }
        notified.push((reminder.entry_id, reminder.due_on.clone()));
    }
    if !notified.is_empty() {
        storage::note::reminders::mark_many_reminders_notified(store, &notified).await?;
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

#[cfg(test)]
mod scheduler_tests {
    use super::*;
    use anyhow::Result;
    use entity::{board, card, entry};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn store_with_due_reminders() -> Result<(Store, String, i64)> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let due_on = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let board = board::ActiveModel {
            title: Set("Delivery".to_string()),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let list = card::ActiveModel {
            title: Set("Today".to_string()),
            board_id: Set(board.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        for (position, title) in [(0, "First"), (1, "Second")] {
            entry::ActiveModel {
                title: Set(title.to_string()),
                description: Set(String::new()),
                card_id: Set(list.id),
                position: Set(position),
                due_on: Set(Some(due_on.clone())),
                reminder_enabled: Set(true),
                ..Default::default()
            }
            .insert(&db)
            .await?;
        }
        Ok((Store::from(db), due_on, list.id))
    }

    #[tokio::test]
    async fn delivery_marks_only_successfully_presented_reminders() -> Result<()> {
        let (store, today, _) = store_with_due_reminders().await?;
        let calls = AtomicUsize::new(0);
        let presenter = move |_: &DueReminder| {
            let call = calls.fetch_add(1, Ordering::Relaxed);
            if call == 1 {
                anyhow::bail!("notification service failed");
            }
            Ok(())
        };

        let error = deliver_due_reminders(&store, &today, &presenter)
            .await
            .expect_err("second notification should fail");
        assert_eq!(error.to_string(), "notification service failed");
        let remaining = storage::note::reminders::load_due_reminders(&store, &today).await?;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].title, "Second");

        deliver_due_reminders(&store, &today, &|_| Ok(())).await?;
        assert!(
            storage::note::reminders::load_due_reminders(&store, &today)
                .await?
                .is_empty()
        );
        Ok(())
    }

    #[tokio::test]
    async fn scheduler_reschedules_on_change_and_delivers_each_reminder_once() -> Result<()> {
        let (store, today, list_id) = store_with_due_reminders().await?;
        let wake = Arc::new(Notify::new());
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let presenter: Arc<ReminderPresenter> = Arc::new(move |reminder: &DueReminder| {
            sender.send(reminder.title.clone())?;
            Ok(())
        });
        let scheduler = tokio::spawn(run_reminder_scheduler(
            store.clone(),
            wake.clone(),
            presenter,
            None,
        ));

        for title in ["First", "Second"] {
            let delivered = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                .await?
                .ok_or_else(|| anyhow::anyhow!("scheduler stopped"))?;
            assert_eq!(delivered, title);
        }

        entry::ActiveModel {
            title: Set("Added later".to_string()),
            description: Set(String::new()),
            card_id: Set(list_id),
            position: Set(2),
            due_on: Set(Some(today.clone())),
            reminder_enabled: Set(true),
            ..Default::default()
        }
        .insert(&store)
        .await?;
        wake.notify_one();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), receiver.recv()).await?,
            Some("Added later".to_string())
        );

        wake.notify_one();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), receiver.recv())
                .await
                .is_err()
        );
        assert!(
            storage::note::reminders::load_due_reminders(&store, &today)
                .await?
                .is_empty()
        );
        scheduler.abort();
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn scheduler_waits_for_deadline_or_change_without_minute_ticks() {
        let wake = Arc::new(Notify::new());
        let deadline_wait = tokio::spawn({
            let wake = wake.clone();
            async move {
                wait_for_deadline_or_wake(&wake, Some(Duration::from_secs(3_600))).await;
            }
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(60)).await;
        assert!(!deadline_wait.is_finished());
        wake.notify_one();
        deadline_wait
            .await
            .expect("wake should interrupt the deadline");

        let idle_wait = tokio::spawn({
            let wake = wake.clone();
            async move { wait_for_deadline_or_wake(&wake, None).await }
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(86_400)).await;
        assert!(!idle_wait.is_finished());
        wake.notify_one();
        idle_wait
            .await
            .expect("a database change should interrupt idle wait");
    }

    #[test]
    fn due_date_is_scheduled_at_the_start_of_its_local_day() {
        let now = Local::now();
        assert_eq!(
            duration_until_due_date(now.date_naive(), now),
            Duration::ZERO
        );
        let tomorrow = now.date_naive().succ_opt().expect("date should advance");
        let delay = duration_until_due_date(tomorrow, now);
        assert!(delay > Duration::ZERO);
        assert!(delay <= Duration::from_secs(25 * 60 * 60));
    }

    #[test]
    fn database_watcher_accepts_database_and_wal_events() {
        use notify::{Event, EventKind};
        let database = Path::new("C:/data/castle.db");
        let event =
            |path| Event::new(EventKind::Modify(notify::event::ModifyKind::Any)).add_path(path);
        assert!(is_database_change(&event(database.to_path_buf()), database));
        assert!(is_database_change(
            &event("C:/data/castle.db-wal".into()),
            database
        ));
        assert!(!is_database_change(
            &event("C:/data/other.db-wal".into()),
            database
        ));
    }

    #[tokio::test]
    async fn database_watcher_wakes_for_external_wal_write() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let database = directory.path().join("castle.db");
        std::fs::write(&database, [])?;
        let wake = Arc::new(Notify::new());
        let _watcher = watch_database_changes(&database, wake.clone())?;

        std::fs::write(directory.path().join("castle.db-wal"), b"changed")?;
        tokio::time::timeout(Duration::from_secs(5), wake.notified()).await?;
        Ok(())
    }
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
