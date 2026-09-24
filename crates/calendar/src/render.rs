use super::*;
use gpui_kit::component::date_picker::DatePicker;

impl CalendarWorkspace {
    pub(super) fn render_calendar_grid(
        &self,
        month: NaiveDate,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let weekday_headers = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let first_weekday = month.weekday().num_days_from_monday() as i64;
        let today = Local::now().date_naive();
        let cells = (0..42).map(|index| {
            let day_offset = index as i64 - first_weekday;
            let day = month + Duration::days(day_offset);
            let in_month = day.month() == month.month();
            let is_today = day == today;
            let date_text = if in_month {
                day.day().to_string()
            } else {
                format!("{} {}", day.format("%b"), day.day())
            };
            let mut day_entries = if in_month {
                self.state
                    .entries
                    .iter()
                    .filter(|entry| entry.due_on == day.to_string())
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            day_entries.sort_by(|left, right| {
                let rank = |entry: &CalendarEntryRecord| match calendar_entry_lifecycle(entry) {
                    EntryLifecycleState::Open => 0,
                    EntryLifecycleState::Completed => 1,
                    EntryLifecycleState::Cancelled => 2,
                };
                rank(left)
                    .cmp(&rank(right))
                    .then_with(|| left.title.cmp(&right.title))
            });
            let mut cell = v_flex()
                .id(SharedString::from(format!("calendar-day-{day}")))
                .flex_1()
                .min_h(px(96.))
                .min_w_0()
                .gap_1()
                .p_2()
                .border_1()
                .border_color(if is_today {
                    theme.primary.opacity(0.82)
                } else {
                    theme.border.opacity(if in_month { 0.68 } else { 0.28 })
                })
                .bg(if is_today {
                    theme.primary.opacity(0.07)
                } else if in_month {
                    theme.background
                } else {
                    theme.background.opacity(0.36)
                })
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if is_today || in_month {
                                    theme.foreground
                                } else {
                                    theme.muted_foreground.opacity(0.58)
                                })
                                .child(date_text),
                        )
                        .when(is_today, |this| {
                            this.child(
                                div()
                                    .px_1()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .bg(theme.primary.opacity(0.16))
                                    .text_color(theme.primary)
                                    .child("Today"),
                            )
                        }),
                )
                .drag_over::<CalendarEntryDrag>(|this, _, _, cx| {
                    this.border_2()
                        .border_color(cx.theme().primary)
                        .bg(cx.theme().drop_target)
                })
                .on_drop(
                    cx.listener(move |this, info: &CalendarEntryDrag, window, cx| {
                        this.reschedule_calendar_entry(info.entry_id, day.to_string(), window, cx);
                    }),
                );
            for entry in day_entries {
                let entry_id = entry.entry_id;
                let selected = self.state.selected_entry_id == Some(entry_id);
                let accent = match entry.workflow_role {
                    CalendarListRole::Neutral => theme.primary,
                    CalendarListRole::Done => theme.success,
                    CalendarListRole::Cancelled => theme.danger,
                };
                let label = if entry.occurrence_key.is_some() {
                    format!("↻ {}", entry.title)
                } else {
                    entry.title.clone()
                };
                let can_drag = entry.occurrence_key.is_none();
                let mut entry_view = div()
                    .id(SharedString::from(format!(
                        "calendar-entry-{}-{}",
                        entry.entry_id, day
                    )))
                    .debug_selector(|| format!("calendar-entry-{entry_id}-{day}"))
                    .w_full()
                    .px_1()
                    .py_0p5()
                    .rounded_sm()
                    .cursor_pointer()
                    .bg(accent.opacity(if selected { 0.28 } else { 0.12 }))
                    .text_xs()
                    .text_color(accent)
                    .hover(|this| this.bg(accent.opacity(0.24)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_calendar_entry(entry_id, window, cx);
                    }))
                    .child(label);
                if can_drag {
                    entry_view = entry_view.on_drag(
                        CalendarEntryDrag {
                            entry_id: entry.entry_id,
                            title: entry.title.clone().into(),
                        },
                        |drag, _, _, cx| cx.new(|_| drag.clone()),
                    );
                }
                cell = cell.child(entry_view);
            }
            cell.into_any_element()
        });
        let weeks = (0..6).map(|week| {
            h_flex()
                .flex_1()
                .min_h_0()
                .children(cells.clone().skip(week * 7).take(7))
                .into_any_element()
        });
        v_flex()
            .w_full()
            .flex_1()
            .min_h(px(616.))
            .gap_1()
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .children(weekday_headers.into_iter().map(|weekday| {
                        div()
                            .flex_1()
                            .px_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(weekday)
                            .into_any_element()
                    })),
            )
            .children(weeks)
            .into_any_element()
    }

    pub(super) fn render_calendar_recurrence_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let has_entries = !self.calendar_entry_options().is_empty();
        let recurrence_picker = Select::new(&self.state.recurrence_entry_select)
            .id("calendar-recurrence-entry-picker")
            .disabled(self.state.creating)
            .placeholder(if has_entries {
                "Choose an entry"
            } else {
                "No entries available"
            })
            .accessibility_label("Entry to repeat")
            .search_placeholder("Search entries by title or list")
            .menu_max_h(px(240.))
            .disabled(!has_entries)
            .w_full();

        v_flex()
            .id("calendar-add-recurring-section")
            .debug_selector(|| "calendar-add-recurring-section".into())
            .gap_3()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(theme.border.opacity(0.72))
            .bg(theme.muted.opacity(0.12))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child("Entry"),
                    )
                    .child(
                        div()
                            .debug_selector(|| "calendar-recurrence-entry-picker".into())
                            .child(recurrence_picker),
                    ),
            )
            .when_some(self.state.recurrence_error.clone(), |this, error| {
                this.child(
                    div()
                        .debug_selector(|| "calendar-recurrence-error".into())
                        .px_3()
                        .py_2()
                        .rounded_sm()
                        .border_1()
                        .border_color(theme.danger.opacity(0.48))
                        .bg(theme.danger.opacity(0.08))
                        .text_xs()
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .when(!has_entries, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.warning)
                        .child("Add an entry to this board before creating a recurring task."),
                )
            })
            .child(
                h_flex()
                    .items_start()
                    .gap_3()
                    .when(self.route_narrow(CalendarRoute::Recurring), |this| {
                        this.flex_col()
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .w_full()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.muted_foreground)
                                    .child("First occurrence"),
                            )
                            .child(
                                DatePicker::new(&self.state.recurrence_start_picker)
                                    .placeholder("No date selected")
                                    .cleanable(true)
                                    .number_of_months(1)
                                    .disabled(self.state.creating)
                                    .w_full(),
                            ),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .w_full()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.muted_foreground)
                                    .child("Until (optional)"),
                            )
                            .child(
                                DatePicker::new(&self.state.recurrence_until_picker)
                                    .placeholder("No end date")
                                    .cleanable(true)
                                    .number_of_months(1)
                                    .disabled(self.state.creating)
                                    .w_full(),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child("Repeat rule"),
                    )
                    .child(
                        Input::new(&self.state.recurrence_rule_input)
                            .disabled(self.state.creating)
                            .w_full(),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "Examples: daily, every 2 weeks on mon and wed, monthly on day 15.",
                    )),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child("Create the next copy"),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                Button::new("recurrence-mode-completion")
                                    .disabled(self.state.creating)
                                    .label("After completion")
                                    .small()
                                    .outline()
                                    .selected(!self.state.recurrence_on_schedule)
                                    .toggled(!self.state.recurrence_on_schedule)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_recurrence_generation_mode(false, cx);
                                    })),
                            )
                            .child(
                                Button::new("recurrence-mode-schedule")
                                    .disabled(self.state.creating)
                                    .label("On schedule")
                                    .small()
                                    .outline()
                                    .selected(self.state.recurrence_on_schedule)
                                    .toggled(self.state.recurrence_on_schedule)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_recurrence_generation_mode(true, cx);
                                    })),
                            ),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        if self.state.recurrence_on_schedule {
                            "Castle creates each copy when its scheduled date arrives."
                        } else {
                            "Castle waits until the current copy is completed."
                        },
                    )),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .pt_3()
                    .border_t_1()
                    .border_color(theme.border.opacity(0.72))
                    .child(
                        Button::new("discard-recurrence-form")
                            .debug_selector(|| "discard-recurrence-form".into())
                            .label("Discard")
                            .ghost()
                            .small()
                            .disabled(self.state.creating)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.clear_recurrence_form(window, cx);
                            })),
                    )
                    .child(
                        Button::new("create-recurring-task")
                            .icon(IconName::Plus)
                            .label(if self.state.creating {
                                "Creating..."
                            } else {
                                "Create recurrence"
                            })
                            .primary()
                            .small()
                            .disabled(self.state.creating || !has_entries)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_recurring_task_from_form(window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_calendar_detail_header(
        &self,
        entry_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Item details"),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        div()
                            .debug_selector(|| "calendar-repeat-entry".into())
                            .child(
                                Button::new("calendar-repeat-entry")
                                    .label("Repeat")
                                    .outline()
                                    .small()
                                    .tooltip("Create a recurring schedule for this item")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_recurrence_for_entry(entry_id, window, cx);
                                    })),
                            ),
                    )
                    .child(
                        Button::new("calendar-close-entry-details")
                            .icon(IconName::Close)
                            .ghost()
                            .compact()
                            .tooltip("Close item details")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_calendar_entry(cx);
                                if this.route_narrow(CalendarRoute::Month) {
                                    cx.emit(CalendarWorkspaceEvent::Back);
                                }
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_calendar_lifecycle_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let status = self
            .selected_calendar_entry()
            .map(calendar_entry_lifecycle)
            .unwrap_or(EntryLifecycleState::Open);
        h_flex()
            .gap_1()
            .child(
                Button::new("calendar-entry-status-open")
                    .label("Open")
                    .outline()
                    .small()
                    .selected(status == EntryLifecycleState::Open)
                    .disabled(self.selected_saving())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_calendar_entry_lifecycle(EntryLifecycleState::Open, window, cx);
                    })),
            )
            .child(
                Button::new("calendar-entry-status-done")
                    .label("Done")
                    .outline()
                    .small()
                    .selected(status == EntryLifecycleState::Completed)
                    .disabled(self.selected_saving())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_calendar_entry_lifecycle(
                            EntryLifecycleState::Completed,
                            window,
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("calendar-entry-status-cancelled")
                    .label("Cancelled")
                    .outline()
                    .small()
                    .selected(status == EntryLifecycleState::Cancelled)
                    .disabled(self.selected_saving())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_calendar_entry_lifecycle(
                            EntryLifecycleState::Cancelled,
                            window,
                            cx,
                        );
                    })),
            )
            .into_any_element()
    }

    pub(super) fn render_calendar_entry_details(
        &self,
        entry: &CalendarEntryRecord,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let status = self
            .drafts
            .get(&entry.entry_id)
            .map(|draft| draft.status)
            .unwrap_or_else(|| calendar_entry_lifecycle(entry));
        let status_label = calendar_entry_lifecycle_label(status);
        let status_color = match status {
            EntryLifecycleState::Open => theme.primary,
            EntryLifecycleState::Completed => theme.success,
            EntryLifecycleState::Cancelled => theme.danger,
        };
        let entry_id = entry.entry_id;
        let occurrence_note = entry
            .occurrence_key
            .as_ref()
            .map(|_| "Recurring occurrence · edits apply to the source card");

        v_flex()
            .id("calendar-entry-details")
            .debug_selector(|| "calendar-entry-details".into())
            .gap_2()
            .pb_3()
            .border_b_1()
            .border_color(theme.border.opacity(0.72))
            .child(self.render_calendar_detail_header(entry_id, cx))
            .child(self.render_draft_status(cx))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("{} · {}", entry.list_title, entry.due_on)),
            )
            .when(!entry.description.trim().is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(entry.description.clone()),
                )
            })
            .when_some(occurrence_note, |this, note| {
                this.child(div().text_xs().text_color(theme.warning).child(note))
            })
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child("Title"),
            )
            .child(Input::new(&self.state.detail_title_input).w_full())
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child("Due date"),
            )
            .child(
                DatePicker::new(&self.state.detail_due_picker)
                    .placeholder("No due date")
                    .cleanable(true)
                    .number_of_months(1)
                    .w_full(),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(status_color)
                            .child(format!("Status: {status_label}")),
                    )
                    .child(
                        Button::new("save-calendar-entry")
                            .label(if self.selected_saving() {
                                "Saving..."
                            } else {
                                "Save"
                            })
                            .primary()
                            .small()
                            .disabled(self.selected_saving())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_calendar_entry(window, cx);
                            })),
                    ),
            )
            .child(self.render_calendar_lifecycle_actions(cx))
            .when_some(self.selected_error(), |this, error| {
                this.child(div().text_xs().text_color(theme.danger).child(error))
            })
            .child(
                div()
                    .id(SharedString::from(format!(
                        "calendar-entry-detail-id-{entry_id}"
                    )))
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("Entry #{entry_id}")),
            )
            .into_any_element()
    }
}
