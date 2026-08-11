use super::*;
use crate::app_settings::{StoredTab, TabSession};

impl AppShell {
    pub(crate) fn active_note_view(&self) -> Option<Entity<DocumentEditorView>> {
        self.open_tabs
            .get(self.active_tab_index)
            .and_then(|tab| match &tab.kind {
                OpenTabKind::Note { view, .. } => Some(view.clone()),
                _ => None,
            })
    }

    pub(crate) fn open_workspace_target(
        &mut self,
        target: crate::workspace_navigation::WorkspaceNavigationTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            crate::workspace_navigation::WorkspaceNavigationTarget::Note {
                note_id,
                source_offset,
            } => {
                let Some(note) = self.notes.iter().find(|note| note.id == note_id) else {
                    window.push_notification(
                        Notification::warning("The linked note is no longer available."),
                        cx,
                    );
                    return;
                };
                self.open_note_tab(note_id, note.project_id, note.title.clone(), window, cx);
                if let Some(offset) = source_offset
                    && let Some(view) = self.note_views.get(&note_id)
                {
                    view.update(cx, |editor, cx| {
                        editor.navigate_to_offset(offset, window, cx)
                    });
                }
            }
            crate::workspace_navigation::WorkspaceNavigationTarget::Board { board_id, .. } => {
                let Some(board) = self.boards.iter().find(|board| board.id == board_id) else {
                    window.push_notification(
                        Notification::warning("The linked board is no longer available."),
                        cx,
                    );
                    return;
                };
                let view = self.open_board_tab(
                    board_id,
                    board.project_id,
                    board.title.clone(),
                    window,
                    cx,
                );
                view.update(cx, |board, cx| {
                    board.queue_reveal_target(target, cx);
                    board.apply_pending_reveal(window, cx);
                });
            }
        }
    }

    pub(super) fn cancel_pending_board_open(&mut self) {
        let Some(pending) = self.pending_board_open.take() else {
            return;
        };
        if let Some(index) = self
            .open_tabs
            .iter()
            .position(|tab| tab.id == pending.tab_id)
        {
            self.open_tabs.remove(index);
            if self.active_tab_index > index {
                self.active_tab_index -= 1;
            }
        }
    }

    pub(super) fn persist_tab_session(&mut self, cx: &mut Context<Self>) {
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
        AppSettings::set_tab_session(session, cx);
    }

    pub(crate) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_pending_board_open();
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
        mut index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.open_tabs.len() {
            return;
        }

        let target_tab_id = self.open_tabs[index].id;
        if self
            .pending_board_open
            .as_ref()
            .is_some_and(|pending| target_tab_id == pending.tab_id)
        {
            return;
        }
        if self.pending_board_open.is_some() {
            self.cancel_pending_board_open();
            let Some(updated_index) = self
                .open_tabs
                .iter()
                .position(|tab| tab.id == target_tab_id)
            else {
                return;
            };
            index = updated_index;
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
        self.cancel_pending_board_open();
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

        let closing_tab_id = self.open_tabs[index].id;
        if self
            .pending_board_open
            .as_ref()
            .is_some_and(|pending| pending.tab_id == closing_tab_id)
        {
            self.pending_board_open = None;
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
        self.prune_closed_saved_note_views(cx);
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
        self.prune_closed_saved_note_views(cx);
        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        self.persist_tab_session(cx);
        cx.notify();
    }

    pub(crate) fn close_all_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_board_open = None;
        self.open_tabs.clear();
        self.open_tabs.push(OpenTab {
            id: self.next_tab_id,
            title: "Home".into(),
            kind: OpenTabKind::Chooser,
        });
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.active_tab_index = 0;
        self.prune_closed_saved_note_views(cx);
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
        let db = cx.global::<AppServices>().store().connection();
        let runtime = cx.global::<AppServices>().runtime();
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
                    storage::workspace::persist_workspace_title(db.as_ref(), target, title).await
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
                    Ok(Ok(update)) => {
                        if let WorkspaceTitleTarget::Note(note_id) = target
                            && let Some(view) =
                                this.open_tabs.iter().find_map(|tab| match &tab.kind {
                                    OpenTabKind::Note {
                                        note_id: open_note_id,
                                        view,
                                        ..
                                    } if *open_note_id == note_id => Some(view.clone()),
                                    _ => None,
                                })
                        {
                            view.update(cx, |note, cx| {
                                note.apply_file_path(update.file_path, cx);
                            });
                        }
                        this.refresh_workspace(cx);
                    }
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
        let db = cx.global::<AppServices>().store().connection();
        let runtime = cx.global::<AppServices>().runtime();
        let save_lock = self.workspace_title_save_lock.clone();

        async move {
            if pending.is_empty() {
                return;
            }

            let result = runtime
                .spawn(async move {
                    let _guard = save_lock.lock().await;
                    for (target, title) in pending {
                        storage::workspace::persist_workspace_title(db.as_ref(), target, title)
                            .await?;
                    }
                    Ok::<(), anyhow::Error>(())
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
    ) -> Entity<BoardView> {
        self.record_item_opened(crate::home::WorkspaceItemKind::Board, board_id, cx);
        if let Some(pending) = self
            .pending_board_open
            .as_ref()
            .filter(|pending| pending.board_id == board_id)
        {
            return pending.view.clone();
        }

        if let Some((index, view)) =
            self.open_tabs
                .iter()
                .enumerate()
                .find_map(|(index, tab)| match &tab.kind {
                    OpenTabKind::Board {
                        board_id: id, view, ..
                    } if *id == board_id => Some((index, view.clone())),
                    _ => None,
                })
        {
            self.activate_tab(index, window, cx);
            return view;
        }

        self.cancel_pending_board_open();

        let view = BoardView::view(window, cx);
        Self::observe_board_view(&view, window, cx);
        let replaced_chooser_id = self
            .open_tabs
            .get(self.active_tab_index)
            .filter(|tab| matches!(tab.kind, OpenTabKind::Chooser))
            .map(|tab| tab.id);
        let tab_id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.open_tabs.push(OpenTab {
            id: tab_id,
            title,
            kind: OpenTabKind::Board {
                board_id,
                project_id,
                view: view.clone(),
            },
        });
        self.pending_board_open = Some(PendingBoardOpen {
            board_id,
            view: view.clone(),
            tab_id,
            replaced_chooser_id,
        });
        cx.notify();
        view.update(cx, |board, cx| board.reload_board(board_id, cx));
        view
    }

    pub(super) fn finish_pending_board_open(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.pending_board_open.take() else {
            return;
        };
        if let Some(chooser_id) = pending.replaced_chooser_id
            && let Some(index) = self.open_tabs.iter().position(|tab| tab.id == chooser_id)
        {
            self.open_tabs.remove(index);
            if self.active_tab_index > index {
                self.active_tab_index -= 1;
            }
        }
        if let Some(index) = self
            .open_tabs
            .iter()
            .position(|tab| tab.id == pending.tab_id)
        {
            self.activate_tab(index, window, cx);
        }
    }

    pub(crate) fn open_note_tab(
        &mut self,
        note_id: u32,
        project_id: Option<u32>,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_pending_board_open();
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
            Self::observe_document_editor(&view, window, cx);
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

    fn prune_closed_saved_note_views(&mut self, cx: &App) {
        let open_tabs = &self.open_tabs;
        self.note_views.retain(|note_id, view| {
            open_tabs.iter().any(|tab| {
                matches!(
                    &tab.kind,
                    OpenTabKind::Note {
                        note_id: open_note_id,
                        ..
                    } if open_note_id == note_id
                )
            }) || view.read(cx).save_state() != SaveState::Saved
        });
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
