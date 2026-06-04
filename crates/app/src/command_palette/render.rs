use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder as _, px, relative,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _, ThemeRegistry,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Input,
    v_flex,
};

use crate::app_shell::AppShell;
use crate::command_palette::{
    CommandPaletteMode, PaletteCommand, PaletteCommandKind, SearchablePaletteCommand,
};

const COMMAND_PALETTE_RESULT_LIMIT: usize = 18;

impl AppShell {
    pub(crate) fn rebuild_command_palette_workspace_commands(&mut self) {
        let note_commands = self.notes.iter().map(|note| {
            let project_name = note
                .project_name
                .clone()
                .unwrap_or_else(|| "Standalone".into());

            searchable_command(PaletteCommand {
                label: SharedString::from(format!("Go to: {}", note.title)),
                subtitle: SharedString::from(format!("Note - {project_name}")),
                icon: IconName::BookOpen,
                kind: PaletteCommandKind::OpenNote {
                    note_id: note.id,
                    project_id: note.project_id,
                    title: note.title.clone(),
                },
            })
        });

        let board_commands = self.boards.iter().map(|board| {
            let project_name = board
                .project_name
                .clone()
                .unwrap_or_else(|| "Standalone".into());

            searchable_command(PaletteCommand {
                label: SharedString::from(format!("Go to: {}", board.title)),
                subtitle: SharedString::from(format!("Board - {project_name}")),
                icon: IconName::LayoutDashboard,
                kind: PaletteCommandKind::OpenBoard {
                    board_id: board.id,
                    project_id: board.project_id,
                    title: board.title.clone(),
                },
            })
        });

        self.command_palette.workspace_commands = note_commands.chain(board_commands).collect();
    }

    pub(crate) fn command_palette_commands(&self) -> Vec<PaletteCommand> {
        let query = self.command_palette.query.trim().to_lowercase();
        let explicit_new_command = new_command(&self.command_palette.query);
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

        if query.is_empty() || explicit_new_command.is_some() {
            let remaining = COMMAND_PALETTE_RESULT_LIMIT.saturating_sub(commands.len());
            commands.extend(
                self.command_palette
                    .workspace_commands
                    .iter()
                    .take(remaining)
                    .map(|entry| entry.command.clone()),
            );
            return commands
                .into_iter()
                .take(COMMAND_PALETTE_RESULT_LIMIT)
                .collect();
        }

        commands.retain(|command| command_matches(command, &query));
        let remaining = COMMAND_PALETTE_RESULT_LIMIT - commands.len();
        commands.extend(
            self.command_palette
                .workspace_commands
                .iter()
                .filter(|entry| entry.search_text.contains(&query))
                .take(remaining)
                .map(|entry| entry.command.clone()),
        );

        commands.truncate(COMMAND_PALETTE_RESULT_LIMIT);
        commands
    }

    pub(crate) fn filtered_theme_names(&self, cx: &mut Context<Self>) -> Vec<SharedString> {
        let query = self.command_palette.query.trim().to_lowercase();

        ThemeRegistry::global(cx)
            .sorted_themes()
            .iter()
            .map(|theme| theme.name.clone())
            .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
            .collect()
    }

    pub(crate) fn render_command_palette_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let title = match self.command_palette.mode {
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
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
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
                                self.command_palette.mode == CommandPaletteMode::Themes,
                                |this| {
                                    this.child(
                                        Button::new("command-palette-back")
                                            .label("Back")
                                            .ghost()
                                            .small()
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.command_palette.mode =
                                                    CommandPaletteMode::Commands;
                                                this.command_palette.query.clear();
                                                this.command_palette.selected_index = 0;
                                                this.command_palette
                                                    .scroll_handle
                                                    .scroll_to_item(0);
                                                this.command_palette.suppress_input_event = true;
                                                this.command_palette.input.update(
                                                    cx,
                                                    |input, cx| {
                                                        input.set_value("", window, cx);
                                                        input.focus(window, cx);
                                                    },
                                                );
                                                this.command_palette.suppress_input_event = false;
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
                        Input::new(&self.command_palette.input)
                            .border_0()
                            .rounded_none()
                            .bg(theme.popover)
                            .prefix(IconName::Search),
                    )
                    .child(match self.command_palette.mode {
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
            .relative()
            .gap_1()
            .p_2()
            .max_h(px(420.))
            .overflow_y_scroll()
            .track_scroll(&self.command_palette.scroll_handle)
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
                            index == self.command_palette.selected_index,
                            cx,
                        )
                    }))
                },
            )
            .into_any_element()
    }

    fn render_theme_results(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let themes = self.filtered_theme_names(cx);
        let current_theme = cx.theme().theme_name().clone();
        let theme = cx.theme().clone();

        v_flex()
            .id("theme-palette-results")
            .relative()
            .gap_1()
            .p_2()
            .max_h(px(420.))
            .overflow_y_scroll()
            .track_scroll(&self.command_palette.scroll_handle)
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
                            index == self.command_palette.selected_index,
                            cx,
                        )
                    }))
                },
            )
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

fn command_matches(command: &PaletteCommand, query: &str) -> bool {
    command.label.to_lowercase().contains(query) || command.subtitle.to_lowercase().contains(query)
}

fn searchable_command(command: PaletteCommand) -> SearchablePaletteCommand {
    let search_text = format!(
        "{} {}",
        command.label.to_lowercase(),
        command.subtitle.to_lowercase()
    );

    SearchablePaletteCommand {
        command,
        search_text,
    }
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
