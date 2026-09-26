use super::*;
use chrono::NaiveDate;
use runtime::AppRuntime;
use storage::{MutationOrigin, workspace::api};

impl AppShell {
    pub(crate) fn load_home_if_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let home_is_active = matches!(
            self.tabs
                .open_tabs
                .get(self.tabs.active_tab_index)
                .map(|tab| &tab.kind),
            Some(OpenTabKind::Chooser | OpenTabKind::Restored(StoredTab::Chooser))
        );
        if home_is_active {
            self.load_home(window, cx);
        }
    }

    pub(crate) fn load_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.home.phase.is_loading() {
            self.home.refresh_pending = true;
            return;
        }

        let app_runtime = cx.global::<AppRuntime>().clone();
        let db = app_runtime.store();
        self.home.planner_load_generation = self.home.planner_load_generation.wrapping_add(1);
        let generation = self.home.planner_load_generation;
        self.home.loading_more_group = None;
        self.home.phase = LoadPhase::Loading {
            had_content: self.home.phase.has_content(),
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = match app_runtime
                .spawn_tokio(cx.background_executor(), async move {
                    storage::workspace::home::load_home(&db).await
                })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!(err)),
            };
            this.update_in(cx, |this, window, cx| {
                if this.home.planner_load_generation != generation {
                    return;
                }
                match result {
                    Ok(state) => {
                        this.home.planner_calendars.clear();
                        this.home.open_planner_date_picker = None;
                        for task in state
                            .today
                            .iter()
                            .chain(&state.upcoming)
                            .chain(&state.unscheduled)
                        {
                            let picker = Self::new_planner_calendar_picker(
                                task.entry_id,
                                task.due_on.as_deref(),
                                window,
                                cx,
                            );
                            this.home.planner_calendars.insert(task.entry_id, picker);
                        }
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
                    this.load_home(window, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn load_trash(&mut self, cx: &mut Context<Self>) {
        if self.trash.phase.is_loading() {
            self.trash.refresh_pending = true;
            return;
        }

        let app_runtime = cx.global::<AppRuntime>().clone();
        let db = app_runtime.store();
        self.trash.phase = LoadPhase::Loading {
            had_content: self.trash.phase.has_content(),
        };
        cx.spawn(async move |this, cx| {
            let result = match app_runtime
                .spawn_tokio(cx.background_executor(), async move {
                    storage::workspace::trash::load_trash(&db).await
                })
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

    pub(crate) fn open_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.open_tabs.iter().position(|tab| {
            matches!(
                tab.kind,
                OpenTabKind::Chooser | OpenTabKind::Restored(StoredTab::Chooser)
            )
        }) {
            let was_active = self.tabs.active_tab_index == index;
            self.activate_tab(index, window, cx);
            if was_active {
                self.load_home_if_active(window, cx);
            }
            return;
        }
        self.replace_or_push_active(OpenTabKind::Chooser, "Home".into(), window, cx);
    }

    pub(crate) fn open_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_pending_board_open();
        if let Some(index) = self.tabs.open_tabs.iter().position(|tab| {
            matches!(
                tab.kind,
                OpenTabKind::Trash | OpenTabKind::Restored(StoredTab::Trash)
            )
        }) {
            self.activate_tab(index, window, cx);
            self.load_trash(cx);
            return;
        }
        self.replace_or_push_active(OpenTabKind::Trash, "Trash".into(), window, cx);
        self.load_trash(cx);
    }

    pub(crate) fn record_item_opened(
        &mut self,
        kind: WorkspaceItemKind,
        id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let app_runtime = cx.global::<AppRuntime>().clone();
        let db = app_runtime.store();
        self.record_opened_task = Some(cx.spawn_in(window, async move |this, cx| {
            let (cancel_on_drop, cancelled) = tokio::sync::oneshot::channel::<()>();
            let update = app_runtime.spawn_tokio(cx.background_executor(), async move {
                tokio::select! {
                    biased;
                    _ = cancelled => None,
                    result = storage::workspace::home::mark_opened(&db, kind, id, now_ts()) => {
                        Some(result)
                    }
                }
            });
            let result = update.await;
            drop(cancel_on_drop);
            match result {
                Ok(Some(Ok(()))) => {
                    this.update_in(cx, |this, window, cx| this.load_home_if_active(window, cx))
                        .ok();
                }
                Ok(Some(Err(error))) => {
                    eprintln!("Failed to record opened workspace item: {error}");
                }
                Err(error) => eprintln!("Recent-item task failed: {error}"),
                Ok(None) => {}
            }
        }));
    }

    pub(crate) fn open_home_item(
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

    pub(crate) fn open_planner_task(
        &mut self,
        entry: PlannerTask,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = planner_task_navigation_target(&entry);
        let view = self.open_board_tab(
            entry.board_id,
            entry.project_id,
            entry.board_title.into(),
            window,
            cx,
        );
        view.update(cx, |board, cx| {
            board.queue_reveal_target(target, cx);
            board.apply_pending_reveal(window, cx);
        });
    }

    pub(crate) fn load_more_planner_tasks(
        &mut self,
        group: PlannerTaskGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.home.loading_more_group.is_some() {
            return;
        }
        let (offset, total) = match group {
            PlannerTaskGroup::Today => (self.home.data.today.len(), self.home.data.today_total),
            PlannerTaskGroup::Upcoming => {
                (self.home.data.upcoming.len(), self.home.data.upcoming_total)
            }
            PlannerTaskGroup::Unscheduled => (
                self.home.data.unscheduled.len(),
                self.home.data.unscheduled_total,
            ),
        };
        if offset >= total {
            return;
        }

        self.home.loading_more_group = Some(group);
        let generation = self.home.planner_load_generation;
        self.home.planner_action_error = None;
        let runtime = cx.global::<AppRuntime>().clone();
        let store = runtime.store();
        let task = runtime.spawn_tokio(cx.background_executor(), async move {
            storage::workspace::home::load_planner_task_page(
                &store,
                group,
                offset,
                storage::workspace::home::HOME_TASK_PAGE_SIZE,
            )
            .await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = match task.await {
                Ok(result) => result,
                Err(error) => Err(anyhow::anyhow!(error)),
            };
            this.update_in(cx, |this, window, cx| {
                if this.home.planner_load_generation != generation {
                    return;
                }
                this.home.loading_more_group = None;
                match result {
                    Ok(tasks) => {
                        for task in &tasks {
                            this.home
                                .planner_calendars
                                .entry(task.entry_id)
                                .or_insert_with(|| {
                                    Self::new_planner_calendar_picker(
                                        task.entry_id,
                                        task.due_on.as_deref(),
                                        window,
                                        cx,
                                    )
                                });
                        }
                        match group {
                            PlannerTaskGroup::Today => this.home.data.today.extend(tasks),
                            PlannerTaskGroup::Upcoming => this.home.data.upcoming.extend(tasks),
                            PlannerTaskGroup::Unscheduled => {
                                this.home.data.unscheduled.extend(tasks)
                            }
                        }
                    }
                    Err(error) => {
                        this.home.planner_action_error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn complete_planner_task(
        &mut self,
        entry_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let runtime = cx.global::<AppRuntime>().clone();
        let task = runtime.spawn_store(cx.background_executor(), move |store| async move {
            store
                .mutations(MutationOrigin::LocalApp)
                .set_entry_lifecycle(api::SetEntryLifecycleInput {
                    entry_id: i64::from(entry_id),
                    state: api::EntryLifecycleState::Completed,
                })
                .await?;
            Ok::<_, anyhow::Error>(())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(Ok(())) => {
                        this.home.planner_action_error = None;
                        this.load_home_if_active(window, cx);
                    }
                    Ok(Err(error)) => {
                        this.home.planner_action_error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.home.planner_action_error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn save_planner_reschedule(
        &mut self,
        entry_id: u32,
        date: NaiveDate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.home.rescheduling {
            return;
        }
        let due_on = date.format("%Y-%m-%d").to_string();
        self.home.rescheduling = true;
        self.home.planner_action_error = None;
        cx.notify();
        let runtime = cx.global::<AppRuntime>().clone();
        let task = runtime.spawn_store(cx.background_executor(), move |store| async move {
            store
                .mutations(MutationOrigin::LocalApp)
                .set_entry_schedule(api::SetEntryScheduleInput {
                    entry_id: i64::from(entry_id),
                    start_on: None,
                    due_on: Some(due_on),
                    clear_start_on: false,
                    clear_due_on: false,
                })
                .await?;
            Ok::<_, anyhow::Error>(())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                this.home.rescheduling = false;
                match result {
                    Ok(Ok(())) => {
                        this.home.planner_action_error = None;
                        this.load_home_if_active(window, cx);
                    }
                    Ok(Err(error)) => {
                        this.home.planner_action_error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.home.planner_action_error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn new_planner_calendar_picker(
        entry_id: u32,
        due_on: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PlannerCalendarPicker {
        let selected_date =
            due_on.and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok());
        let state = cx.new(|cx| {
            let mut state = CalendarState::new(window, cx);
            state.set_date(Date::Single(selected_date), window, cx);
            state
        });
        let subscriptions = vec![
            cx.observe(&state, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &state,
                window,
                move |this, _, event: &CalendarEvent, window, cx| {
                    if let CalendarEvent::Selected(Date::Single(Some(date))) = event {
                        this.home.open_planner_date_picker = None;
                        this.save_planner_reschedule(entry_id, *date, window, cx);
                    }
                },
            ),
        ];
        PlannerCalendarPicker {
            state,
            _subscriptions: subscriptions,
        }
    }
}
