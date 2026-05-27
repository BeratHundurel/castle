use gpui_component::Icon;

use super::*;

impl AppShell {
    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let sidebar_collapsed = self.sidebar.read(cx).is_collapsed();
        let show_title_input = !matches!(
            self.open_tabs
                .get(self.active_tab_index)
                .map(|tab| &tab.kind),
            Some(OpenTabKind::Chooser) | None
        );

        TitleBar::new().border_0().bg(theme.sidebar).child(
            h_flex()
                .id("title-bar-content")
                .size_full()
                .items_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, window, _| window.prevent_default())
                .child(
                    h_flex()
                        .id("sidebar-title-bar")
                        .h_full()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .child(
                            SidebarToggleButton::new()
                                .collapsed(sidebar_collapsed)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.sidebar.update(cx, |sidebar, cx| {
                                        sidebar.toggle_collapsed(cx);
                                    });
                                    cx.notify();
                                })),
                        )
                        .when(show_title_input, |this| {
                            this.child(active_tab_icon(self.open_tabs.get(self.active_tab_index)))
                                .child(
                                    Input::new(&self.title_input)
                                        .border_0()
                                        .bg(theme.sidebar)
                                        .rounded_none()
                                        .w_full(),
                                )
                        }),
                )
                .child(self.render_tabs(cx)),
        )
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_index = self
            .active_tab_index
            .min(self.open_tabs.len().saturating_sub(1));

        let active_tab_id = self.open_tabs.get(active_index).map(|t| t.id);

        let tab_bar = TabBar::new("open-tabs")
            .pill()
            .small()
            .menu(true)
            .bg(cx.theme().sidebar)
            .selected_index(active_index)
            .on_click(cx.listener(|this, index: &usize, window, cx| {
                this.activate_tab(*index, window, cx);
            }))
            .children(self.open_tabs.iter().enumerate().map(|(index, tab)| {
                Tab::new()
                    .px_2()
                    .text_color(cx.theme().primary_foreground)
                    .label(tab_label(tab, cx))
                    .prefix(active_tab_icon(Some(tab)))
                    .suffix(
                        Button::new(("close-tab", tab.id as usize))
                            .icon(IconName::Close)
                            .when_else(index == active_index, |b| b.primary(), |b| b.ghost())
                            .xsmall()
                            .tooltip("Close tab")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_tab(index, window, cx);
                            })),
                    )
            }))
            .suffix(
                Button::new("new-tab")
                    .icon(IconName::Plus)
                    .ghost()
                    .xsmall()
                    .tooltip("New tab")
                    .on_click(cx.listener(|this, _, window, cx| this.new_tab(window, cx))),
            );

        if let Some(tab_id) = active_tab_id {
            div()
                .child(tab_bar)
                .context_menu(move |menu, _, _cx| {
                    menu.menu("Close", Box::new(CloseTabAction(tab_id)))
                        .menu("Close Others", Box::new(CloseOtherTabsAction(tab_id)))
                        .menu("Close All", Box::new(CloseAllTabsAction))
                })
                .into_any_element()
        } else {
            tab_bar.into_any_element()
        }
    }

    fn render_active_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(tab) = self.open_tabs.get(self.active_tab_index) else {
            return div().size_full().into_any_element();
        };

        match &tab.kind {
            OpenTabKind::Chooser => self.render_chooser(cx).into_any_element(),
            OpenTabKind::Board { view, .. } => view.clone().into_any_element(),
            OpenTabKind::Note { view, .. } => view.clone().into_any_element(),
        }
    }

    fn render_chooser(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_project = self
            .active_project_id
            .and_then(|id| self.projects.iter().find(|project| project.id == id));

        v_flex()
            .id("new-tab-chooser")
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .p_6()
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Create or open"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Choose a note or board."),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("new-note-active")
                            .label(match active_project {
                                Some(project) => format!("New note in {}", project.name),
                                None => "New note".to_string(),
                            })
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_note(this.active_project_id, window, cx);
                            })),
                    )
                    .child(
                        Button::new("new-note-standalone")
                            .label("Standalone note")
                            .outline()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_note(None, window, cx);
                            })),
                    )
                    .child(
                        Button::new("open-note-file")
                            .label("Open note file")
                            .outline()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_note_file(window, cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("new-board-active")
                            .label(match active_project {
                                Some(project) => format!("New board in {}", project.name),
                                None => "New board".to_string(),
                            })
                            .outline()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_board(this.active_project_id, window, cx);
                            })),
                    )
                    .child(
                        Button::new("new-board-standalone")
                            .label("Standalone board")
                            .outline()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_board(None, window, cx);
                            })),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .w(px(420.))
                    .max_h(px(220.))
                    .overflow_y_scrollbar()
                    .children(self.boards.iter().map(|board| {
                        let board_id = board.id;
                        let title = board.title.clone();
                        let subtitle = board
                            .project_name
                            .clone()
                            .unwrap_or_else(|| "Standalone".into());

                        Button::new(("open-board", board_id as usize))
                            .label(format!("{} - {}", title, subtitle))
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_board_tab(board_id, None, title.clone(), window, cx);
                            }))
                    })),
            )
    }
}

impl Focusable for AppShell {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let theme = cx.theme().clone();

        v_flex()
            .id("app-container")
            .track_focus(&self.focus_handle)
            .key_context("AppShell")
            .size_full()
            .overflow_hidden()
            .on_action(cx.listener(|this, _: &CycleNextTab, window, cx| {
                this.cycle_next_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CyclePrevTab, window, cx| {
                this.cycle_prev_tab(window, cx);
            }))
            .on_action(cx.listener(|this, action: &CloseTabAction, window, cx| {
                this.close_tab_by_id(action.0, window, cx);
            }))
            .on_action(
                cx.listener(|this, action: &CloseOtherTabsAction, window, cx| {
                    this.close_other_tabs(action.0, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &CloseAllTabsAction, window, cx| {
                this.close_all_tabs(window, cx);
            }))
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .id("main-container")
                    .size_full()
                    .overflow_hidden()
                    .rounded(theme.radius)
                    .child(self.sidebar.clone())
                    .child(
                        v_flex()
                            .id("content-container")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .min_w_0()
                                    .w_full()
                                    .overflow_hidden()
                                    .child(self.render_active_tab(cx)),
                            ),
                    )
                    .children(dialog_layer),
            )
    }
}

fn active_tab_icon(tab: Option<&OpenTab>) -> Icon {
    match tab.map(|tab| &tab.kind) {
        Some(OpenTabKind::Note { .. }) => Icon::new(IconName::BookOpen).small(),
        Some(OpenTabKind::Board { .. }) => Icon::new(IconName::LayoutDashboard).small(),
        _ => Icon::new(IconName::Plus).small(),
    }
}

fn tab_label(tab: &OpenTab, cx: &mut Context<AppShell>) -> SharedString {
    match &tab.kind {
        OpenTabKind::Note { view, .. } => {
            let state = view.read(cx).save_state();
            if matches!(
                state,
                SaveState::Dirty | SaveState::Missing | SaveState::Error(_)
            ) {
                SharedString::from(format!("* {}", tab.title))
            } else {
                tab.title.clone()
            }
        }
        _ => tab.title.clone(),
    }
}
