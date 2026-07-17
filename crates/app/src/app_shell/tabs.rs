use super::*;
use crate::app_settings::{StoredTab, TabSession};

impl AppShell {
    pub(super) fn persist_tab_session(&mut self, cx: &mut Context<Self>) {
        self.tab_session_save_generation = self.tab_session_save_generation.saturating_add(1);
        let generation = self.tab_session_save_generation;
        let tabs = self
            .open_tabs
            .iter()
            .map(|tab| match &tab.kind {
                OpenTabKind::Chooser => StoredTab::Chooser,
                OpenTabKind::Trash => StoredTab::Trash,
                OpenTabKind::Board {
                    board_id,
                    project_id,
                    ..
                } => StoredTab::Board {
                    board_id: *board_id,
                    project_id: *project_id,
                    title: tab.title.to_string(),
                },
                OpenTabKind::Note {
                    note_id,
                    project_id,
                    ..
                } => StoredTab::Note {
                    note_id: *note_id,
                    project_id: *project_id,
                    title: tab.title.to_string(),
                },
            })
            .collect();
        let session = TabSession {
            tabs,
            active_tab_index: self.active_tab_index,
            active_project_id: self.active_project_id,
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(250))
                .await;
            this.update(cx, |this, cx| {
                if this.tab_session_save_generation == generation {
                    AppSettings::set_tab_session(session, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.open_tabs.len();
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.open_tabs.push(OpenTab {
            id,
            title: "Home".into(),
            kind: OpenTabKind::Chooser,
        });
        self.activate_tab(index, window, cx);
    }

    pub(super) fn sync_sidebar_active(&self, cx: &mut Context<Self>) {
        if let Some(tab) = self.open_tabs.get(self.active_tab_index) {
            match &tab.kind {
                OpenTabKind::Board {
                    board_id,
                    project_id,
                    ..
                } => {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.active_project_id = *project_id;
                        sidebar.active_item = Some(crate::sidebar::ActiveItem::Board(*board_id));
                        cx.notify();
                    });
                }
                OpenTabKind::Note {
                    note_id,
                    project_id,
                    ..
                } => {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.active_project_id = *project_id;
                        sidebar.active_item = Some(crate::sidebar::ActiveItem::Note(*note_id));
                        cx.notify();
                    });
                }
                OpenTabKind::Chooser => {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.active_item = None;
                        cx.notify();
                    });
                }
                OpenTabKind::Trash => {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.active_item = None;
                        cx.notify();
                    });
                }
            }
        }
    }

    pub(super) fn activate_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.open_tabs.len() {
            return;
        }

        self.active_tab_index = index;
        let tab = &self.open_tabs[index];

        match &tab.kind {
            OpenTabKind::Board {
                board_id: _,
                project_id,
                ..
            } => {
                self.active_project_id = *project_id;
            }
            OpenTabKind::Note {
                note_id: _,
                project_id,
                ..
            } => {
                self.active_project_id = *project_id;
            }
            OpenTabKind::Chooser | OpenTabKind::Trash => {}
        }

        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        self.persist_tab_session(cx);
        cx.notify();
    }

    pub(super) fn activate_project(
        &mut self,
        project_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_project_id = Some(project_id);

        if matches!(
            self.open_tabs
                .get(self.active_tab_index)
                .map(|tab| &tab.kind),
            Some(OpenTabKind::Chooser)
        ) {
            self.sync_sidebar_active(cx);
            self.persist_tab_session(cx);
            cx.notify();
            return;
        }

        if let Some(index) = self
            .open_tabs
            .iter()
            .position(|tab| matches!(tab.kind, OpenTabKind::Chooser))
        {
            self.activate_tab(index, window, cx);
            return;
        }

        let index = self.open_tabs.len();
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.open_tabs.push(OpenTab {
            id,
            title: "Home".into(),
            kind: OpenTabKind::Chooser,
        });
        self.activate_tab(index, window, cx);
    }

    pub(super) fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.open_tabs.len() {
            return;
        }

        let was_active = self.active_tab_index == index;
        self.open_tabs.remove(index);
        if self.open_tabs.is_empty() {
            self.open_tabs.push(OpenTab {
                id: self.next_tab_id,
                title: "Home".into(),
                kind: OpenTabKind::Chooser,
            });
            self.next_tab_id = self.next_tab_id.saturating_add(1);
            self.active_tab_index = 0;
        } else if self.active_tab_index >= self.open_tabs.len() {
            self.active_tab_index = self.open_tabs.len().saturating_sub(1);
        } else if self.active_tab_index > index {
            self.active_tab_index -= 1;
        }

        if was_active || self.active_tab_index >= self.open_tabs.len() {
            self.sync_sidebar_active(cx);
        }
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        self.persist_tab_session(cx);
        cx.notify();
    }

    pub(super) fn close_tab_by_id(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.open_tabs.iter().position(|tab| tab.id == id) {
            self.close_tab(index, window, cx);
        }
    }

    pub(super) fn close_project_tabs(
        &mut self,
        project_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_indexes = self
            .open_tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| match &tab.kind {
                OpenTabKind::Board {
                    project_id: Some(tab_project_id),
                    ..
                }
                | OpenTabKind::Note {
                    project_id: Some(tab_project_id),
                    ..
                } if *tab_project_id == project_id => Some(index),
                _ => None,
            })
            .collect::<Vec<_>>();

        for index in tab_indexes.into_iter().rev() {
            self.close_tab(index, window, cx);
        }
    }

    pub(super) fn close_other_tabs(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_tabs.retain(|tab| tab.id == id);
        if self.open_tabs.is_empty() {
            self.open_tabs.push(OpenTab {
                id: self.next_tab_id,
                title: "Home".into(),
                kind: OpenTabKind::Chooser,
            });
            self.next_tab_id = self.next_tab_id.saturating_add(1);
        }
        self.active_tab_index = 0;
        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        self.persist_tab_session(cx);
        cx.notify();
    }

    pub(crate) fn close_all_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_tabs.clear();
        self.open_tabs.push(OpenTab {
            id: self.next_tab_id,
            title: "Home".into(),
            kind: OpenTabKind::Chooser,
        });
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.active_tab_index = 0;
        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        self.persist_tab_session(cx);
        cx.notify();
    }

    pub(super) fn cycle_next_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open_tabs.len() <= 1 {
            return;
        }
        let next = (self.active_tab_index + 1) % self.open_tabs.len();
        self.activate_tab(next, window, cx);
    }

    pub(super) fn cycle_prev_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open_tabs.len() <= 1 {
            return;
        }
        let prev = if self.active_tab_index == 0 {
            self.open_tabs.len() - 1
        } else {
            self.active_tab_index - 1
        };
        self.activate_tab(prev, window, cx);
    }

    pub(super) fn sync_title_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .open_tabs
            .get(self.active_tab_index)
            .map(|tab| tab.title.to_string())
            .unwrap_or_else(|| "Home".to_string());

        self.suppress_title_event = true;
        self.title_input.update(cx, |input, cx| {
            input.set_value(title, window, cx);
        });
        self.suppress_title_event = false;
    }

    pub(super) fn rename_active_tab(&mut self, title: String, cx: &mut Context<Self>) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }

        let Some(tab) = self.open_tabs.get_mut(self.active_tab_index) else {
            return;
        };

        tab.title = SharedString::from(title);
        let target = match &tab.kind {
            OpenTabKind::Note { note_id, view, .. } => {
                view.update(cx, |note, cx| note.apply_title(title, cx));
                Some(WorkspaceTitleTarget::Note(*note_id))
            }
            OpenTabKind::Board { board_id, .. } => Some(WorkspaceTitleTarget::Board(*board_id)),
            OpenTabKind::Chooser | OpenTabKind::Trash => None,
        };

        if let Some(target) = target {
            self.schedule_workspace_title_save(target, title.to_string(), cx);
        }

        self.persist_tab_session(cx);
        cx.notify();
    }

    fn schedule_workspace_title_save(
        &mut self,
        target: WorkspaceTitleTarget,
        title: String,
        cx: &mut Context<Self>,
    ) {
        let generation = self
            .pending_workspace_title_saves
            .entry(target)
            .and_modify(|pending| {
                pending.generation = pending.generation.saturating_add(1);
                pending.title.clone_from(&title);
            })
            .or_insert_with(|| PendingWorkspaceTitleSave {
                generation: 1,
                title: title.clone(),
            })
            .generation;
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        let save_lock = self.workspace_title_save_lock.clone();

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(250))
                .await;
            let is_current = this
                .read_with(cx, |this, _| {
                    this.pending_workspace_title_saves
                        .get(&target)
                        .is_some_and(|pending| pending.generation == generation)
                })
                .unwrap_or(false);
            if !is_current {
                return;
            }

            let result = runtime
                .spawn(async move {
                    let _guard = save_lock.lock().await;
                    persist_workspace_title(db.as_ref(), target, title).await
                })
                .await;

            this.update(cx, |this, cx| {
                if this
                    .pending_workspace_title_saves
                    .get(&target)
                    .is_none_or(|pending| pending.generation != generation)
                {
                    return;
                }
                this.pending_workspace_title_saves.remove(&target);

                match result {
                    Ok(Ok(())) => this.refresh_workspace(cx),
                    Ok(Err(err)) => {
                        eprintln!("Failed to save workspace title: {err}");
                        this.refresh_workspace(cx);
                    }
                    Err(err) => {
                        eprintln!("Failed to join workspace title task: {err}");
                        this.refresh_workspace(cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn flush_pending_workspace_title_saves(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl Future<Output = ()> + use<> {
        let pending = std::mem::take(&mut self.pending_workspace_title_saves)
            .into_iter()
            .map(|(target, pending)| (target, pending.title))
            .collect::<Vec<_>>();
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        let save_lock = self.workspace_title_save_lock.clone();

        async move {
            if pending.is_empty() {
                return;
            }

            let result = runtime
                .spawn(async move {
                    let _guard = save_lock.lock().await;
                    for (target, title) in pending {
                        persist_workspace_title(db.as_ref(), target, title).await?;
                    }
                    Ok::<(), sea_orm::DbErr>(())
                })
                .await;

            match result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => eprintln!("Failed to flush workspace titles: {err}"),
                Err(err) => eprintln!("Failed to join workspace title flush task: {err}"),
            }
        }
    }

    pub(crate) fn open_board_tab(
        &mut self,
        board_id: u32,
        project_id: Option<u32>,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_item_opened(crate::home::WorkspaceItemKind::Board, board_id, cx);
        if let Some(index) = self.open_tabs.iter().position(
            |tab| matches!(&tab.kind, OpenTabKind::Board { board_id: id, .. } if *id == board_id),
        ) {
            self.activate_tab(index, window, cx);
            return;
        }

        let view = self
            .board_views
            .entry(board_id)
            .or_insert_with(|| BoardView::view(window, cx))
            .clone();
        view.update(cx, |board, cx| board.reload_board(board_id, cx));
        self.replace_or_push_active(
            OpenTabKind::Board {
                board_id,
                project_id,
                view,
            },
            title,
            window,
            cx,
        );
    }

    pub(crate) fn open_note_tab(
        &mut self,
        note_id: u32,
        project_id: Option<u32>,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.record_item_opened(crate::home::WorkspaceItemKind::Note, note_id, cx);
        if let Some(index) = self.open_tabs.iter().position(
            |tab| matches!(&tab.kind, OpenTabKind::Note { note_id: id, .. } if *id == note_id),
        ) {
            self.activate_tab(index, window, cx);
            return;
        }

        let view = if let Some(view) = self.note_views.get(&note_id) {
            view.clone()
        } else {
            let view = DocumentEditorView::view(note_id, window, cx);
            Self::observe_document_editor(&view, cx);
            self.note_views.insert(note_id, view.clone());
            view
        };
        self.replace_or_push_active(
            OpenTabKind::Note {
                note_id,
                project_id,
                view,
            },
            title,
            window,
            cx,
        );
    }

    pub(super) fn replace_or_push_active(
        &mut self,
        kind: OpenTabKind,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.open_tabs.get_mut(self.active_tab_index)
            && matches!(tab.kind, OpenTabKind::Chooser)
        {
            tab.kind = kind;
            tab.title = title;
            self.sync_sidebar_active(cx);
            self.sync_title_input(window, cx);
            self.persist_tab_session(cx);
            cx.notify();
            return;
        }

        let index = self.open_tabs.len();
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.open_tabs.push(OpenTab { id, title, kind });
        self.activate_tab(index, window, cx);
    }
}

async fn persist_workspace_title(
    db: &sea_orm::DatabaseConnection,
    target: WorkspaceTitleTarget,
    title: String,
) -> Result<(), sea_orm::DbErr> {
    match target {
        WorkspaceTitleTarget::Board(board_id) => {
            board::ActiveModel {
                id: Set(board_id as i64),
                title: Set(title),
                ..Default::default()
            }
            .update(db)
            .await?;
        }
        WorkspaceTitleTarget::Note(note_id) => {
            note::ActiveModel {
                id: Set(note_id as i64),
                title: Set(title),
                updated_at: Set(now_ts()),
                ..Default::default()
            }
            .update(db)
            .await?;
        }
    }

    Ok(())
}
