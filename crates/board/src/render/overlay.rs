use super::*;

fn entry_metadata_sections(
    labels: impl IntoElement,
    due_date: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .min_h(px(132.))
        .items_stretch()
        .gap_4()
        .flex_wrap()
        .p_3()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border.opacity(0.4))
        .bg(cx.theme().secondary.opacity(0.1))
        .child(
            div()
                .id("entry-metadata-labels")
                .debug_selector(|| "entry-metadata-labels".into())
                .min_w(px(260.))
                .flex_1()
                .child(labels),
        )
        .child(
            div()
                .id("entry-metadata-due-date")
                .debug_selector(|| "entry-metadata-due-date".into())
                .min_w(px(260.))
                .flex_1()
                .border_l_1()
                .border_color(cx.theme().border.opacity(0.32))
                .pl_4()
                .child(due_date),
        )
}

impl BoardView {
    pub(super) fn render_entry_detail_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let selected_entry = self.selected_entry();

        div()
            .id("entry-detail-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_stretch()
            .justify_end()
            .bg(theme.overlay.opacity(0.72))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_entry_dialog(cx)),
            )
            .child(
                v_flex()
                    .id("entry-detail-panel")
                    .w(px(640.))
                    .min_w(px(420.))
                    .max_w(relative(0.94))
                    .h_full()
                    .overflow_hidden()
                    .rounded_none()
                    .border_l_1()
                    .border_color(theme.border.opacity(0.78))
                    .bg(theme.popover)
                    .text_color(theme.popover_foreground)
                    .shadow_lg()
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.render_entry_detail_header(selected_entry, cx))
                    .child(self.render_entry_detail_body(selected_entry, cx))
                    .when(self.entry_editing.dialog.editing, |this| {
                        this.child(self.render_entry_detail_footer(cx))
                    }),
            )
    }

    pub(super) fn render_entry_detail_header(
        &self,
        selected_entry: Option<(&str, &BoardCardState)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();

        h_flex()
            .flex_shrink_0()
            .items_start()
            .gap_4()
            .px_5()
            .py_4()
            .border_b_1()
            .border_color(theme.border.opacity(0.74))
            .bg(theme.popover)
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::LayoutDashboard).xsmall())
                            .child(match selected_entry {
                                Some((card_title, _)) => SharedString::from(card_title.to_string()),
                                None => SharedString::from("Card details"),
                            }),
                    )
                    .when_else(
                        self.entry_editing.dialog.editing,
                        |this| {
                            this.child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(relative(1.15))
                                    .child("Edit card"),
                            )
                        },
                        |this| {
                            this.child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(relative(1.15))
                                    .whitespace_normal()
                                    .child(match selected_entry {
                                        Some((_, entry)) => entry.title.clone(),
                                        None => SharedString::from("Card not found"),
                                    }),
                            )
                        },
                    ),
            )
            .child(self.render_entry_header_actions(cx))
    }

    pub(super) fn render_entry_header_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entry_id = self.entry_editing.dialog.entry_id;
        let move_destinations = entry_id
            .and_then(|entry_id| {
                self.data
                    .lists
                    .iter()
                    .find(|card| card.entries.iter().any(|entry| entry.id == entry_id))
                    .map(|source_card| {
                        self.data
                            .lists
                            .iter()
                            .filter(|card| card.id != source_card.id)
                            .map(|card| (card.id, card.title.clone()))
                            .collect::<Vec<_>>()
                    })
            })
            .unwrap_or_default();

        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .when(!self.entry_editing.dialog.editing, |this| {
                this.child(
                    Button::new("edit-entry")
                        .icon(IconName::Replace)
                        .ghost()
                        .compact()
                        .tooltip("Edit")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.start_editing_entry(window, cx);
                        })),
                )
                .child(
                    Button::new("entry-actions")
                        .icon(IconName::Ellipsis)
                        .ghost()
                        .compact()
                        .tooltip("Card actions")
                        .dropdown_menu_with_anchor(Anchor::LeftCenter, move |menu, window, cx| {
                            let danger = cx.theme().danger;
                            let menu = if let Some(entry_id) = entry_id
                                && !move_destinations.is_empty()
                            {
                                let destinations = move_destinations.clone();
                                menu.submenu("Move to list", window, cx, move |menu, _, _| {
                                    destinations.iter().fold(
                                        menu,
                                        |menu, (target_card_id, title)| {
                                            menu.menu(
                                                title.clone(),
                                                Box::new(MoveEntryAction {
                                                    entry_id,
                                                    target_card_id: *target_card_id,
                                                }),
                                            )
                                        },
                                    )
                                })
                                .separator()
                            } else {
                                menu
                            };

                            menu.menu_element(Box::new(DuplicateEntryAction), move |_, _| {
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .items_center()
                                    .justify_between()
                                    .child("Duplicate card")
                                    .child(Icon::new(IconName::Copy).xsmall())
                            })
                            .menu_element(Box::new(CopyCardInternalLinkAction), move |_, _| {
                                h_flex()
                                    .w_full()
                                    .gap_2()
                                    .items_center()
                                    .justify_between()
                                    .child("Copy internal link")
                                    .child(Icon::new(IconName::Copy).xsmall())
                            })
                            .menu_element(
                                Box::new(DeleteEntryAction),
                                move |_, _| {
                                    h_flex()
                                        .w_full()
                                        .gap_2()
                                        .items_center()
                                        .justify_between()
                                        .text_color(danger)
                                        .child("Delete")
                                        .child(Icon::new(IconName::Delete).xsmall())
                                },
                            )
                        }),
                )
            })
            .child(
                Button::new("close-entry-detail")
                    .icon(IconName::Close)
                    .ghost()
                    .xsmall()
                    .tooltip("Close")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_entry_dialog(cx);
                    })),
            )
    }

    pub(super) fn render_entry_detail_body(
        &self,
        selected_entry: Option<(&str, &BoardCardState)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();

        if self.entry_editing.dialog.editing {
            return div().flex_1().overflow_hidden().child(
                v_flex()
                    .id("entry-detail-edit-scroll")
                    .size_full()
                    .flex_1()
                    .gap_4()
                    .p_5()
                    .overflow_y_scrollbar()
                    .child(self.render_entry_edit_form(cx)),
            );
        }

        let labels = self
            .render_entry_labels(selected_entry, cx)
            .into_any_element();
        let due_date = self
            .render_entry_due_date(selected_entry, cx)
            .into_any_element();

        div().flex_1().overflow_hidden().child(
            v_flex()
                .id("entry-detail-content-scroll")
                .size_full()
                .flex_1()
                .gap_4()
                .p_5()
                .bg(theme.popover)
                .overflow_y_scrollbar()
                .child(
                    v_flex()
                        .gap_4()
                        .child(self.render_entry_description(selected_entry, cx))
                        .child(self.render_entry_related_notes(selected_entry, cx))
                        .child(entry_metadata_sections(labels, due_date, cx))
                        .child(self.render_entry_properties(selected_entry, cx))
                        .child(self.render_entry_checklist(selected_entry, cx)),
                ),
        )
    }

    pub(super) fn render_entry_due_date(
        &self,
        selected_entry: Option<(&str, &BoardCardState)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let due_on = selected_entry
            .and_then(|(_, entry)| entry.due_on.as_deref())
            .filter(|due_on| !due_on.trim().is_empty());
        let reminder_enabled = selected_entry
            .map(|(_, entry)| entry.reminder_enabled)
            .unwrap_or(false);
        let reminder_view = cx.entity();
        let notifications = cx.global::<crate::BoardServices>().notifications();
        let notification_availability = notifications.availability();
        let (notification_label, notification_help, notification_icon, notification_color) =
            match notification_availability {
                crate::notifications::NotificationAvailability::Enabled => (
                    "System notifications on",
                    if reminder_enabled {
                        "Castle will alert you on the due date or the next time it starts."
                    } else {
                        "Windows is ready. Enable this card's reminder to receive an alert."
                    },
                    IconName::CircleCheck,
                    cx.theme().success,
                ),
                crate::notifications::NotificationAvailability::DisabledForApplication => (
                    "System notifications off",
                    "Windows is blocking Castle notifications. Enable them in Settings.",
                    IconName::CircleX,
                    cx.theme().danger,
                ),
                crate::notifications::NotificationAvailability::DisabledForUser => (
                    "System notifications off",
                    "Windows notifications are turned off. Enable them in Settings.",
                    IconName::CircleX,
                    cx.theme().danger,
                ),
                crate::notifications::NotificationAvailability::DisabledByPolicy => (
                    "System notifications blocked",
                    "Notifications are disabled by system policy.",
                    IconName::CircleX,
                    cx.theme().danger,
                ),
                crate::notifications::NotificationAvailability::Unsupported => (
                    "System notifications unavailable",
                    "Castle does not support system notifications on this platform yet.",
                    IconName::Info,
                    cx.theme().muted_foreground,
                ),
                crate::notifications::NotificationAvailability::Unavailable => (
                    "Notification status unavailable",
                    "Castle could not check the system notification service.",
                    IconName::Info,
                    cx.theme().warning,
                ),
            };
        let (status_label, status_color) = match due_on {
            Some(due_on) => match due_date_status(due_on, Local::now().date_naive()) {
                DueDateStatus::Overdue => ("Overdue", cx.theme().danger),
                DueDateStatus::Today => ("Today", cx.theme().primary),
                DueDateStatus::Future => ("Scheduled", cx.theme().success),
                DueDateStatus::Invalid => ("Invalid", cx.theme().warning),
            },
            None => ("Unscheduled", cx.theme().muted_foreground),
        };
        let reminder_help = if due_on.is_some() {
            "Notify when this card is due."
        } else {
            "Choose a due date to enable reminders."
        };

        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().muted_foreground)
                            .child(Icon::new(IconName::Calendar).xsmall())
                            .child("Due date"),
                    )
                    .child(
                        div()
                            .rounded(px(3.))
                            .px_1p5()
                            .py(px(2.))
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .bg(status_color.opacity(0.14))
                            .text_color(status_color)
                            .child(status_label),
                    ),
            )
            .child(
                DatePicker::new(&self.entry_editing.due_date_picker)
                    .w_full()
                    .cleanable(true)
                    .placeholder("No due date")
                    .number_of_months(1),
            )
            .child(
                v_flex()
                    .gap_2()
                    .pt_3()
                    .border_t_1()
                    .border_color(cx.theme().border.opacity(0.36))
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_1p5()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(Icon::new(IconName::Bell).xsmall())
                                            .child("Reminder"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(reminder_help),
                                    ),
                            )
                            .child(
                                Checkbox::new("toggle-card-reminder")
                                    .xsmall()
                                    .label(if reminder_enabled { "On" } else { "Off" })
                                    .checked(reminder_enabled)
                                    .disabled(due_on.is_none())
                                    .tooltip(if due_on.is_some() {
                                        if reminder_enabled {
                                            "Turn off this card's reminder"
                                        } else {
                                            "Notify me when this card is due"
                                        }
                                    } else {
                                        "Choose a due date first"
                                    })
                                    .on_click(move |checked, _, cx| {
                                        reminder_view.update(cx, |this, cx| {
                                            this.set_selected_entry_reminder(*checked, cx);
                                        });
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                h_flex()
                                    .min_w_0()
                                    .items_center()
                                    .gap_1p5()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(notification_color)
                                    .child(
                                        Icon::new(notification_icon)
                                            .xsmall()
                                            .text_color(notification_color),
                                    )
                                    .child(notification_label),
                            )
                            .when(
                                notification_availability
                                    == crate::notifications::NotificationAvailability::Enabled,
                                |this| {
                                    let notifications = notifications.clone();
                                    this.child(
                                        Button::new("test-card-notification")
                                            .label("Test")
                                            .ghost()
                                            .xsmall()
                                            .tooltip("Send a test system notification now")
                                            .on_click(cx.listener(move |_, _, window, cx| {
                                                match notifications.show_test_notification() {
                                                    Ok(()) => window.push_notification(
                                                        Notification::success(
                                                            "Test sent. Check Windows notifications if it did not pop up.",
                                                        ),
                                                        cx,
                                                    ),
                                                    Err(error) => window.push_notification(
                                                        Notification::error(format!(
                                                            "Could not send test notification: {error}"
                                                        )),
                                                        cx,
                                                    ),
                                                }
                                            })),
                                    )
                                },
                            )
                            .when(notification_availability.can_open_settings(), |this| {
                                this.child(
                                    Button::new("open-system-notification-settings")
                                        .label("Open settings")
                                        .ghost()
                                        .xsmall()
                                        .on_click(|_, _, cx| {
                                            cx.open_url("ms-settings:notifications");
                                        }),
                                )
                            }),
                    )
                    .when(
                        notification_availability
                            != crate::notifications::NotificationAvailability::Enabled,
                        |this| {
                            this.child(
                                div()
                                    .text_xs()
                                    .line_height(relative(1.35))
                                    .text_color(cx.theme().muted_foreground)
                                    .child(notification_help),
                            )
                        },
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::entry_metadata_sections;
    use gpui_kit::{
        Context, IntoElement, Render, Styled, TestAppContext, VisualTestContext, Window, div, px,
        size,
    };

    struct EntryMetadataSectionsTest;

    impl Render for EntryMetadataSectionsTest {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            entry_metadata_sections(div().h(px(80.)), div().h(px(180.)), cx)
        }
    }

    #[gpui_kit::test]
    fn metadata_sections_share_the_same_row_height(cx: &mut TestAppContext) {
        cx.update(gpui_kit::component::init);
        let (_, cx) = cx.add_window_view(|_, _| EntryMetadataSectionsTest);
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(800.), px(400.)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        let Some(labels_bounds) = cx.debug_bounds("entry-metadata-labels") else {
            panic!("labels metadata section should be rendered");
        };
        let Some(due_date_bounds) = cx.debug_bounds("entry-metadata-due-date") else {
            panic!("due date metadata section should be rendered");
        };

        assert_eq!(labels_bounds.size.height, due_date_bounds.size.height);
    }
}
