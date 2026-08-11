use std::{collections::HashMap, sync::Arc};

use entity::{card, entry};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection, DbErr, TransactionTrait};
use tokio::sync::Notify;

pub type ListPositions = Vec<(u32, i32)>;
pub type EntryPositions = Vec<(u32, u32, i32)>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BoardLayoutSnapshot {
    pub lists: ListPositions,
    pub entries: EntryPositions,
}

#[derive(Clone)]
pub struct BoardLayoutPersistence {
    boards: Arc<std::sync::Mutex<HashMap<u32, BoardPersistenceState>>>,
    runtime: tokio::runtime::Handle,
}

impl Default for BoardLayoutPersistence {
    fn default() -> Self {
        Self::new(tokio::runtime::Handle::current())
    }
}

struct BoardPersistenceState {
    next_revision: u64,
    completed_revision: u64,
    successful_revision: u64,
    last_error: Option<(u64, String)>,
    pending: Option<PositionSnapshot>,
    worker_running: bool,
    changed: Arc<Notify>,
}

impl Default for BoardPersistenceState {
    fn default() -> Self {
        Self {
            next_revision: 0,
            completed_revision: 0,
            successful_revision: 0,
            last_error: None,
            pending: None,
            worker_running: false,
            changed: Arc::new(Notify::new()),
        }
    }
}

struct PositionSnapshot {
    revision: u64,
    layout: BoardLayoutSnapshot,
}

impl BoardLayoutPersistence {
    pub fn new(runtime: tokio::runtime::Handle) -> Self {
        Self {
            boards: Default::default(),
            runtime,
        }
    }

    pub fn submit(
        &self,
        board_id: u32,
        db: Arc<DatabaseConnection>,
        layout: BoardLayoutSnapshot,
    ) -> anyhow::Result<u64> {
        let (revision, start_worker) = {
            let mut boards = self.boards.lock().map_err(|_| {
                anyhow::anyhow!("Failed to lock the board-layout persistence coordinator")
            })?;
            let state = boards.entry(board_id).or_default();
            state.next_revision = state.next_revision.saturating_add(1);
            let revision = state.next_revision;
            state.pending = Some(PositionSnapshot { revision, layout });
            let start_worker = if state.worker_running {
                false
            } else {
                state.worker_running = true;
                true
            };
            (revision, start_worker)
        };

        if start_worker {
            self.runtime
                .spawn(run_worker(board_id, db, self.boards.clone()));
        }
        Ok(revision)
    }

    pub async fn wait_for_pending(&self, board_id: u32) -> anyhow::Result<()> {
        let target_revision = {
            let boards = self.boards.lock().map_err(|_| {
                anyhow::anyhow!("Failed to lock the board-layout persistence coordinator")
            })?;
            let Some(state) = boards.get(&board_id) else {
                return Ok(());
            };
            state.next_revision
        };
        self.wait_for_revision(board_id, target_revision).await
    }

    pub async fn wait_for_revision(
        &self,
        board_id: u32,
        target_revision: u64,
    ) -> anyhow::Result<()> {
        let changed = {
            let boards = self.boards.lock().map_err(|_| {
                anyhow::anyhow!("Failed to lock the board-layout persistence coordinator")
            })?;
            let Some(state) = boards.get(&board_id) else {
                return Ok(());
            };
            state.changed.clone()
        };

        loop {
            let notified = changed.notified();
            let outcome = {
                let boards = self.boards.lock().map_err(|_| {
                    anyhow::anyhow!("Failed to lock the board-layout persistence coordinator")
                })?;
                let Some(state) = boards.get(&board_id) else {
                    return Ok(());
                };
                if state.completed_revision < target_revision {
                    None
                } else if state.successful_revision >= target_revision {
                    Some(Ok(()))
                } else {
                    let message = state
                        .last_error
                        .as_ref()
                        .filter(|(revision, _)| *revision >= target_revision)
                        .map(|(_, message)| message.clone())
                        .unwrap_or_else(|| "Card positions were not persisted".to_string());
                    Some(Err(anyhow::anyhow!(message)))
                }
            };
            if let Some(outcome) = outcome {
                return outcome;
            }
            notified.await;
        }
    }
}

async fn run_worker(
    board_id: u32,
    db: Arc<DatabaseConnection>,
    boards: Arc<std::sync::Mutex<HashMap<u32, BoardPersistenceState>>>,
) {
    loop {
        let snapshot = {
            let Ok(mut boards) = boards.lock() else {
                eprintln!("Failed to lock the board-layout persistence coordinator");
                return;
            };
            let Some(state) = boards.get_mut(&board_id) else {
                return;
            };
            let Some(snapshot) = state.pending.take() else {
                state.worker_running = false;
                state.changed.notify_waiters();
                return;
            };
            snapshot
        };

        let revision = snapshot.revision;
        let result = persist_board_layout_in_db(db.as_ref(), snapshot.layout).await;
        let Ok(mut boards) = boards.lock() else {
            eprintln!("Failed to lock the board-layout persistence coordinator");
            return;
        };
        let Some(state) = boards.get_mut(&board_id) else {
            return;
        };
        state.completed_revision = state.completed_revision.max(revision);
        match result {
            Ok(()) => {
                state.successful_revision = state.successful_revision.max(revision);
                if state
                    .last_error
                    .as_ref()
                    .is_some_and(|(failed_revision, _)| *failed_revision <= revision)
                {
                    state.last_error = None;
                }
            }
            Err(error) => {
                let message = format!("Failed to save board layout: {error}");
                eprintln!("{message}");
                state.last_error = Some((revision, message));
            }
        }
        let should_stop = state.pending.is_none();
        if should_stop {
            state.worker_running = false;
        }
        state.changed.notify_waiters();
        drop(boards);
        if should_stop {
            return;
        }
    }
}

async fn persist_board_layout_in_db(
    db: &DatabaseConnection,
    layout: BoardLayoutSnapshot,
) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    for (list_id, position) in layout.lists {
        card::ActiveModel {
            id: Set(list_id as i64),
            position: Set(position),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }
    for (entry_id, card_id, position) in layout.entries {
        entry::ActiveModel {
            id: Set(entry_id as i64),
            card_id: Set(card_id as i64),
            position: Set(position),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }
    txn.commit().await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::{Context as _, Result};
    use entity::{board, card, entry};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{
        ActiveModelTrait, ActiveValue::Set, ConnectionTrait, Database, EntityTrait, QueryOrder,
    };

    use super::{BoardLayoutPersistence, BoardLayoutSnapshot};

    #[tokio::test]
    async fn rapid_updates_persist_only_the_latest_snapshot() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let board = board::ActiveModel {
            title: Set("Kanban".to_string()),
            project_id: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let todo = card::ActiveModel {
            title: Set("Todo".to_string()),
            board_id: Set(board.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let done = card::ActiveModel {
            title: Set("Done".to_string()),
            board_id: Set(board.id),
            position: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let first = entry::ActiveModel {
            title: Set("First".to_string()),
            description: Set(String::new()),
            card_id: Set(todo.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let second = entry::ActiveModel {
            title: Set("Second".to_string()),
            description: Set(String::new()),
            card_id: Set(todo.id),
            position: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        db.execute_unprepared(
            r#"
            CREATE TABLE position_write_audit (entry_id INTEGER NOT NULL);
            CREATE TRIGGER audit_entry_position_update
            AFTER UPDATE OF card_id, position ON entry BEGIN
                INSERT INTO position_write_audit (entry_id) VALUES (NEW.id);
            END;
            "#,
        )
        .await?;

        let db = Arc::new(db);
        let persistence = BoardLayoutPersistence::default();
        persistence.submit(
            board.id as u32,
            db.clone(),
            BoardLayoutSnapshot {
                lists: vec![(todo.id as u32, 0), (done.id as u32, 1)],
                entries: vec![
                    (first.id as u32, todo.id as u32, 0),
                    (second.id as u32, todo.id as u32, 1),
                ],
            },
        )?;
        for index in 0..100 {
            let target = if index % 2 == 0 { done.id } else { todo.id };
            persistence.submit(
                board.id as u32,
                db.clone(),
                BoardLayoutSnapshot {
                    lists: vec![(todo.id as u32, 0), (done.id as u32, 1)],
                    entries: vec![
                        (first.id as u32, target as u32, 0),
                        (second.id as u32, target as u32, 1),
                    ],
                },
            )?;
        }
        persistence.submit(
            board.id as u32,
            db.clone(),
            BoardLayoutSnapshot {
                lists: vec![(done.id as u32, 0), (todo.id as u32, 1)],
                entries: vec![
                    (first.id as u32, done.id as u32, 0),
                    (second.id as u32, done.id as u32, 1),
                ],
            },
        )?;

        persistence.wait_for_pending(board.id as u32).await?;
        let stored = entry::Entity::find()
            .order_by_asc(entry::Column::Position)
            .all(db.as_ref())
            .await?;
        assert_eq!(
            stored
                .iter()
                .map(|entry| (entry.id, entry.card_id, entry.position))
                .collect::<Vec<_>>(),
            vec![(first.id, done.id, 0), (second.id, done.id, 1)]
        );
        let stored_lists = card::Entity::find()
            .order_by_asc(card::Column::Position)
            .all(db.as_ref())
            .await?;
        assert_eq!(
            stored_lists
                .iter()
                .map(|list| (list.id, list.position))
                .collect::<Vec<_>>(),
            vec![(done.id, 0), (todo.id, 1)]
        );
        let writes = db
            .query_one_raw(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM position_write_audit",
            ))
            .await?
            .context("position write count")?;
        let write_count = writes.try_get::<i64>("", "count")?;
        assert!(write_count >= 2);
        assert!(write_count < 202, "rapid submissions should be coalesced");
        Ok(())
    }

    #[tokio::test]
    async fn failed_write_is_reported_and_the_next_submission_can_run() -> Result<()> {
        let db = Arc::new(Database::connect("sqlite::memory:").await?);
        Migrator::up(db.as_ref(), None).await?;
        let persistence = BoardLayoutPersistence::default();
        let board_id = 42;

        let board = board::ActiveModel {
            id: Set(board_id as i64),
            title: Set("Recovery".to_string()),
            project_id: Set(None),
            ..Default::default()
        }
        .insert(db.as_ref())
        .await?;
        let list = card::ActiveModel {
            title: Set("Todo".to_string()),
            board_id: Set(board.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(db.as_ref())
        .await?;
        let entry = entry::ActiveModel {
            title: Set("Recovered".to_string()),
            description: Set(String::new()),
            card_id: Set(list.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(db.as_ref())
        .await?;

        persistence.submit(
            board_id,
            db.clone(),
            BoardLayoutSnapshot {
                lists: vec![(list.id as u32, 7)],
                entries: vec![(999, 999, 0)],
            },
        )?;
        let error = persistence
            .wait_for_pending(board_id)
            .await
            .expect_err("an update for a missing card must fail");
        assert!(error.to_string().contains("Failed to save board layout"));
        let stored_list = card::Entity::find_by_id(list.id)
            .one(db.as_ref())
            .await?
            .context("list should still exist")?;
        assert_eq!(
            stored_list.position, 0,
            "the failed snapshot must roll back"
        );

        persistence.submit(
            board_id,
            db,
            BoardLayoutSnapshot {
                lists: vec![(list.id as u32, 0)],
                entries: vec![(entry.id as u32, list.id as u32, 1)],
            },
        )?;
        persistence.wait_for_pending(board_id).await?;
        Ok(())
    }
}
