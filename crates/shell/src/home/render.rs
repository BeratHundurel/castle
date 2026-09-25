use super::*;
use chrono::NaiveDate;

fn planner_empty_line(copy: &'static str, cx: &mut Context<AppShell>) -> gpui_kit::AnyElement {
    div()
        .px_2()
        .py_1()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(copy)
        .into_any_element()
}

impl AppShell {
    pub(crate) fn render_home(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_project = self.workspace.active_project_id.and_then(|id| {
            self.workspace
                .projects
                .iter()
                .find(|project| project.id == id)
        });

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
                    .px_4()
                    .py_6()
                    .gap_4()
                    .child(
                        h_flex()
                            .w_full()
                            .items_end()
                            .justify_between()
                            .when(self.window_is_narrow, |this| {
                                this.flex_col().items_start()
                            })
                            .gap_4()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .child("Home"),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .flex_shrink_0()
                                    .when(self.window_is_narrow, |this| {
                                        this.w_full()
                                    })
                                    .when(!self.window_is_narrow, |this| {
                                        this.w(px(320.))
                                    })
                                    .child(
                                        Button::new("home-new-note")
                                            .flex_1()
                                            .min_w_0()
                                            .icon(IconName::Plus)
                                            .label("New note")
                                            .primary()
                                            .tooltip_with_action(
                                                "New note",
                                                &NewNoteAction,
                                                Some("AppShell"),
                                            )
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.create_note(active_project_id, window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("home-new-board")
                                            .flex_1()
                                            .min_w_0()
                                            .icon(IconName::LayoutDashboard)
                                            .label("New board")
                                            .outline()
                                            .tooltip_with_action(
                                                "New board",
                                                &NewBoardAction,
                                                Some("AppShell"),
                                            )
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.create_board(active_project_id, window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .flex_1()
                            .items_stretch()
                            .when(self.window_is_narrow, |this| this.flex_col())
                            .gap_10()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_6()
                                    .when_some(
                                        self.home.planner_action_error.clone(),
                                        |this, error| {
                                            this.child(
                                                div()
                                                    .p_2()
                                                    .rounded(cx.theme().radius)
                                                    .bg(cx.theme().danger.opacity(0.08))
                                                    .text_sm()
                                                    .text_color(cx.theme().danger)
                                                    .child(error),
                                            )
                                        },
                                    )
                                    .child(
                                        v_flex()
                                            .w_full()
                                            .gap_3()
                                            .child(section_title(
                                                "Needs attention",
                                                "Overdue and due today",
                                                cx,
                                            ))
                                            .child(self.render_today(cx)),
                                    )
                                    .child(
                                        v_flex()
                                            .w_full()
                                            .gap_3()
                                            .child(
                                                h_flex()
                                                    .items_end()
                                                    .justify_between()
                                                    .child(
                                                        v_flex()
                                                            .gap_1()
                                                            .child(
                                                                div()
                                                                    .text_lg()
                                                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                                                    .child("Next 7 days"),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(cx.theme().muted_foreground)
                                                                    .child("Scheduled from tomorrow"),
                                                            ),
                                                    )
                                                    .child(
                                                        Button::new("home-view-calendar")
                                                            .label("View calendar")
                                                            .ghost()
                                                            .small()
                                                            .on_click(cx.listener(|this, _, window, cx| {
                                                                this.open_workspace_calendar(window, cx);
                                                            })),
                                                    ),
                                            )
                                            .child(self.render_upcoming(cx)),
                                    )
                                    .child(
                                        v_flex()
                                            .gap_3()
                                            .child(section_title("Unscheduled", "Open tasks without a date", cx))
                                            .child(self.render_unscheduled(cx)),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .when(self.window_is_narrow, |this| this.w_full())
                                    .when(!self.window_is_narrow, |this| {
                                        this.w(px(320.)).flex_shrink_0()
                                    })
                                    .gap_6()
                                    .child(
                                        v_flex()
                                            .gap_3()
                                            .child(section_title("Pinned", "Keep close", cx))
                                            .child(self.render_home_items(
                                                "home-pinned",
                                                &self.home.data.pinned,
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
                                                &self.home.data.recent,
                                                "Open a note or board and it will appear here.",
                                                cx,
                                            )),
                                    ),
                            ),
                    ),
            )
    }

    pub(crate) fn render_today(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        if self.home.phase.is_loading() && !self.home.phase.has_content() {
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

        if let Some(error) = self.home.phase.error() {
            return inline_retry(
                error,
                cx.listener(|this, _, window, cx| this.load_home(window, cx)),
                cx,
            )
            .into_any_element();
        }

        if self.home.data.today.is_empty() {
            return planner_empty_line("Nothing needs attention today.", cx);
        }

        self.render_planner_section(
            "home-today-tasks",
            &self.home.data.today,
            self.home.data.today_total,
            PlannerTaskGroup::Today,
            cx,
        )
    }

    pub(crate) fn render_upcoming(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        if self.home.data.upcoming.is_empty() {
            return planner_empty_line("Nothing scheduled in the next seven days.", cx);
        }

        self.render_planner_section(
            "home-upcoming-tasks",
            &self.home.data.upcoming,
            self.home.data.upcoming_total,
            PlannerTaskGroup::Upcoming,
            cx,
        )
    }

    pub(crate) fn render_unscheduled(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        if self.home.data.unscheduled.is_empty() {
            return planner_empty_line("No open tasks without a date.", cx);
        }

        self.render_planner_section(
            "home-unscheduled-tasks",
            &self.home.data.unscheduled,
            self.home.data.unscheduled_total,
            PlannerTaskGroup::Unscheduled,
            cx,
        )
    }

    fn render_planner_section(
        &self,
        id: &'static str,
        tasks: &[PlannerTask],
        total: usize,
        group: PlannerTaskGroup,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let list = self.render_planner_tasks(id, tasks, cx);
        let remaining = total.saturating_sub(tasks.len());
        if remaining == 0 {
            return list;
        }

        let action_id: u32 = match group {
            PlannerTaskGroup::Today => 0,
            PlannerTaskGroup::Upcoming => 1,
            PlannerTaskGroup::Unscheduled => 2,
        };

        let label = if self.home.loading_more_group == Some(group) {
            "Loading…".to_string()
        } else {
            format!(
                "Show {} more",
                remaining.min(storage::workspace::home::HOME_TASK_PAGE_SIZE)
            )
        };

        v_flex()
            .id((id, action_id))
            .gap_1()
            .child(list)
            .child(
                Button::new(("home-planner-load-more", action_id))
                    .label(label)
                    .ghost()
                    .small()
                    .disabled(self.home.loading_more_group.is_some())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.load_more_planner_tasks(group, window, cx);
                    })),
            )
            .into_any_element()
    }

    fn render_planner_tasks(
        &self,
        id: &'static str,
        tasks: &[PlannerTask],
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let today = Local::now().date_naive().to_string();
        v_flex()
            .id(id)
            .gap_0()
            .children(tasks.iter().cloned().enumerate().map(|(index, task)| {
                let task_id = task.entry_id;
                let overdue = task
                    .due_on
                    .as_deref()
                    .is_some_and(|due_on| due_on < today.as_str());

                let due_label = match task.due_on.as_deref() {
                    Some(_) if overdue => "Overdue".to_string(),
                    Some(due_on) if due_on == today => "Today".to_string(),
                    Some(due_on) => NaiveDate::parse_from_str(due_on, "%Y-%m-%d")
                        .map(|date| date.format("%a, %b %-d").to_string())
                        .unwrap_or_else(|_| due_on.to_string()),
                    None => "No date".to_string(),
                };

                let due_color = if overdue {
                    cx.theme().danger
                } else if task.due_on.is_none() {
                    cx.theme().muted_foreground
                } else if task.due_on.as_deref() == Some(today.as_str()) {
                    cx.theme().warning
                } else {
                    cx.theme().info
                };

                let breadcrumb = format!("{} · {}", task.board_title, task.list_title);
                let row_group = format!("home-planner-task-{task_id}");
                let due_control = if let Some(picker) = self.home.planner_calendars.get(&task_id) {
                    let calendar = picker.state.clone();
                    let date_picker_open = self.home.open_planner_date_picker == Some(task_id);
                    let trigger = Button::new(("home-reschedule-task", task_id as usize))
                        .label(due_label.clone())
                        .ghost()
                        .small()
                        .text_color(due_color)
                        .disabled(self.home.rescheduling)
                        .tooltip(if task.due_on.is_some() {
                            "Reschedule task"
                        } else {
                            "Set a due date"
                        });

                    div()
                        .id(("home-reschedule-control", task_id as usize))
                        .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                        .child(
                            Popover::new(("home-reschedule-picker", task_id as usize))
                                .anchor(gpui_kit::Anchor::TopRight)
                                .open(date_picker_open)
                                .on_open_change(cx.listener(move |this, open: &bool, _, cx| {
                                    this.home.open_planner_date_picker = if *open {
                                        Some(task_id)
                                    } else if this.home.open_planner_date_picker == Some(task_id) {
                                        None
                                    } else {
                                        this.home.open_planner_date_picker
                                    };
                                    cx.notify();
                                }))
                                .trigger(trigger)
                                .content(move |_, _, _| {
                                    Calendar::new(&calendar)
                                        .number_of_months(1)
                                        .border_0()
                                        .rounded_none()
                                        .p_0()
                                }),
                        )
                        .into_any_element()
                } else {
                    div()
                        .text_xs()
                        .text_color(due_color)
                        .child(due_label)
                        .into_any_element()
                };

                let task_view = v_flex().gap_1().child(
                    h_flex()
                        .id((id, index))
                        .group(row_group.clone())
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_1()
                        .px_1()
                        .py_2()
                        .rounded(cx.theme().radius)
                        .hover(|this| this.bg(cx.theme().secondary_hover.opacity(0.48)))
                        .child(
                            Button::new(("home-complete-task", task_id as usize))
                                .icon(IconName::CircleCheck)
                                .ghost()
                                .compact()
                                .when(!self.window_is_narrow, |this| {
                                    this.invisible()
                                        .focus_visible(|this| this.visible())
                                        .group_hover(row_group.clone(), |this| this.visible())
                                })
                                .tooltip("Complete task")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.complete_planner_task(task_id, window, cx);
                                })),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_sm()
                                        .font_weight(gpui_kit::FontWeight::MEDIUM)
                                        .child(task.title.clone()),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(breadcrumb),
                                ),
                        )
                        .child(due_control)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_planner_task(task.clone(), window, cx);
                        })),
                );
                task_view.into_any_element()
            }))
            .into_any_element()
    }

    pub(crate) fn render_home_items(
        &self,
        id: &'static str,
        items: &[WorkspaceHomeItem],
        empty_copy: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        if items.is_empty() {
            return div()
                .id(id)
                .px_2()
                .py_1()
                .text_xs()
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
}
