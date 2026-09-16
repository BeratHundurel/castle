use super::*;
use gpui_kit::{IntoElement, Render, canvas};

const CALENDAR_RAIL_WIDTH: f32 = 336.;

pub struct CalendarPage {
    workspace: Entity<CalendarWorkspace>,
    route: CalendarRoute,
    focus: FocusHandle,
}

impl CalendarPage {
    pub fn new(
        workspace: Entity<CalendarWorkspace>,
        route: CalendarRoute,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&workspace, |_, _, cx| cx.notify()).detach();
        Self {
            workspace,
            route,
            focus: cx.focus_handle(),
        }
    }
}

impl Focusable for CalendarPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for CalendarPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.downgrade();
        let route = self.route;
        let content = self
            .workspace
            .update(cx, |model, cx| model.render_page(route, cx));
        div()
            .id("calendar-page")
            .debug_selector(move || format!("calendar-page-{route:?}"))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &gpui_kit::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && this.route == CalendarRoute::Month {
                    this.workspace.update(cx, |model, cx| {
                        model.state.selected_entry_id = None;
                        cx.notify();
                    });
                }
            }))
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(content)
            .child(
                canvas(
                    move |bounds, window, cx| {
                        workspace
                            .update(cx, |model, cx| {
                                let narrow = bounds.size.width < px(1120.);
                                let was_narrow = model.route_narrow(route);
                                if was_narrow != narrow {
                                    if !was_narrow
                                        && narrow
                                        && route == CalendarRoute::Month
                                        && model.state.selected_entry_id.is_some()
                                    {
                                        cx.defer_in(window, |_, _, cx| {
                                            cx.emit(CalendarWorkspaceEvent::Navigate(
                                                CalendarRoute::Item,
                                            ))
                                        });
                                    }
                                    model.set_route_narrow(route, narrow);
                                    cx.notify();
                                }
                            })
                            .ok();
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
}

impl CalendarWorkspace {
    fn render_page(&self, route: CalendarRoute, cx: &mut Context<Self>) -> AnyElement {
        let body = match route {
            CalendarRoute::Month => self.render_month(cx),
            CalendarRoute::Item => self.render_inspector(true, cx),
            CalendarRoute::Recurring => self.render_recurring(cx),
        };
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_header(route, cx))
            .when_some(self.state.error.clone(), |this, error| {
                this.child(
                    div()
                        .px_4()
                        .py_2()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .child(body)
            .into_any_element()
    }

    fn render_header(&self, route: CalendarRoute, cx: &mut Context<Self>) -> AnyElement {
        let title = match route {
            CalendarRoute::Month => "Calendar",
            CalendarRoute::Item => "Item details",
            CalendarRoute::Recurring => "Recurring tasks",
        };
        h_flex()
            .id("calendar-header")
            .debug_selector(|| "calendar-header".into())
            .h(px(52.))
            .flex_shrink_0()
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("calendar-back")
                    .label("Board")
                    .child(IconName::ChevronRight)
                    .ghost()
                    .small()
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(CalendarWorkspaceEvent::Board))),
            )
            .when(route == CalendarRoute::Recurring, |this| {
                this.child(
                    Button::new("back-to-calendar-from-recurring")
                        .label("Calendar")
                        .child(IconName::ChevronRight)
                        .ghost()
                        .small()
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(CalendarWorkspaceEvent::Navigate(CalendarRoute::Month));
                        })),
                )
            })
            .child(
                div()
                    .debug_selector(|| "calendar-page-title".into())
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(div().flex_1())
            .when(route == CalendarRoute::Month, |this| {
                this.child(
                    h_flex()
                        .gap_1()
                        .px_1()
                        .py_0p5()
                        .bg(cx.theme().secondary)
                        .rounded(cx.theme().radius)
                        .child(
                            Button::new("calendar-previous-month")
                                .icon(IconName::ChevronLeft)
                                .ghost()
                                .small()
                                .tooltip("Previous month")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.shift_calendar_month(-1, window, cx)
                                })),
                        )
                        .child(
                            div()
                                .debug_selector(|| "calendar-month-label".into())
                                .text_center()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(self.state.month.format("%B %Y").to_string()),
                        )
                        .child(
                            Button::new("calendar-next-month")
                                .icon(IconName::ChevronRight)
                                .ghost()
                                .small()
                                .tooltip("Next month")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.shift_calendar_month(1, window, cx)
                                })),
                        ),
                )
                .child(
                    Button::new("calendar-today")
                        .label("Today")
                        .primary()
                        .small()
                        .on_click(cx.listener(|this, _, window, cx| {
                            let today = Local::now().date_naive();
                            this.state.month = today.with_day(1).unwrap_or(today);
                            this.refresh(window, cx);
                        })),
                )
                .child(
                    Button::new("calendar-recurring")
                        .label("Recurring tasks")
                        .icon(IconName::ExternalLink)
                        .ghost()
                        .small()
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(CalendarWorkspaceEvent::Navigate(CalendarRoute::Recurring));
                        })),
                )
            })
            .when(
                route == CalendarRoute::Recurring && !self.state.recurrence_form_open,
                |this| {
                    this.child(
                        Button::new("calendar-new-recurrence")
                            .debug_selector(|| "calendar-new-recurrence".into())
                            .label("New recurring task")
                            .small()
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_recurrence_form(window, cx);
                            })),
                    )
                },
            )
            .into_any_element()
    }

    fn render_month(&self, cx: &mut Context<Self>) -> AnyElement {
        let grid = self.render_calendar_grid(self.state.month, cx);
        h_flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                div()
                    .debug_selector(|| "calendar-month-scroll-owner".into())
                    .flex_1()
                    .h_full()
                    .min_h_0()
                    .min_w_0()
                    .child(
                        v_flex()
                            .size_full()
                            .gap_4()
                            .p_4()
                            .child(grid)
                            .overflow_y_scrollbar()
                            .id("calendar-month-scroll"),
                    ),
            )
            .when(!self.route_narrow(CalendarRoute::Month), |this| {
                if self.state.selected_entry_id.is_some() {
                    this.child(self.render_inspector(false, cx))
                } else {
                    this.child(self.render_calendar_up_next(cx))
                }
            })
            .into_any_element()
    }

    fn render_calendar_up_next(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut entries = self.state.entries.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.due_on
                .cmp(&right.due_on)
                .then_with(|| left.title.cmp(&right.title))
        });
        let rows = entries.iter().copied().map(|entry| {
            let entry_id = entry.entry_id;
            let status = calendar_entry_lifecycle(entry);
            let status_color = match status {
                EntryLifecycleState::Open => theme.primary,
                EntryLifecycleState::Completed => theme.success,
                EntryLifecycleState::Cancelled => theme.danger,
            };
            Button::new(("calendar-agenda-entry", entry_id as u64))
                .accessibility_label(format!("Open {} due {}", entry.title, entry.due_on))
                .ghost()
                .w_full()
                .h_auto()
                .px_0()
                .py_3()
                .rounded(gpui_kit::component::button::ButtonRounded::None)
                .border_b_1()
                .border_color(theme.border.opacity(0.56))
                .child(
                    v_flex()
                        .debug_selector(move || format!("calendar-agenda-row-{entry_id}"))
                        .w_full()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .debug_selector(move || format!("calendar-agenda-title-{entry_id}"))
                                .w_full()
                                .min_w_0()
                                .truncate()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(entry.title.clone()),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .gap_2()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(entry.due_on.clone())
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(entry.list_title.clone()),
                                )
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .text_color(status_color)
                                        .child(calendar_entry_lifecycle_label(status)),
                                ),
                        ),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_calendar_entry(entry_id, window, cx);
                }))
                .into_any_element()
        });
        v_flex()
            .debug_selector(|| "calendar-up-next".into())
            .w(px(CALENDAR_RAIL_WIDTH))
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .p_4()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Upcoming"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "{} scheduled item{}",
                                entries.len(),
                                if entries.len() == 1 { "" } else { "s" }
                            )),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .px_4()
                    .children(rows)
                    .overflow_y_scrollbar()
                    .id("calendar-up-next-scroll"),
            )
            .when(entries.is_empty(), |this| {
                this.child(
                    v_flex()
                        .debug_selector(|| "calendar-upcoming-empty-guidance".into())
                        .flex_shrink_0()
                        .gap_2()
                        .p_4()
                        .border_t_1()
                        .border_color(theme.border.opacity(0.72))
                        .bg(theme.muted.opacity(0.16))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child("Bring work into the calendar"),
                        )
                        .child(div().text_xs().text_color(theme.muted_foreground).child(
                            "Set a due date on the board, or make a task return automatically.",
                        ))
                        .child(
                            Button::new("calendar-empty-open-recurring")
                                .label("Set up recurring work")
                                .outline()
                                .small()
                                .on_click(cx.listener(|_, _, _, cx| {
                                    cx.emit(CalendarWorkspaceEvent::Navigate(
                                        CalendarRoute::Recurring,
                                    ));
                                })),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_inspector(&self, full: bool, cx: &mut Context<Self>) -> AnyElement {
        let content = self
            .selected_calendar_entry()
            .map(|entry| self.render_calendar_entry_details(entry, cx));
        div()
            .debug_selector(|| "calendar-sidebar-scroll-owner".into())
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .when(full, |this| this.flex_1().w_full())
            .when(!full, |this| {
                this.w(px(CALENDAR_RAIL_WIDTH))
                    .border_l_1()
                    .border_color(cx.theme().border)
            })
            .child(
                div()
                    .id("calendar-detail-content")
                    .debug_selector(|| "calendar-detail-content".into())
                    .size_full()
                    .p_4()
                    .children(content)
                    .overflow_y_scrollbar()
                    .id("calendar-sidebar-scroll"),
            )
            .into_any_element()
    }

    fn render_recurring(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let rows = self.state.recurring.iter().map(|recurring| {
            let entry_id = recurring.entry_id;
            h_flex()
                .id(("recurring-task", entry_id as u64))
                .gap_3()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(theme.border.opacity(0.72))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .child(self.calendar_entry_label(entry_id)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(recurrence_description(recurring)),
                        ),
                )
                .child(
                    Button::new(("delete-recurrence", entry_id as u64))
                        .label("Stop repeating")
                        .ghost()
                        .small()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.delete_recurring_task(entry_id, window, cx)
                        })),
                )
                .into_any_element()
        });
        let task_count = self.state.recurring.len();
        let task_rows = v_flex()
            .debug_selector(|| "recurring-task-list".into())
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Active schedules"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "{task_count} schedule{}",
                                if task_count == 1 { "" } else { "s" }
                            )),
                    ),
            )
            .children(rows)
            .when(self.state.recurring.is_empty(), |this| {
                this.child(
                    v_flex()
                        .id("recurring-empty-state")
                        .debug_selector(|| "recurring-empty-state".into())
                        .gap_3()
                        .p_6()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border.opacity(0.72))
                        .bg(theme.muted.opacity(0.16))
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .child("No recurring schedules"),
                        )
                        .child(
                            div()
                                .debug_selector(|| "recurring-empty-description".into())
                                .max_w(px(560.))
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("Create a schedule when a board item should return automatically."),
                        )
                        .when(!self.state.recurrence_form_open, |this| {
                            this.child(
                                Button::new("recurring-empty-new-task")
                                    .debug_selector(|| "recurring-empty-new-task".into())
                                    .label("New recurring task")
                                    .outline()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_recurrence_form(window, cx);
                                    })),
                            )
                        }),
                )
            });
        let page_content = if self.state.recurrence_form_open {
            let form = v_flex()
                .debug_selector(|| "recurring-form-column".into())
                .w_full()
                .max_w(px(440.))
                .flex_shrink_0()
                .gap_3()
                .child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("New recurring task"),
                        )
                        .child(div().text_sm().text_color(theme.muted_foreground).child(
                            "Choose the item, cadence, and when Castle creates its next copy.",
                        )),
                )
                .child(self.render_calendar_recurrence_form(cx))
                .into_any_element();
            if self.route_narrow(CalendarRoute::Recurring) {
                v_flex()
                    .gap_6()
                    .child(form)
                    .child(task_rows)
                    .into_any_element()
            } else {
                h_flex()
                    .items_start()
                    .gap_6()
                    .child(div().flex_1().min_w_0().child(task_rows))
                    .child(form)
                    .into_any_element()
            }
        } else {
            v_flex()
                .gap_6()
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Build a reliable rhythm"),
                        )
                        .child(
                            div()
                                .max_w(px(680.))
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("Schedules create the next occurrence automatically, either on time or after the current item is completed."),
                        ),
                )
                .child(task_rows)
                .into_any_element()
        };
        div()
            .flex_1()
            .min_h_0()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(1120.))
                    .mx_auto()
                    .p_6()
                    .child(page_content),
            )
            .overflow_y_scrollbar()
            .id("recurring-scroll")
            .into_any_element()
    }

    pub(super) fn render_draft_status(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.selected_dirty(cx) {
                        "Unsaved changes"
                    } else {
                        "All changes saved"
                    }),
            )
            .when(self.selected_dirty(cx), |this| {
                this.child(
                    Button::new("calendar-discard")
                        .label("Discard")
                        .ghost()
                        .small()
                        .disabled(self.selected_saving())
                        .on_click(
                            cx.listener(|this, _, window, cx| this.discard_entry(window, cx)),
                        ),
                )
            })
            .into_any_element()
    }
}

fn recurrence_description(task: &RecurringTaskRecord) -> String {
    let unit = match task.rule.frequency {
        RecurrenceFrequency::Daily => "day",
        RecurrenceFrequency::Weekly => "week",
        RecurrenceFrequency::Monthly => "month",
    };
    let interval = task.rule.interval;
    let mut rule = if interval == 1 {
        format!("Every {unit}")
    } else {
        format!("Every {interval} {unit}s")
    };
    if !task.rule.weekdays.is_empty() {
        rule.push_str(&format!(
            " on {}",
            task.rule
                .weekdays
                .iter()
                .map(|day| format!("{day:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(day) = task.rule.day_of_month {
        rule.push_str(&format!(" on day {day}"));
    }
    let mode = match task.generation_mode {
        GenerationMode::OnSchedule => "on schedule",
        GenerationMode::OnCompletion => "after completion",
    };
    format!("{rule} · {mode} · next {}", task.next_on)
}
