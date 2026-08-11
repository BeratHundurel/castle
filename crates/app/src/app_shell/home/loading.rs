use super::*;

impl AppShell {
    pub(in crate::app_shell) fn load_home(&mut self, cx: &mut Context<Self>) {
        if self.home.phase.is_loading() {
            self.home.refresh_pending = true;
            return;
        }

        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        self.home.phase = LoadPhase::Loading {
            had_content: self.home.phase.has_content(),
        };
        cx.spawn(async move |this, cx| {
            let result = match runtime
                .spawn(async move { crate::home::load_home(&db).await })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!(err)),
            };
            this.update(cx, |this, cx| {
                match result {
                    Ok(state) => {
                        this.home.data = state;
                        this.home.phase = LoadPhase::Ready;
                    }
                    Err(err) => {
                        this.home.phase = LoadPhase::Failed {
                            message: format!("Could not load Home: {err}").into(),
                            had_content: this.home.phase.has_content(),
                        };
                    }
                }
                if std::mem::take(&mut this.home.refresh_pending) {
                    this.load_home(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::app_shell) fn load_trash(&mut self, cx: &mut Context<Self>) {
        if self.trash.phase.is_loading() {
            self.trash.refresh_pending = true;
            return;
        }

        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        self.trash.phase = LoadPhase::Loading {
            had_content: self.trash.phase.has_content(),
        };
        cx.spawn(async move |this, cx| {
            let result = match runtime
                .spawn(async move { crate::trash::load_trash(&db).await })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!(err)),
            };
            this.update(cx, |this, cx| {
                match result {
                    Ok(items) => {
                        this.trash.items = items;
                        this.trash.phase = LoadPhase::Ready;
                    }
                    Err(err) => {
                        this.trash.phase = LoadPhase::Failed {
                            message: format!("Could not load Trash: {err}").into(),
                            had_content: this.trash.phase.has_content(),
                        };
                    }
                }
                if std::mem::take(&mut this.trash.refresh_pending) {
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
            .tabs
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
            .tabs
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
        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        self.record_opened_task = Some(cx.spawn(async move |_, _| {
            let (cancel_on_drop, cancelled) = tokio::sync::oneshot::channel::<()>();
            let update = runtime.spawn(async move {
                tokio::select! {
                    biased;
                    _ = cancelled => None,
                    result = crate::home::mark_opened(&db, kind, id, now_ts()) => {
                        Some(result)
                    }
                }
            });
            let result = update.await;
            drop(cancel_on_drop);
            match result {
                Ok(Some(Err(error))) => {
                    eprintln!("Failed to record opened workspace item: {error}");
                }
                Err(error) => eprintln!("Recent-item task failed: {error}"),
                Ok(Some(Ok(())) | None) => {}
            }
        }));
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
