use entity::entry;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DatabaseConnection, DbBackend, Statement,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueReminder {
    pub entry_id: i64,
    pub title: String,
    pub due_on: String,
    pub board_title: String,
    pub list_title: String,
}

pub async fn load_due_reminders(
    db: &DatabaseConnection,
    due_through: &str,
) -> anyhow::Result<Vec<DueReminder>> {
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
            [due_through.into()],
        ))
        .await?;

    rows.into_iter()
        .map(|row| {
            Ok(DueReminder {
                entry_id: row.try_get("", "id")?,
                title: row.try_get("", "title")?,
                due_on: row.try_get("", "due_on")?,
                board_title: row.try_get("", "board_title")?,
                list_title: row.try_get("", "list_title")?,
            })
        })
        .collect()
}

pub async fn mark_reminder_notified(
    db: &DatabaseConnection,
    entry_id: i64,
    due_on: String,
) -> anyhow::Result<()> {
    entry::ActiveModel {
        id: Set(entry_id),
        reminder_notified_for: Set(Some(due_on)),
        ..Default::default()
    }
    .update(db)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use entity::{board, card, entry, project};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};

    #[tokio::test]
    async fn due_reminders_are_loaded_once_after_notification() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let project = project::ActiveModel {
            name: Set("Active project".to_string()),
            archived: Set(false),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let board = board::ActiveModel {
            title: Set("Delivery".to_string()),
            project_id: Set(Some(project.id)),
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
        let due_entry = entry::ActiveModel {
            title: Set("Ship refactor".to_string()),
            description: Set(String::new()),
            card_id: Set(list.id),
            position: Set(0),
            due_on: Set(Some("2026-07-29".to_string())),
            reminder_enabled: Set(true),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        entry::ActiveModel {
            title: Set("Later".to_string()),
            description: Set(String::new()),
            card_id: Set(list.id),
            position: Set(1),
            due_on: Set(Some("2026-07-31".to_string())),
            reminder_enabled: Set(true),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let reminders = load_due_reminders(&db, "2026-07-29").await?;
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0].entry_id, due_entry.id);
        assert_eq!(reminders[0].board_title, "Delivery");
        assert_eq!(reminders[0].list_title, "Today");

        mark_reminder_notified(&db, due_entry.id, reminders[0].due_on.clone()).await?;
        assert!(load_due_reminders(&db, "2026-07-29").await?.is_empty());
        Ok(())
    }
}
