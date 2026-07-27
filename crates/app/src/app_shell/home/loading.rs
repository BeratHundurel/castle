use super::*;

impl AppShell {
    pub(in crate::app_shell) fn load_home(&mut self, cx: &mut Context<Self>) {
        if self.home_refreshing {
            self.home_refresh_pending = true;
            return;
        }

        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        self.home_refreshing = true;
        cx.spawn(async move |this, cx| {
            let result = match runtime
                .spawn(async move { crate::home::load_home(db.as_ref()).await })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!(err)),
            };
            this.update(cx, |this, cx| {
                this.home_refreshing = false;
                this.home_loaded = true;
                match result {
                    Ok(state) => {
                        this.home_state = state;
                        this.home_error = None;
                    }
                    Err(err) => {
                        this.home_error = Some(format!("Could not load Home: {err}").into())
                    }
                }
                if std::mem::take(&mut this.home_refresh_pending) {
                    this.load_home(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::app_shell) fn load_trash(&mut self, cx: &mut Context<Self>) {
        if self.trash_refreshing {
            self.trash_refresh_pending = true;
            return;
        }

        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        self.trash_refreshing = true;
        cx.spawn(async move |this, cx| {
            let result = match runtime
                .spawn(async move { crate::trash::load_trash(db.as_ref()).await })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!(err)),
            };
            this.update(cx, |this, cx| {
                this.trash_refreshing = false;
                this.trash_loaded = true;
                match result {
                    Ok(items) => {
                        this.trash_items = items;
                        this.trash_error = None;
                    }
                    Err(err) => {
                        this.trash_error = Some(format!("Could not load Trash: {err}").into())
                    }
                }
                if std::mem::take(&mut this.trash_refresh_pending) {
                    this.load_trash(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::app_shell) fn open_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self
            .open_tabs
            .iter()
            .position(|tab| matches!(tab.kind, OpenTabKind::Chooser))
        {
            self.activate_tab(index, window, cx);
            self.load_home(cx);
            return;
        }
        self.replace_or_push_active(OpenTabKind::Chooser, "Home".into(), window, cx);
        self.load_home(cx);
    }

    pub(in crate::app_shell) fn open_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_pending_board_open();
        if let Some(index) = self
            .open_tabs
            .iter()
            .position(|tab| matches!(tab.kind, OpenTabKind::Trash))
        {
            self.activate_tab(index, window, cx);
            self.load_trash(cx);
            return;
        }
        self.replace_or_push_active(OpenTabKind::Trash, "Trash".into(), window, cx);
        self.load_trash(cx);
    }

    pub(in crate::app_shell) fn record_item_opened(
        &mut self,
        kind: WorkspaceItemKind,
        id: u32,
        cx: &mut Context<Self>,
    ) {
        self.record_opened_generation = self.record_opened_generation.saturating_add(1);
        let generation = self.record_opened_generation;
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(250))
                .await;
            let is_current = this
                .read_with(cx, |this, _| this.record_opened_generation == generation)
                .unwrap_or(false);
            if !is_current {
                return;
            }
            let _ = runtime
                .spawn(
                    async move { crate::home::mark_opened(db.as_ref(), kind, id, now_ts()).await },
                )
                .await;
        })
        .detach();
    }

    pub(in crate::app_shell) fn open_home_item(
        &mut self,
        item: WorkspaceHomeItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match item.kind {
            WorkspaceItemKind::Note => {
                self.open_note_tab(item.id, item.project_id, item.title.into(), window, cx)
            }
            WorkspaceItemKind::Board => {
                self.open_board_tab(item.id, item.project_id, item.title.into(), window, cx);
            }
        }
    }

    pub(in crate::app_shell) fn open_today_entry(
        &mut self,
        entry: TodayEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = self.open_board_tab(
            entry.board_id,
            entry.project_id,
            entry.board_title.clone().into(),
            window,
            cx,
        );
        view.update(cx, |board, cx| {
            board.open_entry_dialog(entry.entry_id, window, cx);
        });
    }
}
