use gpui::relative;
use gpui_component::Icon;

use super::*;

impl AppShell {
    pub(super) fn command_palette_commands(&self) -> Vec<PaletteCommand> {
        let query = self.command_palette_query.trim().to_lowercase();
        let explicit_new_command = new_command(&self.command_palette_query);
        let project_label = self
            .active_project_id
            .and_then(|id| self.projects.iter().find(|project| project.id == id))
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "Standalone".into());

        let mut commands = Vec::new();

        if let Some(command) = explicit_new_command.clone() {
            match command {
                NewCommand::Any(title) => {
                    commands.push(new_note_command(
                        self.active_project_id,
                        title.clone(),
                        project_label.clone(),
                    ));
                    commands.push(new_board_command(
                        self.active_project_id,
                        title,
                        project_label.clone(),
                    ));
                }
                NewCommand::Note(title) => {
                    commands.push(new_note_command(
                        self.active_project_id,
                        title,
                        project_label.clone(),
                    ));
                }
                NewCommand::Board(title) => {
                    commands.push(new_board_command(
                        self.active_project_id,
                        title,
                        project_label.clone(),
                    ));
                }
            }
        }

        commands.extend([
            PaletteCommand {
                label: "New tab".into(),
                subtitle: "Open an empty chooser tab".into(),
                icon: IconName::Plus,
                kind: PaletteCommandKind::NewTab,
            },
            PaletteCommand {
                label: "New note".into(),
                subtitle: SharedString::from(format!("Create in {project_label}")),
                icon: IconName::BookOpen,
                kind: PaletteCommandKind::NewNote {
                    project_id: self.active_project_id,
                    title: "Untitled note".to_string(),
                },
            },
            PaletteCommand {
                label: "New board".into(),
                subtitle: SharedString::from(format!("Create in {project_label}")),
                icon: IconName::LayoutDashboard,
                kind: PaletteCommandKind::NewBoard {
                    project_id: self.active_project_id,
                    title: "Board".to_string(),
                },
            },
            PaletteCommand {
                label: "Open note file".into(),
                subtitle: "Choose a markdown or text file".into(),
                icon: IconName::FolderOpen,
                kind: PaletteCommandKind::OpenFile,
            },
            PaletteCommand {
                label: "Switch theme".into(),
                subtitle: "Preview available themes".into(),
                icon: IconName::Palette,
                kind: PaletteCommandKind::SwitchTheme,
            },
            PaletteCommand {
                label: "Close all tabs".into(),
                subtitle: "Return to a new chooser tab".into(),
                icon: IconName::Close,
                kind: PaletteCommandKind::CloseAllTabs,
            },
        ]);

        commands.extend(self.notes.iter().map(|note| {
            let project_name = note
                .project_name
                .clone()
                .unwrap_or_else(|| "Standalone".into());

            PaletteCommand {
                label: SharedString::from(format!("Go to: {}", note.title)),
                subtitle: SharedString::from(format!("Note - {project_name}")),
                icon: IconName::BookOpen,
                kind: PaletteCommandKind::OpenNote {
                    note_id: note.id,
                    project_id: note.project_id,
                    title: note.title.clone(),
                },
            }
        }));

        commands.extend(self.boards.iter().map(|board| {
            let project_name = board
                .project_name
                .clone()
                .unwrap_or_else(|| "Standalone".into());

            PaletteCommand {
                label: SharedString::from(format!("Go to: {}", board.title)),
                subtitle: SharedString::from(format!("Board - {project_name}")),
                icon: IconName::LayoutDashboard,
                kind: PaletteCommandKind::OpenBoard {
                    board_id: board.id,
                    project_id: board.project_id,
                    title: board.title.clone(),
                },
            }
        }));

        if query.is_empty() || explicit_new_command.is_some() {
            return commands.into_iter().take(18).collect();
        }

        commands
            .into_iter()
            .filter(|command| command_matches(command, &query))
            .take(18)
            .collect()
    }

    pub(super) fn filtered_theme_names(&self, cx: &mut Context<Self>) -> Vec<SharedString> {
        let query = self.command_palette_query.trim().to_lowercase();

        ThemeRegistry::global(cx)
            .sorted_themes()
            .iter()
            .map(|theme| theme.name.clone())
            .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
            .collect()
    }

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
                        .w(px(272.))
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
                        let project_id = board.project_id;
                        let title = board.title.clone();
                        let subtitle = board
                            .project_name
                            .clone()
                            .unwrap_or_else(|| "Standalone".into());

                        Button::new(("open-board", board_id as usize))
                            .label(format!("{} - {}", title, subtitle))
                            .ghost()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_board_tab(
                                    board_id,
                                    project_id,
                                    title.clone(),
                                    window,
                                    cx,
                                );
                            }))
                    })),
            )
    }

    fn render_command_palette_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let title = match self.command_palette_mode {
            CommandPaletteMode::Commands => "Command Palette",
            CommandPaletteMode::Themes => "Switch Theme",
        };

        div()
            .id("command-palette-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_start()
            .justify_center()
            .pt_12()
            .bg(theme.transparent)
            .key_context("CommandPalette")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.close_command_palette(window, cx);
                }),
            )
            .child(
                v_flex()
                    .id("command-palette-panel")
                    .w(px(620.))
                    .max_h(px(520.))
                    .overflow_hidden()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.popover)
                    .shadow_md()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.popover_foreground)
                                    .child(title),
                            )
                            .when(
                                self.command_palette_mode == CommandPaletteMode::Themes,
                                |this| {
                                    this.child(
                                        Button::new("command-palette-back")
                                            .label("Back")
                                            .ghost()
                                            .small()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.command_palette_mode =
                                                    CommandPaletteMode::Commands;
                                                this.command_palette_query.clear();
                                                this.command_palette_selected_index = 0;
                                                this.command_palette_scroll_handle
                                                    .scroll_to_item(0);
                                                this.suppress_command_palette_event = true;
                                                this.command_palette_input.update(
                                                    cx,
                                                    |input, cx| {
                                                        input.set_value("", window, cx);
                                                        input.focus(window, cx);
                                                    },
                                                );
                                                this.suppress_command_palette_event = false;
                                                cx.notify();
                                            })),
                                    )
                                },
                            )
                            .child(
                                Button::new("command-palette-close")
                                    .icon(IconName::Close)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Close")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_command_palette(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        Input::new(&self.command_palette_input)
                            .border_0()
                            .rounded_none()
                            .bg(theme.popover)
                            .prefix(IconName::Search),
                    )
                    .child(match self.command_palette_mode {
                        CommandPaletteMode::Commands => self.render_command_results(cx),
                        CommandPaletteMode::Themes => self.render_theme_results(cx),
                    }),
            )
    }

    fn render_command_results(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let commands = self.command_palette_commands();
        let theme = cx.theme().clone();

        v_flex()
            .id("command-palette-results")
            .gap_1()
            .p_2()
            .max_h(px(420.))
            .overflow_y_scroll()
            .track_scroll(&self.command_palette_scroll_handle)
            .when_else(
                commands.is_empty(),
                |this| {
                    this.child(
                        div()
                            .px_3()
                            .py_6()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("No commands found"),
                    )
                },
                |this| {
                    this.children(commands.into_iter().enumerate().map(|(index, command)| {
                        self.render_command_row(
                            index,
                            command,
                            index == self.command_palette_selected_index,
                            cx,
                        )
                    }))
                },
            )
            .vertical_scrollbar(&self.command_palette_scroll_handle)
            .into_any_element()
    }

    fn render_theme_results(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let themes = self.filtered_theme_names(cx);
        let current_theme = cx.theme().theme_name().clone();
        let theme = cx.theme().clone();

        v_flex()
            .id("theme-palette-results")
            .gap_1()
            .p_2()
            .max_h(px(420.))
            .overflow_y_scroll()
            .track_scroll(&self.command_palette_scroll_handle)
            .when_else(
                themes.is_empty(),
                |this| {
                    this.child(
                        div()
                            .px_3()
                            .py_6()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("No themes found"),
                    )
                },
                |this| {
                    this.children(themes.into_iter().enumerate().map(|(index, theme_name)| {
                        let is_current = theme_name == current_theme;
                        self.render_theme_row(
                            index,
                            theme_name,
                            is_current,
                            index == self.command_palette_selected_index,
                            cx,
                        )
                    }))
                },
            )
            .vertical_scrollbar(&self.command_palette_scroll_handle)
            .into_any_element()
    }

    fn render_command_row(
        &self,
        index: usize,
        command: PaletteCommand,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        let label = command.label.clone();
        let subtitle = command.subtitle.clone();
        let icon = command.icon.clone();

        h_flex()
            .id(("command-row", index))
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded(theme.radius)
            .text_color(theme.popover_foreground)
            .bg(if is_selected {
                theme.accent
            } else {
                theme.popover.opacity(0.)
            })
            .hover(|this| this.bg(theme.accent))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.execute_palette_command(command.clone(), window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size_7()
                    .rounded(theme.radius)
                    .bg(theme.primary.opacity(0.14))
                    .text_color(theme.primary)
                    .child(Icon::new(icon).xsmall()),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_0()
                    .child(
                        div()
                            .text_sm()
                            .line_height(relative(1.25))
                            .text_ellipsis()
                            .overflow_hidden()
                            .child(label),
                    )
                    .child(
                        div()
                            .text_xs()
                            .line_height(relative(1.25))
                            .text_color(theme.muted_foreground)
                            .text_ellipsis()
                            .overflow_hidden()
                            .child(subtitle),
                    ),
            )
            .into_any_element()
    }

    fn render_theme_row(
        &self,
        index: usize,
        theme_name: SharedString,
        is_current: bool,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        let label = theme_name.clone();

        h_flex()
            .id(("theme-row", index))
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded(theme.radius)
            .text_color(theme.popover_foreground)
            .bg(if is_selected {
                theme.accent
            } else {
                theme.popover.opacity(0.)
            })
            .hover(|this| this.bg(theme.accent))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.apply_theme(&theme_name, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size_7()
                    .rounded(theme.radius)
                    .bg(theme.primary.opacity(0.14))
                    .text_color(theme.primary)
                    .child(Icon::new(IconName::Palette).xsmall()),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .text_sm()
                    .text_ellipsis()
                    .overflow_hidden()
                    .child(label),
            )
            .when(is_current, |this| {
                this.child(
                    Icon::new(IconName::Check)
                        .xsmall()
                        .text_color(theme.primary),
                )
            })
            .into_any_element()
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
            .on_action(cx.listener(|this, _: &ToggleSidebarAction, window, cx| {
                this.on_toggle_sidebar_action(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CommandPaletteAction, window, cx| {
                this.on_command_palette_action(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &CloseCommandPaletteAction, window, cx| {
                    this.on_close_command_palette_action(window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
                this.on_close_command_palette_action(window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &SelectPrevCommandPaletteItem, _, cx| {
                    this.select_prev_command_palette_item(cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &SelectNextCommandPaletteItem, _, cx| {
                    this.select_next_command_palette_item(cx);
                }),
            )
            .on_action(cx.listener(|this, _: &InputMoveUp, _, cx| {
                this.select_prev_command_palette_item(cx);
            }))
            .on_action(cx.listener(|this, _: &InputMoveDown, _, cx| {
                this.select_next_command_palette_item(cx);
            }))
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .id("main-container")
                    .relative()
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
                    .when(self.command_palette_open, |this| {
                        this.child(self.render_command_palette_overlay(cx))
                    })
                    .children(dialog_layer),
            )
    }
}

fn command_matches(command: &PaletteCommand, query: &str) -> bool {
    command.label.to_lowercase().contains(query) || command.subtitle.to_lowercase().contains(query)
}

#[derive(Clone)]
enum NewCommand {
    Any(String),
    Note(String),
    Board(String),
}

fn new_command(query: &str) -> Option<NewCommand> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_lowercase();
    let (title, kind): (&str, fn(String) -> NewCommand) = if lower.starts_with("new:") {
        (trimmed.get(4..)?, NewCommand::Any)
    } else if lower.starts_with("new note:") {
        (trimmed.get(9..)?, NewCommand::Note)
    } else if lower.starts_with("new board:") {
        (trimmed.get(10..)?, NewCommand::Board)
    } else {
        return None;
    };

    let title = title.trim();
    if title.is_empty() {
        None
    } else {
        Some(kind(title.to_string()))
    }
}

fn new_note_command(
    project_id: Option<u32>,
    title: String,
    project_label: SharedString,
) -> PaletteCommand {
    PaletteCommand {
        label: SharedString::from(format!("New note: {title}")),
        subtitle: SharedString::from(format!("Create in {project_label}")),
        icon: IconName::BookOpen,
        kind: PaletteCommandKind::NewNote { project_id, title },
    }
}

fn new_board_command(
    project_id: Option<u32>,
    title: String,
    project_label: SharedString,
) -> PaletteCommand {
    PaletteCommand {
        label: SharedString::from(format!("New board: {title}")),
        subtitle: SharedString::from(format!("Create in {project_label}")),
        icon: IconName::LayoutDashboard,
        kind: PaletteCommandKind::NewBoard { project_id, title },
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
