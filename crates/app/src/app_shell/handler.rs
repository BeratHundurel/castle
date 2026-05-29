use gpui::{Context, Window};
use gpui_component::{Theme, ThemeRegistry};

use super::AppShell;
use super::{CommandPaletteMode, PaletteCommand, PaletteCommandKind};

impl AppShell {
    pub(super) fn on_toggle_sidebar_action(&mut self, _: &Window, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.toggle_collapsed(cx));
    }

    pub(super) fn on_command_palette_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_command_palette(window, cx);
    }

    pub(super) fn on_close_command_palette_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_command_palette(window, cx);
    }

    pub(super) fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_palette_open = true;
        self.command_palette_mode = CommandPaletteMode::Commands;
        self.command_palette_query.clear();
        self.command_palette_selected_index = 0;
        self.command_palette_scroll_handle.scroll_to_item(0);
        self.suppress_command_palette_event = true;
        self.command_palette_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.suppress_command_palette_event = false;
        self.refresh_workspace(cx);
        cx.notify();
    }

    pub(super) fn close_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.command_palette_open {
            return;
        }

        self.command_palette_open = false;
        self.command_palette_mode = CommandPaletteMode::Commands;
        self.command_palette_query.clear();
        self.command_palette_selected_index = 0;
        self.command_palette_scroll_handle.scroll_to_item(0);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(super) fn execute_first_command_palette_match(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.command_palette_open {
            return;
        }

        match self.command_palette_mode {
            CommandPaletteMode::Commands => {
                let commands = self.command_palette_commands();
                if let Some(command) = commands
                    .get(
                        self.command_palette_selected_index
                            .min(commands.len().saturating_sub(1)),
                    )
                    .cloned()
                {
                    self.execute_palette_command(command, window, cx);
                }
            }
            CommandPaletteMode::Themes => {
                let themes = self.filtered_theme_names(cx);
                if let Some(theme_name) = themes
                    .get(
                        self.command_palette_selected_index
                            .min(themes.len().saturating_sub(1)),
                    )
                    .cloned()
                {
                    self.apply_theme(&theme_name, cx);
                }
            }
        }
    }

    pub(super) fn execute_palette_command(
        &mut self,
        command: PaletteCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command.kind {
            PaletteCommandKind::OpenNote {
                note_id,
                project_id,
                title,
            } => {
                self.close_command_palette(window, cx);
                self.open_note_tab(note_id, project_id, title, window, cx);
            }
            PaletteCommandKind::OpenBoard {
                board_id,
                project_id,
                title,
            } => {
                self.close_command_palette(window, cx);
                self.open_board_tab(board_id, project_id, title, window, cx);
            }
            PaletteCommandKind::NewNote { project_id, title } => {
                self.close_command_palette(window, cx);
                self.create_note_with_title(project_id, title, window, cx);
            }
            PaletteCommandKind::NewBoard { project_id, title } => {
                self.close_command_palette(window, cx);
                self.create_board_with_title(project_id, title, window, cx);
            }
            PaletteCommandKind::OpenFile => {
                self.close_command_palette(window, cx);
                self.open_note_file(window, cx);
            }
            PaletteCommandKind::NewTab => {
                self.close_command_palette(window, cx);
                self.new_tab(window, cx);
            }
            PaletteCommandKind::CloseAllTabs => {
                self.close_command_palette(window, cx);
                self.close_all_tabs(window, cx);
            }
            PaletteCommandKind::SwitchTheme => {
                self.command_palette_mode = CommandPaletteMode::Themes;
                self.command_palette_query.clear();
                self.command_palette_selected_index = 0;
                self.command_palette_scroll_handle.scroll_to_item(0);
                self.suppress_command_palette_event = true;
                self.command_palette_input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.focus(window, cx);
                });
                self.suppress_command_palette_event = false;
                cx.notify();
            }
        }
    }

    pub(super) fn select_prev_command_palette_item(&mut self, cx: &mut Context<Self>) {
        self.move_command_palette_selection(-1, cx);
    }

    pub(super) fn select_next_command_palette_item(&mut self, cx: &mut Context<Self>) {
        self.move_command_palette_selection(1, cx);
    }

    fn move_command_palette_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if !self.command_palette_open {
            return;
        }

        let len = match self.command_palette_mode {
            CommandPaletteMode::Commands => self.command_palette_commands().len(),
            CommandPaletteMode::Themes => self.filtered_theme_names(cx).len(),
        };

        if len == 0 {
            self.command_palette_selected_index = 0;
            cx.notify();
            return;
        }

        let current = self
            .command_palette_selected_index
            .min(len.saturating_sub(1));
        self.command_palette_selected_index = if delta.is_negative() {
            current.checked_sub(1).unwrap_or(len - 1)
        } else {
            (current + 1) % len
        };
        self.command_palette_scroll_handle
            .scroll_to_item(self.command_palette_selected_index);

        if self.command_palette_mode == CommandPaletteMode::Themes
            && let Some(theme_name) = self
                .filtered_theme_names(cx)
                .get(self.command_palette_selected_index)
                .cloned()
        {
            self.apply_theme(&theme_name, cx);
        }

        cx.notify();
    }

    pub(super) fn apply_theme(&mut self, theme_name: &gpui::SharedString, cx: &mut Context<Self>) {
        if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme_config);
            cx.refresh_windows();
            cx.notify();
        }
    }
}
