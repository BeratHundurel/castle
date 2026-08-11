use super::*;

impl AppShell {
    pub(in crate::app_shell) fn render_trash(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.trash.query.trim().to_lowercase();
        let filter = self.trash.kind_filter;
        let items = self
            .trash
            .items
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
                            .children((!self.trash.items.is_empty()).then(|| {
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
                            .child(Input::new(&self.trash.search_input).prefix(IconName::Search).flex_1())
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
                                            this.trash.kind_filter = kind;
                                            cx.notify();
                                        }))
                                }),
                            ),
                    )
                    .child(self.render_trash_items(items, cx)),
            )
    }

    pub(in crate::app_shell) fn render_trash_items(
        &self,
        items: Vec<crate::trash::TrashItem>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if self.trash.phase.is_loading() && !self.trash.phase.has_content() {
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
        if let Some(error) = self.trash.phase.error() {
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

    pub(in crate::app_shell) fn restore_trash_item(
        &mut self,
        item: crate::trash::TrashItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db = cx.global::<AppServices>().store().connection();
        let runtime = cx.global::<AppServices>().runtime();
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
                    this.trash
                        .items
                        .retain(|candidate| candidate.kind != item.kind || candidate.id != item.id);
                    this.reload_open_boards_after_restore(item.kind, cx);
                    this.load_trash(cx);
                    this.load_home(cx);
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

    pub(in crate::app_shell) fn reload_open_boards_after_restore(
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

        for tab in &mut self.tabs.open_tabs {
            if let OpenTabKind::Board { board_id, view, .. } = &tab.kind {
                view.update(cx, |board, cx| board.reload_board(*board_id, cx));
            }
        }
    }

    pub(in crate::app_shell) fn confirm_purge_trash_item(
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

    pub(in crate::app_shell) fn purge_trash_item(
        &mut self,
        item: crate::trash::TrashItem,
        cx: &mut Context<Self>,
    ) {
        let app_db = cx.global::<AppServices>();
        let db = app_db.store().connection();
        let attachments_dir = app_db.data_dir().join("attachments");
        let background = cx.background_executor().clone();
        let runtime = cx.global::<AppServices>().runtime();
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
            if let Ok(artifacts) = &result {
                let artifacts = artifacts.clone();
                let _ = background
                    .spawn(async move { remove_purged_artifacts(artifacts, attachments_dir) })
                    .await;
            }
            this.update(cx, |this, cx| match result {
                Ok(_) => {
                    match item.kind {
                        TrashItemKind::Note => {
                            this.tabs.note_views.remove(&item.id);
                        }
                        TrashItemKind::Board
                        | TrashItemKind::Project
                        | TrashItemKind::List
                        | TrashItemKind::Entry => {}
                    }
                    this.load_trash(cx);
                }
                Err(err) => eprintln!("Failed to delete {} forever: {err}", item.title),
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::app_shell) fn confirm_empty_trash(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let count = self.trash.items.len();
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

    pub(in crate::app_shell) fn empty_trash(&mut self, cx: &mut Context<Self>) {
        let app_db = cx.global::<AppServices>();
        let db = app_db.store().connection();
        let attachments_dir = app_db.data_dir().join("attachments");
        let background = cx.background_executor().clone();
        let runtime = cx.global::<AppServices>().runtime();
        cx.spawn(async move |this, cx| {
            let result = match runtime
                .spawn(async move { crate::trash::purge_all(db.as_ref()).await })
                .await
            {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!(err)),
            };
            if let Ok(artifacts) = &result {
                let artifacts = artifacts.clone();
                let _ = background
                    .spawn(async move { remove_purged_artifacts(artifacts, attachments_dir) })
                    .await;
            }
            this.update(cx, |this, cx| {
                if let Err(err) = result {
                    this.trash.phase = LoadPhase::Failed {
                        message: format!("Could not empty Trash: {err}").into(),
                        had_content: true,
                    };
                } else {
                    this.tabs.note_views.clear();
                }
                this.load_trash(cx);
                this.refresh_workspace(cx);
            })
            .ok();
        })
        .detach();
    }
}
