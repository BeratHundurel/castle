use super::*;
use settings::{StoredTab, TabSession};

impl AppShell {
    fn active_board_view(&self) -> Option<Entity<BoardView>> {
        self.tabs
            .open_tabs
            .get(self.tabs.active_tab_index)
            .and_then(|tab| match &tab.kind {
                OpenTabKind::Board { view, .. } => Some(view.clone()),
                _ => None,
            })
    }

    pub(super) fn move_active_related_note_candidate(
        &mut self,
        direction: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.active_board_view() else {
            return false;
        };
        if !view.read(cx).related_note_picker_open() {
            return false;
        }
        view.update(cx, |board, cx| {
            board.move_related_note_candidate(direction, cx);
        });
        true
    }

    pub(super) fn close_active_related_note_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.active_board_view() else {
            return false;
        };
        if !view.read(cx).related_note_picker_open() {
            return false;
        }
        view.update(cx, |board, cx| {
            board.close_related_note_picker(window, cx);
        });
        true
    }

    pub(crate) fn active_note_view(&self) -> Option<Entity<DocumentEditorView>> {
        self.tabs
            .open_tabs
            .get(self.tabs.active_tab_index)
            .and_then(|tab| match &tab.kind {
                OpenTabKind::Note { view, .. } => Some(view.clone()),
                _ => None,
            })
    }

    fn exit_all_zen_modes(&self, cx: &mut Context<Self>) {
        let views: Vec<Entity<DocumentEditorView>> =
            self.tabs.note_views.values().cloned().collect();
        for view in views {
            view.update(cx, |editor, cx| editor.exit_zen_mode(cx));
        }
    }

    fn exit_zen_modes_for_closed_notes(&self, cx: &mut Context<Self>) {
        let open_note_ids: std::collections::HashSet<u32> = self
            .tabs
            .open_tabs
            .iter()
            .filter_map(|tab| match &tab.kind {
                OpenTabKind::Note { note_id, .. } => Some(*note_id),
                _ => None,
            })
            .collect();
        let closed: Vec<Entity<DocumentEditorView>> = self
            .tabs
            .note_views
            .iter()
            .filter(|(note_id, _)| !open_note_ids.contains(note_id))
            .map(|(_, view)| view.clone())
            .collect();
        for view in closed {
            view.update(cx, |editor, cx| editor.exit_zen_mode(cx));
        }
    }

    pub(crate) fn open_workspace_target(
        &mut self,
        target: ::workspace::WorkspaceNavigationTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            ::workspace::WorkspaceNavigationTarget::Note {
                note_id,
                source_offset,
            } => {
                let Some(note) = self.workspace.notes.iter().find(|note| note.id == note_id) else {
                    window.push_notification(
                        Notification::warning("The linked note is no longer available."),
                        cx,
                    );
                    return;
                };
                self.open_note_tab(note_id, note.project_id, note.title.clone(), window, cx);
                if let Some(offset) = source_offset
                    && let Some(view) = self.tabs.note_views.get(&note_id)
                {
                    view.update(cx, |editor, cx| {
                        editor.navigate_to_offset(offset, window, cx)
                    });
                }
            }
            ::workspace::WorkspaceNavigationTarget::Board { board_id, .. } => {
                let Some(board) = self
                    .workspace
                    .boards
                    .iter()
                    .find(|board| board.id == board_id)
                else {
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
        let Some(pending) = self.workspace.pending_board_open.take() else {
            return;
        };
        if let Some(index) = self
            .tabs
            .open_tabs
            .iter()
            .position(|tab| tab.id == pending.tab_id)
        {
            self.tabs.open_tabs.remove(index);
            if self.tabs.active_tab_index > index {
                self.tabs.active_tab_index -= 1;
            }
            self.tabs
                .tab_scroll_handle
                .scroll_to_item(self.tabs.active_tab_index);
        }
    }

    pub(super) fn persist_tab_session(&mut self, cx: &mut Context<Self>) {
        let tabs = self
            .tabs
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
            active_tab_index: self.tabs.active_tab_index,
            active_project_id: self.workspace.active_project_id,
        };
        AppSettings::set_tab_session(session, cx);
    }

    pub(crate) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_pending_board_open();
        let index = self.tabs.open_tabs.len();
        let id = self.tabs.next_tab_id;
        self.tabs.next_tab_id = self.tabs.next_tab_id.saturating_add(1);
        self.tabs.open_tabs.push(OpenTab {
            id,
            title: "Home".into(),
            kind: OpenTabKind::Chooser,
        });
        self.activate_tab(index, window, cx);
    }

    pub(super) fn sync_sidebar_active(&self, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.open_tabs.get(self.tabs.active_tab_index) {
            match &tab.kind {
                OpenTabKind::Board {
                    board_id,
                    project_id,
                    ..
                } => {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.set_active_board(*board_id, *project_id);
                        cx.notify();
                    });
                }
                OpenTabKind::Note {
                    note_id,
                    project_id,
                    ..
                } => {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.set_active_note(*note_id, *project_id);
                        cx.notify();
                    });
                }
                OpenTabKind::Chooser => {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.clear_active_item();
                        cx.notify();
                    });
                }
                OpenTabKind::Trash => {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.clear_active_item();
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
        if index >= self.tabs.open_tabs.len() {
            return;
        }

        let target_tab_id = self.tabs.open_tabs[index].id;
        if self
            .workspace
            .pending_board_open
            .as_ref()
            .is_some_and(|pending| target_tab_id == pending.tab_id)
        {
            return;
        }
        if self.workspace.pending_board_open.is_some() {
            self.cancel_pending_board_open();
            let Some(updated_index) = self
                .tabs
                .open_tabs
                .iter()
                .position(|tab| tab.id == target_tab_id)
            else {
                return;
            };
            index = updated_index;
        }

        if self.tabs.active_tab_index != index {
            self.exit_all_zen_modes(cx);
        }
        self.tabs.active_tab_index = index;
        self.tabs.tab_scroll_handle.scroll_to_item(index);
        let tab = &self.tabs.open_tabs[index];

        match &tab.kind {
            OpenTabKind::Board {
                board_id: _,
                project_id,
                ..
            } => {
                self.workspace.active_project_id = *project_id;
            }
            OpenTabKind::Note {
                note_id: _,
                project_id,
                ..
            } => {
                self.workspace.active_project_id = *project_id;
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
        self.workspace.active_project_id = Some(project_id);

        if matches!(
            self.tabs
                .open_tabs
                .get(self.tabs.active_tab_index)
                .map(|tab| &tab.kind),
            Some(OpenTabKind::Chooser)
        ) {
            self.sync_sidebar_active(cx);
            self.persist_tab_session(cx);
            cx.notify();
            return;
        }

        if let Some(index) = self
            .tabs
            .open_tabs
            .iter()
            .position(|tab| matches!(tab.kind, OpenTabKind::Chooser))
        {
            self.activate_tab(index, window, cx);
            return;
        }

        let index = self.tabs.open_tabs.len();
        let id = self.tabs.next_tab_id;
        self.tabs.next_tab_id = self.tabs.next_tab_id.saturating_add(1);
        self.tabs.open_tabs.push(OpenTab {
            id,
            title: "Home".into(),
            kind: OpenTabKind::Chooser,
        });
        self.activate_tab(index, window, cx);
    }

    pub(super) fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.open_tabs.len() {
            return;
        }

        let closing_tab_id = self.tabs.open_tabs[index].id;
        if self
            .workspace
            .pending_board_open
            .as_ref()
            .is_some_and(|pending| pending.tab_id == closing_tab_id)
        {
            self.workspace.pending_board_open = None;
        }
        let was_active = self.tabs.active_tab_index == index;
        let closing_note_id = match &self.tabs.open_tabs[index].kind {
            OpenTabKind::Note { note_id, .. } => Some(*note_id),
            _ => None,
        };
        self.tabs.open_tabs.remove(index);
        if self.tabs.open_tabs.is_empty() {
            self.tabs.open_tabs.push(OpenTab {
                id: self.tabs.next_tab_id,
                title: "Home".into(),
                kind: OpenTabKind::Chooser,
            });
            self.tabs.next_tab_id = self.tabs.next_tab_id.saturating_add(1);
            self.tabs.active_tab_index = 0;
        } else if self.tabs.active_tab_index >= self.tabs.open_tabs.len() {
            self.tabs.active_tab_index = self.tabs.open_tabs.len().saturating_sub(1);
        } else if self.tabs.active_tab_index > index {
            self.tabs.active_tab_index -= 1;
        }
        self.tabs
            .tab_scroll_handle
            .scroll_to_item(self.tabs.active_tab_index);

        if was_active || self.tabs.active_tab_index >= self.tabs.open_tabs.len() {
            self.sync_sidebar_active(cx);
        }
        if let Some(note_id) = closing_note_id
            && let Some(view) = self.tabs.note_views.get(&note_id).cloned()
        {
            view.update(cx, |editor, cx| editor.exit_zen_mode(cx));
        }
        self.prune_closed_saved_note_views(cx);
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        self.persist_tab_session(cx);
        cx.notify();
    }

    pub(super) fn close_tab_by_id(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.open_tabs.iter().position(|tab| tab.id == id) {
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
            .tabs
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
        self.tabs.open_tabs.retain(|tab| tab.id == id);
        if self.tabs.open_tabs.is_empty() {
            self.tabs.open_tabs.push(OpenTab {
                id: self.tabs.next_tab_id,
                title: "Home".into(),
                kind: OpenTabKind::Chooser,
            });
            self.tabs.next_tab_id = self.tabs.next_tab_id.saturating_add(1);
        }
        self.tabs.active_tab_index = 0;
        self.tabs.tab_scroll_handle.scroll_to_item(0);
        self.exit_zen_modes_for_closed_notes(cx);
        self.prune_closed_saved_note_views(cx);
        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        self.persist_tab_session(cx);
        cx.notify();
    }

    pub(crate) fn close_all_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace.pending_board_open = None;
        self.tabs.open_tabs.clear();
        self.tabs.open_tabs.push(OpenTab {
            id: self.tabs.next_tab_id,
            title: "Home".into(),
            kind: OpenTabKind::Chooser,
        });
        self.tabs.next_tab_id = self.tabs.next_tab_id.saturating_add(1);
        self.tabs.active_tab_index = 0;
        self.tabs.tab_scroll_handle.scroll_to_item(0);
        self.exit_all_zen_modes(cx);
        self.prune_closed_saved_note_views(cx);
        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        self.persist_tab_session(cx);
        cx.notify();
    }

    pub(super) fn cycle_next_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.open_tabs.len() <= 1 {
            return;
        }
        let next = (self.tabs.active_tab_index + 1) % self.tabs.open_tabs.len();
        self.activate_tab(next, window, cx);
    }

    pub(super) fn cycle_prev_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.open_tabs.len() <= 1 {
            return;
        }
        let prev = if self.tabs.active_tab_index == 0 {
            self.tabs.open_tabs.len() - 1
        } else {
            self.tabs.active_tab_index - 1
        };
        self.activate_tab(prev, window, cx);
    }

    pub(super) fn sync_title_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .tabs
            .open_tabs
            .get(self.tabs.active_tab_index)
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

        let Some(tab) = self.tabs.open_tabs.get_mut(self.tabs.active_tab_index) else {
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
            .workspace
            .pending_title_saves
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
        let db = cx.global::<AppRuntime>().store();
        let runtime = cx.global::<AppRuntime>().tokio_handle();
        let save_lock = self.workspace.title_save_lock.clone();

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(250))
                .await;
            let is_current = this
                .read_with(cx, |this, _| {
                    this.workspace
                        .pending_title_saves
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
                    storage::workspace::persist_workspace_title(&db, target, title).await
                })
                .await;

            this.update(cx, |this, cx| {
                if this
                    .workspace
                    .pending_title_saves
                    .get(&target)
                    .is_none_or(|pending| pending.generation != generation)
                {
                    return;
                }
                this.workspace.pending_title_saves.remove(&target);

                match result {
                    Ok(Ok(update)) => {
                        if let WorkspaceTitleTarget::Note(note_id) = target
                            && let Some(view) =
                                this.tabs.open_tabs.iter().find_map(|tab| match &tab.kind {
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
        let pending = std::mem::take(&mut self.workspace.pending_title_saves)
            .into_iter()
            .map(|(target, pending)| (target, pending.title))
            .collect::<Vec<_>>();
        let db = cx.global::<AppRuntime>().store();
        let runtime = cx.global::<AppRuntime>().tokio_handle();
        let save_lock = self.workspace.title_save_lock.clone();

        async move {
            if pending.is_empty() {
                return;
            }

            let result = runtime
                .spawn(async move {
                    let _guard = save_lock.lock().await;
                    for (target, title) in pending {
                        storage::workspace::persist_workspace_title(&db, target, title).await?;
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
        self.record_item_opened(
            storage::workspace::home::WorkspaceItemKind::Board,
            board_id,
            cx,
        );
        if let Some(pending) = self
            .workspace
            .pending_board_open
            .as_ref()
            .filter(|pending| pending.board_id == board_id)
        {
            return pending.view.clone();
        }

        if let Some((index, view)) =
            self.tabs
                .open_tabs
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
            .tabs
            .open_tabs
            .get(self.tabs.active_tab_index)
            .filter(|tab| matches!(tab.kind, OpenTabKind::Chooser))
            .map(|tab| tab.id);
        let tab_id = self.tabs.next_tab_id;
        self.tabs.next_tab_id = self.tabs.next_tab_id.saturating_add(1);
        self.tabs.open_tabs.push(OpenTab {
            id: tab_id,
            title,
            kind: OpenTabKind::Board {
                board_id,
                project_id,
                view: view.clone(),
            },
        });
        self.workspace.pending_board_open = Some(PendingBoardOpen {
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
        let Some(pending) = self.workspace.pending_board_open.take() else {
            return;
        };
        if let Some(chooser_id) = pending.replaced_chooser_id
            && let Some(index) = self
                .tabs
                .open_tabs
                .iter()
                .position(|tab| tab.id == chooser_id)
        {
            self.tabs.open_tabs.remove(index);
            if self.tabs.active_tab_index > index {
                self.tabs.active_tab_index -= 1;
            }
        }
        if let Some(index) = self
            .tabs
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
        self.record_item_opened(
            storage::workspace::home::WorkspaceItemKind::Note,
            note_id,
            cx,
        );
        if let Some(index) = self.tabs.open_tabs.iter().position(
            |tab| matches!(&tab.kind, OpenTabKind::Note { note_id: id, .. } if *id == note_id),
        ) {
            self.activate_tab(index, window, cx);
            return;
        }

        let view = if let Some(view) = self.tabs.note_views.get(&note_id) {
            view.clone()
        } else {
            let view = DocumentEditorView::view(note_id, window, cx);
            Self::observe_document_editor(&view, window, cx);
            self.tabs.note_views.insert(note_id, view.clone());
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
        let open_tabs = &self.tabs.open_tabs;
        self.tabs.note_views.retain(|note_id, view| {
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
        if let Some(tab) = self.tabs.open_tabs.get_mut(self.tabs.active_tab_index)
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

        let index = self.tabs.open_tabs.len();
        let id = self.tabs.next_tab_id;
        self.tabs.next_tab_id = self.tabs.next_tab_id.saturating_add(1);
        self.tabs.open_tabs.push(OpenTab { id, title, kind });
        self.activate_tab(index, window, cx);
    }
}
