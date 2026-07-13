use chrono::{Local, TimeZone as _};
use gpui::StatefulInteractiveElement as _;
use gpui_component::{
    Icon, Selectable as _, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    input::Input,
    scroll::ScrollableElement as _,
};

use super::*;
use crate::home::{TodayEntry, WorkspaceHomeItem, WorkspaceItemKind};
use crate::trash::{MoveToTrash, PurgeTrashItem, RestoreTrashItem};

impl AppShell {
    pub(super) fn load_home(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn load_trash(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn open_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn open_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn record_item_opened(
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

    fn open_home_item(
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
                self.open_board_tab(item.id, item.project_id, item.title.into(), window, cx)
            }
        }
    }

    fn open_today_entry(&mut self, entry: TodayEntry, window: &mut Window, cx: &mut Context<Self>) {
        self.open_board_tab(
            entry.board_id,
            entry.project_id,
            entry.board_title.clone().into(),
            window,
            cx,
        );
        if let Some(OpenTabKind::Board { view, .. }) = self
            .open_tabs
            .get(self.active_tab_index)
            .map(|tab| &tab.kind)
        {
            view.update(cx, |board, cx| {
                board.open_entry_dialog(entry.entry_id, window, cx);
            });
        }
    }

    pub(super) fn render_home(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_project = self
            .active_project_id
            .and_then(|id| self.projects.iter().find(|project| project.id == id));
        let active_project_id = active_project.map(|project| project.id);

        v_flex()
            .id("workspace-home")
            .size_full()
            .overflow_y_scrollbar()
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(1080.))
                    .mx_auto()
                    .p_6()
                    .gap_6()
                    .child(
                        h_flex()
                            .items_end()
                            .justify_between()
                            .gap_4()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_2xl()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("Home"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child("The work that needs your attention, without the noise."),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("home-new-note")
                                            .icon(IconName::Plus)
                                            .label(match active_project {
                                                Some(project) => format!("Note in {}", project.name),
                                                None => "New note".to_string(),
                                            })
                                            .primary()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.create_note(active_project_id, window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("home-new-board")
                                            .icon(IconName::LayoutDashboard)
                                            .label("New board")
                                            .outline()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.create_board(active_project_id, window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .items_start()
                            .gap_6()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_3()
                                    .child(section_title("Today", "Overdue and due today", cx))
                                    .child(self.render_today(cx)),
                            )
                            .child(
                                v_flex()
                                    .w(px(320.))
                                    .flex_shrink_0()
                                    .gap_6()
                                    .child(
                                        v_flex()
                                            .gap_3()
                                            .child(section_title("Pinned", "Keep close", cx))
                                            .child(self.render_home_items(
                                                "home-pinned",
                                                &self.home_state.pinned,
                                                "Pin notes or boards from their item menu.",
                                                cx,
                                            )),
                                    )
                                    .child(
                                        v_flex()
                                            .gap_3()
                                            .child(section_title("Recent", "Last opened", cx))
                                            .child(self.render_home_items(
                                                "home-recent",
                                                &self.home_state.recent,
                                                "Open a note or board and it will appear here.",
                                                cx,
                                            )),
                                    ),
                            ),
                    ),
            )
    }

    fn render_today(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.home_refreshing && !self.home_loaded {
            return v_flex()
                .gap_2()
                .children((0_usize..3).map(|index| {
                    div()
                        .id(("home-today-skeleton", index))
                        .h(px(64.))
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().secondary.opacity(0.62))
                }))
                .into_any_element();
        }
        if let Some(error) = self.home_error.clone() {
            return inline_retry(error, cx.listener(|this, _, _, cx| this.load_home(cx)), cx)
                .into_any_element();
        }
        if self.home_state.today.is_empty() {
            return empty_state(
                IconName::Calendar,
                "Nothing due today",
                "Your boards are clear for today.",
                cx,
            )
            .into_any_element();
        }

        v_flex()
            .gap_2()
            .children(
                self.home_state
                    .today
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(index, entry)| {
                        let overdue = entry.due_on < Local::now().date_naive().to_string();
                        let breadcrumb = if entry.labels.is_empty() {
                            format!("{} / {}", entry.board_title, entry.list_title)
                        } else {
                            format!(
                                "{} / {} / {}",
                                entry.board_title,
                                entry.list_title,
                                entry
                                    .labels
                                    .iter()
                                    .take(2)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        };
                        let checklist = (entry.checklist_total > 0).then(|| {
                            format!("{}/{}", entry.checklist_checked, entry.checklist_total)
                        });
                        h_flex()
                            .id(("home-today-entry", index))
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_3()
                            .px_3()
                            .py_3()
                            .rounded(cx.theme().radius)
                            .border_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .bg(cx.theme().secondary.opacity(0.34))
                            .hover(|this| {
                                this.bg(cx.theme().secondary_hover.opacity(0.62))
                                    .border_color(cx.theme().primary.opacity(0.32))
                            })
                            .child(div().w(px(3.)).h_8().rounded_full().bg(if overdue {
                                cx.theme().danger
                            } else {
                                cx.theme().warning
                            }))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child(entry.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(breadcrumb),
                                    ),
                            )
                            .children(checklist.map(|value| {
                                h_flex()
                                    .gap_1()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(Icon::new(IconName::CircleCheck).xsmall())
                                    .child(value)
                            }))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(if overdue {
                                        cx.theme().danger
                                    } else {
                                        cx.theme().warning
                                    })
                                    .child(if overdue { "Overdue" } else { "Today" }),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_today_entry(entry.clone(), window, cx);
                            }))
                    }),
            )
            .into_any_element()
    }

    fn render_home_items(
        &self,
        id: &'static str,
        items: &[WorkspaceHomeItem],
        empty_copy: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if items.is_empty() {
            return div()
                .id(id)
                .p_3()
                .rounded(cx.theme().radius)
                .bg(cx.theme().secondary.opacity(0.32))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(empty_copy)
                .into_any_element();
        }
        v_flex()
            .id(id)
            .gap_1()
            .children(items.iter().cloned().enumerate().map(|(index, item)| {
                let icon = match item.kind {
                    WorkspaceItemKind::Note => IconName::BookOpen,
                    WorkspaceItemKind::Board => IconName::LayoutDashboard,
                };
                h_flex()
                    .id((id, index))
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .px_2()
                    .py_2()
                    .rounded(cx.theme().radius)
                    .hover(|this| this.bg(cx.theme().secondary_hover.opacity(0.7)))
                    .child(
                        Icon::new(icon)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_ellipsis()
                            .overflow_hidden()
                            .child(item.title.clone()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_home_item(item.clone(), window, cx);
                    }))
            }))
            .into_any_element()
    }

    pub(super) fn render_trash(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.trash_query.trim().to_lowercase();
        let filter = self.trash_kind_filter;
        let items = self
            .trash_items
            .iter()
            .filter(|item| {
                filter.is_none_or(|kind| item.kind == kind)
                    && (query.is_empty()
                        || item.title.to_lowercase().contains(&query)
                        || item
                            .location
                            .as_deref()
                            .is_some_and(|location| location.to_lowercase().contains(&query)))
            })
            .cloned()
            .collect::<Vec<_>>();

        v_flex()
            .id("trash-view")
            .size_full()
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .size_full()
                    .max_w(px(980.))
                    .mx_auto()
                    .p_6()
                    .gap_5()
                    .child(
                        h_flex()
                            .items_end()
                            .justify_between()
                            .gap_4()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(div().text_2xl().font_weight(gpui::FontWeight::SEMIBOLD).child("Trash"))
                                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Restore anything you removed, or delete it permanently.")),
                            )
                            .children((!self.trash_items.is_empty()).then(|| {
                                Button::new("empty-trash")
                                    .label("Empty Trash")
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm_empty_trash(window, cx);
                                    }))
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Input::new(&self.trash_search_input).prefix(IconName::Search).flex_1())
                            .children(
                                [
                                    ("All", None),
                                    ("Notes", Some(TrashItemKind::Note)),
                                    ("Boards", Some(TrashItemKind::Board)),
                                    ("Projects", Some(TrashItemKind::Project)),
                                    ("Lists", Some(TrashItemKind::List)),
                                    ("Cards", Some(TrashItemKind::Entry)),
                                ]
                                .into_iter()
                                .enumerate()
                                .map(|(index, (label, kind))| {
                                    Button::new(("trash-filter", index))
                                        .label(label)
                                        .ghost()
                                        .small()
                                        .selected(filter == kind)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.trash_kind_filter = kind;
                                            cx.notify();
                                        }))
                                }),
                            ),
                    )
                    .child(self.render_trash_items(items, cx)),
            )
    }

    fn render_trash_items(
        &self,
        items: Vec<crate::trash::TrashItem>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if self.trash_refreshing && !self.trash_loaded {
            return v_flex()
                .gap_2()
                .children((0_usize..4).map(|index| {
                    div()
                        .id(("trash-skeleton", index))
                        .h_12()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().secondary.opacity(0.62))
                }))
                .into_any_element();
        }
        if let Some(error) = self.trash_error.clone() {
            return inline_retry(error, cx.listener(|this, _, _, cx| this.load_trash(cx)), cx)
                .into_any_element();
        }
        if items.is_empty() {
            return empty_state(
                IconName::Delete,
                "Trash is empty",
                "Removed items will appear here.",
                cx,
            )
            .into_any_element();
        }

        v_flex()
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .gap_1()
            .children(items.into_iter().map(|item| {
                let item_key = format!("{}-{}", item.kind.key(), item.id);
                let deleted = Local
                    .timestamp_opt(item.deleted_at, 0)
                    .single()
                    .map(|value| value.format("%b %-d, %H:%M").to_string())
                    .unwrap_or_else(|| "Recently".to_string());
                h_flex()
                    .id(format!("trash-item-{item_key}"))
                    .w_full()
                    .gap_3()
                    .items_center()
                    .px_3()
                    .py_3()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.58))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .child(item.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(item.kind.label()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} / {deleted}",
                                        item.location
                                            .clone()
                                            .unwrap_or_else(|| "Workspace".to_string())
                                    )),
                            ),
                    )
                    .child(
                        Button::new(format!("restore-trash-item-{item_key}"))
                            .label("Restore")
                            .outline()
                            .small()
                            .on_click(cx.listener({
                                let item = item.clone();
                                move |this, _, window, cx| {
                                    this.restore_trash_item(item.clone(), window, cx)
                                }
                            })),
                    )
                    .child(
                        Button::new(format!("purge-trash-item-{item_key}"))
                            .icon(IconName::Delete)
                            .ghost()
                            .small()
                            .tooltip("Delete forever")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.confirm_purge_trash_item(item.clone(), window, cx);
                            })),
                    )
            }))
            .into_any_element()
    }

    fn restore_trash_item(
        &mut self,
        item: crate::trash::TrashItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        cx.spawn_in(window, async move |this, cx| {
            let request = RestoreTrashItem(MoveToTrash {
                kind: item.kind,
                id: item.id,
            });
            let result = match runtime
                .spawn(async move { crate::trash::restore_item(db.as_ref(), request).await })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!(err)),
            };
            this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    this.trash_items
                        .retain(|candidate| candidate.kind != item.kind || candidate.id != item.id);
                    this.reload_open_boards_after_restore(item.kind, cx);
                    this.load_trash(cx);
                    this.load_home(cx);
                    this.sidebar
                        .update(cx, |sidebar, cx| sidebar.refresh_projects(cx));
                    this.refresh_workspace(cx);
                }
                Err(err) => {
                    this.load_trash(cx);
                    window.push_notification(
                        gpui_component::notification::Notification::error(err.to_string()),
                        cx,
                    );
                }
            })
            .ok();
        })
        .detach();
    }

    fn reload_open_boards_after_restore(
        &mut self,
        kind: crate::trash::TrashItemKind,
        cx: &mut Context<Self>,
    ) {
        if !matches!(
            kind,
            crate::trash::TrashItemKind::List | crate::trash::TrashItemKind::Entry
        ) {
            return;
        }

        for tab in &mut self.open_tabs {
            if let OpenTabKind::Board { board_id, view, .. } = &tab.kind {
                view.update(cx, |board, cx| board.reload_board(*board_id, cx));
            }
        }
    }

    fn confirm_purge_trash_item(
        &mut self,
        item: crate::trash::TrashItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let title = item.title.clone();
        window.open_alert_dialog(cx, move |alert, _, cx| {
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
                .title(format!("Delete ‘{title}’ forever"))
                .description("This permanently removes the item and cannot be undone.")
                .button_props(
                    DialogButtonProps::default()
                        .ok_variant(ButtonVariant::Danger)
                        .ok_text("Delete forever")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok({
                    let view = view.clone();
                    let item = item.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| this.purge_trash_item(item.clone(), cx));
                        true
                    }
                })
        });
    }

    fn purge_trash_item(&mut self, item: crate::trash::TrashItem, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();
        let background = cx.background_executor().clone();
        let runtime = tokio::runtime::Handle::current();
        cx.spawn(async move |this, cx| {
            let request = PurgeTrashItem(MoveToTrash {
                kind: item.kind,
                id: item.id,
            });
            let result = match runtime
                .spawn(async move { crate::trash::purge_item(db.as_ref(), request).await })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!(err)),
            };
            if let Ok(paths) = &result {
                let paths = paths.clone();
                let _ = background
                    .spawn(async move {
                        for path in paths {
                            let _ = std::fs::remove_file(path);
                        }
                    })
                    .await;
            }
            this.update(cx, |this, cx| match result {
                Ok(_) => {
                    match item.kind {
                        TrashItemKind::Note => {
                            this.note_views.remove(&item.id);
                        }
                        TrashItemKind::Board => {
                            this.board_views.remove(&item.id);
                        }
                        TrashItemKind::Project | TrashItemKind::List | TrashItemKind::Entry => {}
                    }
                    this.load_trash(cx);
                }
                Err(err) => eprintln!("Failed to delete {} forever: {err}", item.title),
            })
            .ok();
        })
        .detach();
    }

    fn confirm_empty_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let count = self.trash_items.len();
        window.open_alert_dialog(cx, move |alert, _, cx| {
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
                .title("Empty Trash")
                .description(format!(
                    "This permanently deletes {count} item(s) and cannot be undone."
                ))
                .button_props(
                    DialogButtonProps::default()
                        .ok_variant(ButtonVariant::Danger)
                        .ok_text("Empty Trash")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| this.empty_trash(cx));
                        true
                    }
                })
        });
    }

    fn empty_trash(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();
        let background = cx.background_executor().clone();
        let runtime = tokio::runtime::Handle::current();
        cx.spawn(async move |this, cx| {
            let result = match runtime
                .spawn(async move { crate::trash::purge_all(db.as_ref()).await })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!(err)),
            };
            if let Ok(paths) = &result {
                let paths = paths.clone();
                let _ = background
                    .spawn(async move {
                        for path in paths {
                            let _ = std::fs::remove_file(path);
                        }
                    })
                    .await;
            }
            this.update(cx, |this, cx| {
                if let Err(err) = result {
                    this.trash_error = Some(format!("Could not empty Trash: {err}").into());
                } else {
                    this.note_views.clear();
                    this.board_views.clear();
                }
                this.load_trash(cx);
                this.sidebar
                    .update(cx, |sidebar, cx| sidebar.list_projects(cx));
                this.refresh_workspace(cx);
            })
            .ok();
        })
        .detach();
    }
}

fn section_title(
    title: &'static str,
    subtitle: &'static str,
    cx: &mut Context<AppShell>,
) -> impl IntoElement {
    h_flex()
        .items_end()
        .justify_between()
        .child(
            div()
                .text_lg()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(subtitle),
        )
}

fn empty_state(
    icon: IconName,
    title: &'static str,
    body: &'static str,
    cx: &mut Context<AppShell>,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .items_center()
        .gap_2()
        .p_6()
        .rounded(cx.theme().radius)
        .bg(cx.theme().secondary.opacity(0.28))
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(icon).small())
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(cx.theme().foreground)
                .child(title),
        )
        .child(div().text_xs().child(body))
}

fn inline_retry(
    error: SharedString,
    retry: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &mut Context<AppShell>,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .justify_between()
        .gap_3()
        .p_3()
        .rounded(cx.theme().radius)
        .bg(cx.theme().danger.opacity(0.08))
        .text_sm()
        .text_color(cx.theme().danger)
        .child(error)
        .child(
            Button::new("retry-workspace-view")
                .label("Retry")
                .outline()
                .small()
                .on_click(retry),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use entity::{board, card, entry, note};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};
    use std::{path::PathBuf, sync::Arc, time::Duration};

    #[gpui::test]
    fn rapid_tab_churn_keeps_database_and_views_responsive(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();

        let database_path = std::env::temp_dir().join(format!(
            "castle-tab-churn-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after the Unix epoch")
                .as_nanos()
        ));
        std::fs::File::create(&database_path).expect("test database file should be created");
        let database_url = format!("sqlite:{}", database_path.display()).replace('\\', "/");

        let (db, note_id, board_id) = runtime
            .block_on(async {
                let db = Database::connect(database_url).await?;
                Migrator::up(&db, None).await?;
                let note = note::ActiveModel {
                    title: Set("Restored note".to_string()),
                    project_id: Set(None),
                    file_path: Set(None),
                    file_managed_by_app: Set(false),
                    cached_content: Set("# Restored content".to_string()),
                    file_missing_since: Set(None),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                let board = board::ActiveModel {
                    title: Set("Restored board".to_string()),
                    project_id: Set(None),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                let list = card::ActiveModel {
                    title: Set("Todo".to_string()),
                    board_id: Set(board.id),
                    position: Set(0),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                entry::ActiveModel {
                    title: Set("Restored card".to_string()),
                    description: Set(String::new()),
                    card_id: Set(list.id),
                    position: Set(0),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>((db, note.id as u32, board.id as u32))
            })
            .expect("tab churn test setup should succeed");

        let settings_dir =
            std::env::temp_dir().join(format!("castle-restore-test-{}", std::process::id()));
        let db = Arc::new(db);
        let held_connection = runtime
            .block_on(db.get_sqlite_connection_pool().acquire())
            .expect("test should reserve the SQLite connection");
        let app_db = crate::DB {
            conn: db.clone(),
            data_dir: PathBuf::new(),
        };
        let mut shell = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(crate::app_settings::AppSettings::load(settings_dir));
            cx.set_global(app_db);
            cx.open_window(Default::default(), |window, cx| {
                let view = AppShell::view(window, cx);
                shell = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("restore test window should open")
        });
        let shell = shell.expect("app shell should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_note_tab(note_id, None, "Restored note".into(), window, cx);
                shell.open_board_tab(board_id, None, "Restored board".into(), window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.close_all_tabs(window, cx);
            });
        });
        drop(held_connection);

        for _ in 0..100 {
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| {
                    shell.open_note_tab(note_id, None, "Restored note".into(), window, cx);
                });
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| {
                    shell.close_all_tabs(window, cx);
                    shell.open_board_tab(board_id, None, "Restored board".into(), window, cx);
                });
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| shell.close_all_tabs(window, cx));
            });
        }

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_note_tab(note_id, None, "Restored note".into(), window, cx);
                shell.open_board_tab(board_id, None, "Restored board".into(), window, cx);
            });
        });

        for _ in 0..100 {
            cx.run_until_parked();
            std::thread::sleep(Duration::from_millis(20));
        }

        let (note_view, board_view) = shell.read_with(&cx, |shell, _| {
            let note_view = shell.open_tabs.iter().find_map(|tab| match &tab.kind {
                OpenTabKind::Note { view, .. } => Some(view.clone()),
                _ => None,
            });
            let board_view = shell.open_tabs.iter().find_map(|tab| match &tab.kind {
                OpenTabKind::Board { view, .. } => Some(view.clone()),
                _ => None,
            });
            (
                note_view.expect("restored note tab should exist"),
                board_view.expect("restored board tab should exist"),
            )
        });

        for _ in 0..50 {
            cx.run_until_parked();
            let note_loaded = note_view
                .read_with(&cx, |note, cx| note.loaded_content(cx))
                .is_some();
            let board_loaded = board_view.read_with(&cx, |board, _| board.loaded_card_count() == 1);
            if note_loaded && board_loaded {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert_eq!(
            note_view.read_with(&cx, |note, cx| note.loaded_content(cx)),
            Some("# Restored content".to_string())
        );
        assert_eq!(
            board_view.read_with(&cx, |board, _| board.loaded_card_count()),
            1
        );

        runtime
            .block_on(tokio::time::timeout(Duration::from_secs(1), async {
                entity::project::ActiveModel {
                    name: Set("Created after restore".to_string()),
                    archived: Set(false),
                    position: Set(1),
                    ..Default::default()
                }
                .insert(db.as_ref())
                .await?;
                card::ActiveModel {
                    title: Set("Added after restore".to_string()),
                    board_id: Set(board_id as i64),
                    position: Set(1),
                    ..Default::default()
                }
                .insert(db.as_ref())
                .await?;
                Ok::<_, sea_orm::DbErr>(())
            }))
            .expect("database should remain responsive after tab churn")
            .expect("post-churn writes should succeed");

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell
                    .sidebar
                    .update(cx, |sidebar, cx| sidebar.refresh_projects(cx));
                if let Some(index) = shell.open_tabs.iter().position(
                    |tab| matches!(tab.kind, OpenTabKind::Board { board_id: id, .. } if id == board_id),
                ) {
                    shell.close_tab(index, window, cx);
                }
                shell.open_board_tab(board_id, None, "Restored board".into(), window, cx);
            });
        });

        for _ in 0..100 {
            cx.run_until_parked();
            let sidebar_has_project = shell.read_with(&cx, |shell, cx| {
                shell
                    .sidebar
                    .read(cx)
                    .contains_project_named("Created after restore")
            });
            let reopened_board_has_lists = shell.read_with(&cx, |shell, cx| {
                shell.open_tabs.iter().any(|tab| match &tab.kind {
                    OpenTabKind::Board { view, .. } => view.read(cx).loaded_card_count() == 2,
                    _ => false,
                })
            });
            if sidebar_has_project && reopened_board_has_lists {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(shell.read_with(&cx, |shell, cx| {
            shell
                .sidebar
                .read(cx)
                .contains_project_named("Created after restore")
        }));
        assert!(shell.read_with(&cx, |shell, cx| {
            shell.open_tabs.iter().any(|tab| match &tab.kind {
                OpenTabKind::Board { view, .. } => view.read(cx).loaded_card_count() == 2,
                _ => false,
            })
        }));
    }
}
