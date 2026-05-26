use super::*;

impl AppShell {
    pub(super) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.open_tabs.len();
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.open_tabs.push(OpenTab {
            id,
            title: "New tab".into(),
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
            OpenTabKind::Chooser => {}
        }

        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
        self.focus_handle.focus(window, cx);
        cx.notify();
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
                title: "New tab".into(),
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
        cx.notify();
    }

    pub(super) fn close_tab_by_id(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.open_tabs.iter().position(|tab| tab.id == id) {
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
                title: "New tab".into(),
                kind: OpenTabKind::Chooser,
            });
            self.next_tab_id = self.next_tab_id.saturating_add(1);
        }
        self.active_tab_index = 0;
        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
        cx.notify();
    }

    pub(super) fn close_all_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_tabs.clear();
        self.open_tabs.push(OpenTab {
            id: self.next_tab_id,
            title: "New tab".into(),
            kind: OpenTabKind::Chooser,
        });
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        self.active_tab_index = 0;
        self.sync_sidebar_active(cx);
        self.sync_title_input(window, cx);
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
            .unwrap_or_else(|| "New tab".to_string());

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
        match &tab.kind {
            OpenTabKind::Note { view, .. } => {
                view.update(cx, |note, cx| note.set_title(title.to_string(), cx));
                self.sidebar
                    .update(cx, |_, cx| SidebarView::list_projects(cx));
            }
            OpenTabKind::Board { board_id, .. } => {
                let db = cx.global::<DB>().conn.clone();
                let board_id = *board_id;
                let title = title.to_string();
                cx.spawn(async move |_, _| -> Result<()> {
                    board::ActiveModel {
                        id: Set(board_id as i64),
                        title: Set(title),
                        ..Default::default()
                    }
                    .update(&*db)
                    .await?;
                    Ok(())
                })
                .detach();
                self.sidebar
                    .update(cx, |_, cx| SidebarView::list_projects(cx));
                self.refresh_workspace(cx);
            }
            OpenTabKind::Chooser => {}
        }

        cx.notify();
    }

    pub(super) fn open_board_tab(
        &mut self,
        board_id: u32,
        project_id: Option<u32>,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.open_tabs.iter().position(
            |tab| matches!(&tab.kind, OpenTabKind::Board { board_id: id, .. } if *id == board_id),
        ) {
            self.activate_tab(index, window, cx);
            return;
        }

        let view = BoardView::view(window, cx);
        view.update(cx, |board, cx| board.load_board(board_id, cx));
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

    pub(super) fn open_note_tab(
        &mut self,
        note_id: u32,
        project_id: Option<u32>,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.open_tabs.iter().position(
            |tab| matches!(&tab.kind, OpenTabKind::Note { note_id: id, .. } if *id == note_id),
        ) {
            self.activate_tab(index, window, cx);
            return;
        }

        let view = MarkdownEditorView::view(note_id, window, cx);
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

    fn replace_or_push_active(
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
