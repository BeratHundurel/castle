use super::*;
use runtime::AppRuntime;
use settings::{StoredTab, TabSession};

impl AppShell {
    fn active_board_view(&self, cx: &App) -> Option<Entity<BoardView>> {
        self.tabs
            .open_tabs
            .get(self.tabs.active_tab_index)
            .and_then(|tab| match &tab.kind {
                OpenTabKind::Board {
                    view, navigation, ..
                } if navigation.read(cx).is_board(cx) => Some(view.clone()),
                _ => None,
            })
    }

    pub(super) fn move_active_related_note_candidate(
        &mut self,
        direction: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.active_board_view(cx) else {
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
        let Some(view) = self.active_board_view(cx) else {
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
                source_range,
            } => {
                let Some(note) = self.workspace.notes.iter().find(|note| note.id == note_id) else {
                    window.push_notification(
                        Notification::warning("The linked note is no longer available."),
                        cx,
                    );
                    return;
                };
                self.open_note_tab(note_id, note.project_id, note.title.clone(), window, cx);
                if let Some(range) = source_range
                    && let Some(view) = self.tabs.note_views.get(&note_id)
                {
                    view.update(cx, |editor, cx| {
                        editor.navigate_to_range(range.0..range.1, window, cx)
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
                if let Some(navigation) =
                    self.tabs.open_tabs.iter().find_map(|tab| match &tab.kind {
                        OpenTabKind::Board {
                            board_id: id,
                            navigation,
                            ..
                        } if *id == board_id => Some(navigation.clone()),
                        _ => None,
                    })
                {
                    navigation.update(cx, |navigation, cx| navigation.return_to_board(window, cx));
                }
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
            .filter_map(|tab| match &tab.kind {
                OpenTabKind::Restored(stored_tab) => Some(stored_tab.clone()),
                OpenTabKind::Chooser => Some(StoredTab::Chooser),
                OpenTabKind::Trash => Some(StoredTab::Trash),
                OpenTabKind::Cheatsheet { .. } => Some(StoredTab::Cheatsheet),
                OpenTabKind::Board {
                    board_id,
                    project_id,
                    ..
                } => Some(StoredTab::Board {
                    board_id: *board_id,
                    project_id: *project_id,
                    title: tab.title.to_string(),
                }),
                OpenTabKind::Note {
                    note_id,
                    project_id,
                    ..
                } => Some(StoredTab::Note {
                    note_id: *note_id,
                    project_id: *project_id,
                    title: tab.title.to_string(),
                }),
                OpenTabKind::Settings { .. } => None,
            })
            .collect();
        let active_tab_index = self
            .tabs
            .open_tabs
            .get(self.tabs.active_tab_index)
            .map(|_| {
                self.tabs
                    .open_tabs
                    .iter()
                    .take(self.tabs.active_tab_index.saturating_add(1))
                    .filter(|tab| !matches!(tab.kind, OpenTabKind::Settings { .. }))
                    .count()
                    .saturating_sub(1)
            })
            .unwrap_or_default();
        let session = TabSession {
            tabs,
            active_tab_index,
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
                OpenTabKind::Restored(_)
                | OpenTabKind::Trash
                | OpenTabKind::Settings { .. }
                | OpenTabKind::Cheatsheet { .. } => {
                    self.sidebar.update(cx, |sidebar, cx| {
                        sidebar.clear_active_item();
                        cx.notify();
                    });
                }
            }
        }
    }

    pub(super) fn materialize_restored_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(OpenTabKind::Restored(stored_tab)) =
            self.tabs.open_tabs.get(index).map(|tab| &tab.kind)
        else {
            return;
        };
        let kind = match stored_tab.clone() {
            StoredTab::Chooser => OpenTabKind::Chooser,
            StoredTab::Trash => OpenTabKind::Trash,
            StoredTab::Cheatsheet => OpenTabKind::Cheatsheet {
                view: CheatsheetView::view((self.shortcuts)(cx), window, cx),
            },
            StoredTab::Board {
                board_id,
                project_id,
                ..
            } => {
                let view = BoardView::view(window, cx);
                Self::observe_board_view(&view, window, cx);
                view.update(cx, |board, cx| board.load_board(board_id, cx));
                let navigation =
                    cx.new(|cx| BoardNavigation::new(board_id, view.clone(), window, cx));
                OpenTabKind::Board {
                    board_id,
                    project_id,
                    view,
                    navigation,
                }
            }
            StoredTab::Note {
                note_id,
                project_id,
                ..
            } => {
                let view = if let Some(view) = self.tabs.note_views.get(&note_id) {
                    view.clone()
                } else {
                    let view = DocumentEditorView::view(note_id, window, cx);
                    Self::observe_document_editor(&view, window, cx);
                    self.tabs.note_views.insert(note_id, view.clone());
                    view
                };
                OpenTabKind::Note {
                    note_id,
                    project_id,
                    view,
                }
            }
        };
        self.tabs.open_tabs[index].kind = kind;
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

        let tab_changed = self.tabs.active_tab_index != index;
        if tab_changed {
            self.exit_all_zen_modes(cx);
        }
        self.materialize_restored_tab(index, window, cx);
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
            OpenTabKind::Restored(_)
            | OpenTabKind::Chooser
            | OpenTabKind::Trash
            | OpenTabKind::Settings { .. }
            | OpenTabKind::Cheatsheet { .. } => {}
        }

        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
        self.focus_active_tab(window, cx);
        self.persist_tab_session(cx);
        if tab_changed {
            self.load_home_if_active(cx);
        }
        cx.notify();
    }

    pub(super) fn focus_active_tab(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self
            .tabs
            .open_tabs
            .get(self.tabs.active_tab_index)
            .map(|tab| &tab.kind)
        {
            Some(OpenTabKind::Board { navigation, .. }) => {
                navigation.update(cx, |navigation, cx| navigation.focus_current(window, cx));
            }
            Some(OpenTabKind::Cheatsheet { view }) => view.focus_handle(cx).focus(window, cx),
            _ => self.focus_handle.focus(window, cx),
        }
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

        if let Some(index) = self.tabs.open_tabs.iter().position(|tab| {
            matches!(
                tab.kind,
                OpenTabKind::Chooser | OpenTabKind::Restored(StoredTab::Chooser)
            )
        }) {
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
        self.materialize_restored_tab(self.tabs.active_tab_index, window, cx);
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
        if was_active {
            self.load_home_if_active(cx);
        }
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
                OpenTabKind::Restored(
                    StoredTab::Board {
                        project_id: Some(tab_project_id),
                        ..
                    }
                    | StoredTab::Note {
                        project_id: Some(tab_project_id),
                        ..
                    },
                ) if *tab_project_id == project_id => Some(index),
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
        self.materialize_restored_tab(0, window, cx);
        self.tabs.tab_scroll_handle.scroll_to_item(0);
        self.exit_zen_modes_for_closed_notes(cx);
        self.prune_closed_saved_note_views(cx);
        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        self.persist_tab_session(cx);
        self.load_home_if_active(cx);
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
            OpenTabKind::Restored(_)
            | OpenTabKind::Chooser
            | OpenTabKind::Trash
            | OpenTabKind::Settings { .. }
            | OpenTabKind::Cheatsheet { .. } => None,
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
        let app_runtime = cx.global::<AppRuntime>().clone();
        let db = app_runtime.store();
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

            let result = app_runtime
                .spawn_tokio(cx.background_executor(), async move {
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
        let app_runtime = cx.global::<AppRuntime>().clone();
        let db = app_runtime.store();
        let save_lock = self.workspace.title_save_lock.clone();
        let background_executor = cx.background_executor().clone();

        async move {
            if pending.is_empty() {
                return;
            }

            let result = app_runtime
                .spawn_tokio(&background_executor, async move {
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

        if let Some(index) = self.tabs.open_tabs.iter().position(|tab| match &tab.kind {
            OpenTabKind::Board { board_id: id, .. }
            | OpenTabKind::Restored(StoredTab::Board { board_id: id, .. }) => *id == board_id,
            _ => false,
        }) {
            self.activate_tab(index, window, cx);
            if let OpenTabKind::Board { view, .. } = &self.tabs.open_tabs[index].kind {
                return view.clone();
            }
        }

        self.cancel_pending_board_open();

        let view = BoardView::view(window, cx);
        Self::observe_board_view(&view, window, cx);
        let navigation = cx.new(|cx| BoardNavigation::new(board_id, view.clone(), window, cx));
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
                navigation,
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
        if let Some(index) = self.tabs.open_tabs.iter().position(|tab| match &tab.kind {
            OpenTabKind::Note { note_id: id, .. }
            | OpenTabKind::Restored(StoredTab::Note { note_id: id, .. }) => *id == note_id,
            _ => false,
        }) {
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

#[cfg(test)]
mod restore_tests {
    use std::{collections::HashSet, path::PathBuf};

    use chrono::{Duration, Local};
    use entity::{board, card, entry, note};
    use gpui_kit::TestAppContext;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectOptions, Database};
    use settings::{AppSettings, StoredTab, TabSession};

    use super::*;

    #[gpui_kit::test]
    fn activating_home_loads_workspace_data_and_refreshes_recent_after_open(
        cx: &mut TestAppContext,
    ) {
        let tokio = tokio::runtime::Runtime::new().expect("Tokio runtime");
        let _runtime_guard = tokio.enter();
        cx.executor().allow_parking();
        let (db, pinned_note_id, opened_note_id, board_id) = tokio.block_on(async {
            let mut options = ConnectOptions::new("sqlite::memory:");
            options.max_connections(1).min_connections(1);
            let db = Database::connect(options).await.expect("test database");
            Migrator::up(&db, None).await.expect("database migrations");
            let pinned_note = note::ActiveModel {
                title: Set("Pinned note".to_string()),
                project_id: Set(None),
                file_path: Set(None),
                file_managed_by_app: Set(false),
                cached_content: Set(String::new()),
                file_missing_since: Set(None),
                created_at: Set(1),
                updated_at: Set(1),
                is_pinned: Set(true),
                last_opened_at: Set(Some(1)),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("pinned note");
            let opened_note = note::ActiveModel {
                title: Set("Opened note".to_string()),
                project_id: Set(None),
                file_path: Set(None),
                file_managed_by_app: Set(false),
                cached_content: Set(String::new()),
                file_missing_since: Set(None),
                created_at: Set(2),
                updated_at: Set(2),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("opened note");
            let board = board::ActiveModel {
                title: Set("Due board".to_string()),
                project_id: Set(None),
                last_opened_at: Set(Some(2)),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("board");
            let list = card::ActiveModel {
                title: Set("Backlog".to_string()),
                board_id: Set(board.id),
                position: Set(0),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("board list");
            entry::ActiveModel {
                title: Set("Overdue task".to_string()),
                description: Set(String::new()),
                card_id: Set(list.id),
                position: Set(0),
                due_on: Set(Some(
                    (Local::now().date_naive() - Duration::days(1)).to_string(),
                )),
                ..Default::default()
            }
            .insert(&db)
            .await
            .expect("overdue task");

            (
                db,
                pinned_note.id as u32,
                opened_note.id as u32,
                board.id as u32,
            )
        });

        let settings_dir = tempfile::tempdir().expect("settings directory");
        let mut shell = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_kit::component::Theme::default());
            gpui_kit::init(cx);
            cx.set_global(AppSettings::load(settings_dir.path()));
            AppSettings::set_tab_session(
                TabSession {
                    tabs: vec![StoredTab::Chooser, StoredTab::Trash],
                    active_tab_index: 1,
                    active_project_id: None,
                },
                cx,
            );
            cx.set_global(AppRuntime::new(db, PathBuf::new()));
            cx.open_window(Default::default(), |window, cx| {
                let view = AppShell::view(window, test_shell_integration(), cx);
                shell = Some(view.clone());
                cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
            })
            .expect("shell window")
        });
        let shell = shell.expect("shell");
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        shell.read_with(&cx, |shell, _| {
            assert!(matches!(shell.home.phase, LoadPhase::Initial));
        });
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.activate_tab(0, window, cx));
        });
        for _ in 0..10_000 {
            cx.run_until_parked();
            if shell.read_with(&cx, |shell, _| matches!(shell.home.phase, LoadPhase::Ready)) {
                break;
            }
            std::thread::yield_now();
        }
        shell.read_with(&cx, |shell, _| {
            assert!(matches!(shell.home.phase, LoadPhase::Ready));
            assert_eq!(shell.home.data.pinned.len(), 1);
            assert_eq!(shell.home.data.pinned[0].id, pinned_note_id);
            assert_eq!(shell.home.data.today.len(), 1);
            assert_eq!(shell.home.data.today[0].title, "Overdue task");
            assert_eq!(shell.home.data.recent.len(), 1);
            assert_eq!(shell.home.data.recent[0].id, board_id);
        });

        cx.update(|_, cx| {
            shell.update(cx, |shell, cx| {
                shell.record_item_opened(
                    storage::workspace::home::WorkspaceItemKind::Note,
                    opened_note_id,
                    cx,
                );
            });
        });
        for _ in 0..10_000 {
            cx.run_until_parked();
            if shell.read_with(&cx, |shell, _| {
                shell.home.data.recent.len() == 2
                    && shell
                        .home
                        .data
                        .recent
                        .iter()
                        .any(|item| item.id == opened_note_id)
            }) {
                break;
            }
            std::thread::yield_now();
        }
        shell.read_with(&cx, |shell, _| {
            let recent_ids = shell
                .home
                .data
                .recent
                .iter()
                .map(|item| item.id)
                .collect::<HashSet<_>>();
            assert_eq!(recent_ids, HashSet::from([opened_note_id, board_id]));
            assert_eq!(shell.home.data.pinned[0].id, pinned_note_id);
        });
    }

    #[gpui_kit::test]
    fn restores_saved_tabs_only_when_selected_or_needed(cx: &mut TestAppContext) {
        let tokio = tokio::runtime::Runtime::new().expect("Tokio runtime");
        let _runtime_guard = tokio.enter();
        cx.executor().allow_parking();
        let db = tokio.block_on(async {
            let db = Database::connect("sqlite::memory:")
                .await
                .expect("test database");
            Migrator::up(&db, None).await.expect("database migrations");
            db
        });
        let settings_dir = tempfile::tempdir().expect("settings directory");
        let mut shell = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_kit::component::Theme::default());
            gpui_kit::init(cx);
            cx.set_global(AppSettings::load(settings_dir.path()));
            AppSettings::set_tab_session(
                TabSession {
                    tabs: (1..=3)
                        .map(|note_id| StoredTab::Note {
                            note_id,
                            project_id: (note_id == 2).then_some(7),
                            title: format!("Note {note_id}"),
                        })
                        .chain(std::iter::once(StoredTab::Board {
                            board_id: 4,
                            project_id: Some(7),
                            title: "Board 4".into(),
                        }))
                        .chain(std::iter::once(StoredTab::Board {
                            board_id: 5,
                            project_id: Some(8),
                            title: "Board 5".into(),
                        }))
                        .chain([StoredTab::Chooser, StoredTab::Trash])
                        .collect(),
                    active_tab_index: 2,
                    active_project_id: None,
                },
                cx,
            );
            cx.set_global(AppRuntime::new(db, PathBuf::new()));
            cx.open_window(Default::default(), |window, cx| {
                let view = AppShell::view(window, test_shell_integration(), cx);
                shell = Some(view.clone());
                cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
            })
            .expect("shell window")
        });
        let shell = shell.expect("shell");
        let mut cx = gpui_kit::VisualTestContext::from_window(window.into(), cx);

        shell.read_with(&cx, |shell, _| {
            assert_eq!(shell.tabs.open_tabs.len(), 7);
            assert_eq!(shell.tabs.note_views.len(), 1);
            assert_eq!(shell.tabs.active_tab_index, 2);
            assert!(shell.tabs.note_views.contains_key(&3));
            assert!(matches!(shell.home.phase, LoadPhase::Initial));
            assert!(matches!(shell.trash.phase, LoadPhase::Initial));
            assert!(matches!(
                shell.tabs.open_tabs[3].kind,
                OpenTabKind::Restored(StoredTab::Board { board_id: 4, .. })
            ));
        });

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.activate_tab(0, window, cx));
        });
        shell.read_with(&cx, |shell, cx| {
            assert_eq!(shell.tabs.active_tab_index, 0);
            assert_eq!(shell.tabs.note_views.len(), 2);
            assert!(shell.tabs.note_views.contains_key(&1));
            let session = AppSettings::tab_session(cx);
            assert_eq!(session.active_tab_index, 0);
            assert!(matches!(
                &session.tabs[3],
                StoredTab::Board {
                    board_id: 4,
                    title,
                    ..
                } if title == "Board 4"
            ));
        });

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.activate_tab(4, window, cx));
        });
        shell.read_with(&cx, |shell, _| {
            assert!(matches!(
                shell.tabs.open_tabs[4].kind,
                OpenTabKind::Board { board_id: 5, .. }
            ));
            assert!(matches!(
                shell.tabs.open_tabs[3].kind,
                OpenTabKind::Restored(StoredTab::Board { board_id: 4, .. })
            ));
        });

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.open_home(window, cx));
        });
        shell.read_with(&cx, |shell, _| {
            assert_eq!(shell.tabs.open_tabs.len(), 7);
            assert!(shell.home.phase.is_loading());
            assert!(matches!(shell.trash.phase, LoadPhase::Initial));
        });
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.open_trash(window, cx));
        });
        shell.read_with(&cx, |shell, _| {
            assert_eq!(shell.tabs.open_tabs.len(), 7);
            assert!(shell.trash.phase.is_loading());
        });
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.activate_tab(0, window, cx));
        });

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.close_project_tabs(7, window, cx));
        });
        shell.read_with(&cx, |shell, _| {
            assert_eq!(shell.tabs.open_tabs.len(), 5);
            assert_eq!(shell.tabs.note_views.len(), 2);
            assert!(matches!(
                shell.tabs.open_tabs[shell.tabs.active_tab_index].kind,
                OpenTabKind::Note { note_id: 1, .. }
            ));
        });
    }
}
