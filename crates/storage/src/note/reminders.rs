use sea_orm::{DbBackend, Statement};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DueReminder {
    pub entry_id: i64,
    pub title: String,
    pub due_on: String,
    pub board_title: String,
    pub list_title: String,
}

pub async fn load_due_reminders(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
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
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    entry_id: i64,
    due_on: String,
) -> anyhow::Result<()> {
    mark_many_reminders_notified(db, &[(entry_id, due_on)]).await
}

pub async fn mark_many_reminders_notified(
    db: &(
         impl sea_orm::ConnectionTrait
         + sea_orm::TransactionTrait<Transaction = sea_orm::DatabaseTransaction>
     ),
    reminders: &[(i64, String)],
) -> anyhow::Result<()> {
    if reminders.is_empty() {
        return Ok(());
    }
    let mut sql = String::from("UPDATE entry SET reminder_notified_for = CASE id");
    let mut values = Vec::with_capacity(reminders.len() * 3);
    for (entry_id, due_on) in reminders {
        sql.push_str(" WHEN ? THEN ?");
        values.push(sea_orm::Value::from(*entry_id));
        values.push(sea_orm::Value::from(due_on.as_str()));
    }
    sql.push_str(" END WHERE id IN (");
    for (index, (entry_id, _)) in reminders.iter().enumerate() {
        if index > 0 {
            sql.push_str(", ");
        }
        sql.push('?');
        values.push(sea_orm::Value::from(*entry_id));
    }
    sql.push(')');
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        sql,
        values,
    ))
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

    #[tokio::test]
    async fn marking_many_reminders_one_by_one_clears_all_due() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = board::ActiveModel {
            title: Set("Delivery".to_string()),
            project_id: Set(None),
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
        for position in 0..10 {
            entry::ActiveModel {
                title: Set(format!("Due {position}")),
                description: Set(String::new()),
                card_id: Set(list.id),
                position: Set(position),
                due_on: Set(Some("2026-07-29".to_string())),
                reminder_enabled: Set(true),
                ..Default::default()
            }
            .insert(&db)
            .await?;
        }
        let reminders = load_due_reminders(&db, "2026-07-29").await?;
        assert_eq!(reminders.len(), 10);
        let started = std::time::Instant::now();
        for reminder in &reminders {
            mark_reminder_notified(&db, reminder.entry_id, reminder.due_on.clone()).await?;
        }
        let elapsed = started.elapsed();
        eprintln!(
            "BASELINE mark_reminder_notified count={} elapsed_ms={}",
            reminders.len(),
            elapsed.as_millis()
        );
        assert!(load_due_reminders(&db, "2026-07-29").await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn batch_marking_clears_all_due_and_rejects_mismatched_due_on() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let board = board::ActiveModel {
            title: Set("Delivery".to_string()),
            project_id: Set(None),
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
        for position in 0..10 {
            entry::ActiveModel {
                title: Set(format!("Due {position}")),
                description: Set(String::new()),
                card_id: Set(list.id),
                position: Set(position),
                due_on: Set(Some("2026-07-29".to_string())),
                reminder_enabled: Set(true),
                ..Default::default()
            }
            .insert(&db)
            .await?;
        }
        let reminders = load_due_reminders(&db, "2026-07-29").await?;
        assert_eq!(reminders.len(), 10);

        let mismatched: Vec<(i64, String)> = reminders
            .iter()
            .map(|reminder| (reminder.entry_id, "2026-07-30".to_string()))
            .collect();
        mark_many_reminders_notified(&db, &mismatched).await?;
        assert_eq!(load_due_reminders(&db, "2026-07-29").await?.len(), 10);

        let matched: Vec<(i64, String)> = reminders
            .iter()
            .map(|reminder| (reminder.entry_id, reminder.due_on.clone()))
            .collect();
        let started = std::time::Instant::now();
        mark_many_reminders_notified(&db, &matched).await?;
        let elapsed = started.elapsed();
        eprintln!(
            "BATCH mark_many_reminders_notified count={} elapsed_ms={}",
            matched.len(),
            elapsed.as_millis()
        );
        assert!(load_due_reminders(&db, "2026-07-29").await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn batch_marking_uses_single_update_for_many_due() -> Result<()> {
        use sea_orm::{MockDatabase, MockExecResult, Value};
        use std::collections::BTreeMap;

        let idle = MockDatabase::new(DbBackend::Sqlite).into_connection();
        mark_many_reminders_notified(&idle, &[]).await?;
        assert!(idle.into_transaction_log().is_empty());

        let rows: Vec<BTreeMap<String, Value>> = (1..=10_i64)
            .map(|id| {
                BTreeMap::from([
                    ("id".to_string(), Value::from(id)),
                    ("title".to_string(), Value::from(format!("Due {id}"))),
                    ("due_on".to_string(), Value::from("2026-07-29")),
                    ("board_title".to_string(), Value::from("Delivery")),
                    ("list_title".to_string(), Value::from("Today")),
                ])
            })
            .collect();
        let db = MockDatabase::new(DbBackend::Sqlite)
            .append_query_results([rows])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 10,
            }])
            .into_connection();

        let reminders = load_due_reminders(&db, "2026-07-29").await?;
        assert_eq!(reminders.len(), 10);
        let matched: Vec<(i64, String)> = reminders
            .iter()
            .map(|reminder| (reminder.entry_id, reminder.due_on.clone()))
            .collect();
        mark_many_reminders_notified(&db, &matched).await?;

        let log = db.into_transaction_log();
        assert_eq!(
            log.len(),
            2,
            "expected 1 select + 1 batch update, got {} statements",
            log.len()
        );
        assert_eq!(log[1].statements().len(), 1);
        assert!(
            format!("{:?}", log[1].statements()[0]).contains("UPDATE entry"),
            "expected a single UPDATE entry statement, got {:?}",
            log[1].statements()[0]
        );
        Ok(())
    }

    #[tokio::test]
    async fn one_by_one_marking_costs_n_plus_one_statements() -> Result<()> {
        use sea_orm::{MockDatabase, MockExecResult, Value};
        use std::collections::BTreeMap;

        let rows: Vec<BTreeMap<String, Value>> = (1..=10_i64)
            .map(|id| {
                BTreeMap::from([
                    ("id".to_string(), Value::from(id)),
                    ("title".to_string(), Value::from(format!("Due {id}"))),
                    ("due_on".to_string(), Value::from("2026-07-29")),
                    ("board_title".to_string(), Value::from("Delivery")),
                    ("list_title".to_string(), Value::from("Today")),
                ])
            })
            .collect();
        let db = MockDatabase::new(DbBackend::Sqlite)
            .append_query_results([rows])
            .append_exec_results((0..10).map(|_| MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }))
            .into_connection();

        let reminders = load_due_reminders(&db, "2026-07-29").await?;
        assert_eq!(reminders.len(), 10);
        for reminder in &reminders {
            mark_reminder_notified(&db, reminder.entry_id, reminder.due_on.clone()).await?;
        }

        let log = db.into_transaction_log();
        assert_eq!(
            log.len(),
            11,
            "expected 1 select + 10 single-row updates, got {} statements",
            log.len()
        );
        Ok(())
    }
}
