use super::*;

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
                                    .w(px(320.))
                                    .flex_shrink_0()
                                    .gap_2()
                                    .child(
                                        Button::new("home-new-note")
                                            .flex_1()
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
                                            .flex_1()
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

    pub(crate) fn render_today(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
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
            return inline_retry(error, cx.listener(|this, _, _, cx| this.load_home(cx)), cx)
                .into_any_element();
        }
        if self.home.data.today.is_empty() {
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
                self.home
                    .data
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

    pub(crate) fn render_home_items(
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
}
