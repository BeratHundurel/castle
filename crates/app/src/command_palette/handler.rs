use std::time::Duration;

use gpui::{Context, SharedString, Window};
use gpui_component::{Theme, ThemeRegistry};

use crate::DB;
use crate::app_shell::AppShell;
use crate::command_palette::{CommandPaletteMode, PaletteCommand, PaletteCommandKind};
use crate::search::{SearchResult, SearchResultKind};

const WORKSPACE_SEARCH_DEBOUNCE: Duration = Duration::from_millis(180);

impl AppShell {
    pub(crate) fn on_command_palette_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_command_palette(window, cx);
    }

    pub(crate) fn on_close_command_palette_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_command_palette(window, cx);
    }

    pub(crate) fn open_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_palette.open = true;
        self.command_palette.mode = CommandPaletteMode::Commands;
        self.command_palette.query.clear();
        self.command_palette.search_generation =
            self.command_palette.search_generation.saturating_add(1);
        self.command_palette.search_debounce_task = None;
        self.command_palette.selected_index = 0;
        self.command_palette.scroll_handle.scroll_to_item(0);
        self.command_palette.suppress_input_event = true;
        self.command_palette.input.update(cx, |input, cx| {
            input.set_placeholder("Type a command", window, cx);
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.command_palette.suppress_input_event = false;
        self.refresh_workspace(cx);
        cx.notify();
    }

    pub(crate) fn open_workspace_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_palette.open = true;
        self.command_palette.mode = CommandPaletteMode::Search;
        self.command_palette.query.clear();
        self.command_palette.selected_index = 0;
        self.command_palette.search_generation =
            self.command_palette.search_generation.saturating_add(1);
        self.command_palette.search_debounce_task = None;
        self.command_palette.search_results.clear();
        self.command_palette.search_loading = true;
        self.command_palette.search_error = None;
        self.command_palette.scroll_handle.scroll_to_item(0);
        self.command_palette.suppress_input_event = true;
        self.command_palette.input.update(cx, |input, cx| {
            input.set_placeholder("Search notes, boards, cards, and entries", window, cx);
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.command_palette.suppress_input_event = false;
        self.rebuild_workspace_search_index(cx);
        cx.notify();
    }

    pub(crate) fn close_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.command_palette.open {
            return;
        }

        self.command_palette.open = false;
        self.command_palette.mode = CommandPaletteMode::Commands;
        self.command_palette.query.clear();
        self.command_palette.search_generation =
            self.command_palette.search_generation.saturating_add(1);
        self.command_palette.search_debounce_task = None;
        self.command_palette.search_results.clear();
        self.command_palette.search_loading = false;
        self.command_palette.search_error = None;
        self.command_palette.selected_index = 0;
        self.command_palette.scroll_handle.scroll_to_item(0);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn execute_first_command_palette_match(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.command_palette.open {
            return;
        }

        match self.command_palette.mode {
            CommandPaletteMode::Commands => {
                let commands = self.command_palette_commands();
                if let Some(command) = commands
                    .get(
                        self.command_palette
                            .selected_index
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
                        self.command_palette
                            .selected_index
                            .min(themes.len().saturating_sub(1)),
                    )
                    .cloned()
                {
                    self.apply_theme(&theme_name, cx);
                }
            }
            CommandPaletteMode::Search => {
                if let Some(result) = self
                    .command_palette
                    .search_results
                    .get(
                        self.command_palette
                            .selected_index
                            .min(self.command_palette.search_results.len().saturating_sub(1)),
                    )
                    .cloned()
                {
                    self.open_search_result(result, window, cx);
                }
            }
        }
    }

    pub(crate) fn execute_palette_command(
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
                self.command_palette.mode = CommandPaletteMode::Themes;
                self.command_palette.query.clear();
                self.command_palette.search_generation =
                    self.command_palette.search_generation.saturating_add(1);
                self.command_palette.search_debounce_task = None;
                self.command_palette.selected_index = 0;
                self.command_palette.scroll_handle.scroll_to_item(0);
                self.command_palette.suppress_input_event = true;
                self.command_palette.input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.focus(window, cx);
                });
                self.command_palette.suppress_input_event = false;
                cx.notify();
            }
            PaletteCommandKind::SearchWorkspace => {
                self.open_workspace_search(window, cx);
            }
        }
    }

    pub(crate) fn select_prev_command_palette_item(&mut self, cx: &mut Context<Self>) {
        self.move_command_palette_selection(-1, cx);
    }

    pub(crate) fn select_next_command_palette_item(&mut self, cx: &mut Context<Self>) {
        self.move_command_palette_selection(1, cx);
    }

    pub(crate) fn run_workspace_search(&mut self, cx: &mut Context<Self>) {
        let query = self.command_palette.query.trim().to_string();

        self.command_palette.search_generation =
            self.command_palette.search_generation.saturating_add(1);

        let generation = self.command_palette.search_generation;

        self.command_palette.search_debounce_task = None;
        self.command_palette.search_loading = true;
        self.command_palette.search_error = None;

        let db = cx.global::<DB>().conn.clone();
        self.command_palette.search_debounce_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(WORKSPACE_SEARCH_DEBOUNCE)
                .await;

            let result = crate::search::search_workspace(db.as_ref(), &query, 20).await;

            this.update(cx, |this, cx| {
                if this.command_palette.mode != CommandPaletteMode::Search
                    || this.command_palette.search_generation != generation
                {
                    return;
                }

                this.command_palette.search_debounce_task = None;
                this.command_palette.search_loading = false;
                match result {
                    Ok(results) => {
                        this.command_palette.search_results = results;
                        this.command_palette.search_error = None;
                    }
                    Err(err) => {
                        this.command_palette.search_results.clear();
                        this.command_palette.search_error =
                            Some(SharedString::from(format!("Search failed: {err}")));
                    }
                }

                cx.notify();
            })
            .ok();
        }));
    }

    fn rebuild_workspace_search_index(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| {
            let result = crate::search::rebuild_search_index(db.as_ref()).await;

            this.update(cx, |this, cx| {
                if this.command_palette.mode != CommandPaletteMode::Search {
                    return;
                }

                match result {
                    Ok(()) => {
                        this.command_palette.search_error = None;
                        this.run_workspace_search(cx);
                    }
                    Err(err) => {
                        this.command_palette.search_loading = false;
                        this.command_palette.search_results.clear();
                        this.command_palette.search_error =
                            Some(SharedString::from(format!("Search index failed: {err}")));
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn move_command_palette_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if !self.command_palette.open {
            return;
        }

        let len = match self.command_palette.mode {
            CommandPaletteMode::Commands => self.command_palette_commands().len(),
            CommandPaletteMode::Themes => self.filtered_theme_names(cx).len(),
            CommandPaletteMode::Search => self.command_palette.search_results.len(),
        };

        if len == 0 {
            self.command_palette.selected_index = 0;
            cx.notify();
            return;
        }

        let current = self
            .command_palette
            .selected_index
            .min(len.saturating_sub(1));

        self.command_palette.selected_index = if delta.is_negative() {
            current.checked_sub(1).unwrap_or(len - 1)
        } else {
            (current + 1) % len
        };

        self.command_palette
            .scroll_handle
            .scroll_to_item(self.command_palette.selected_index);

        if self.command_palette.mode == CommandPaletteMode::Themes
            && let Some(theme_name) = self
                .filtered_theme_names(cx)
                .get(self.command_palette.selected_index)
                .cloned()
        {
            self.apply_theme(&theme_name, cx);
        }

        cx.notify();
    }

    pub(crate) fn open_search_result(
        &mut self,
        result: SearchResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_command_palette(window, cx);

        match result.kind {
            SearchResultKind::Note => {
                self.open_note_tab(
                    result.open_id,
                    result.project_id,
                    SharedString::from(result.title),
                    window,
                    cx,
                );
            }
            SearchResultKind::Board | SearchResultKind::Card | SearchResultKind::Entry => {
                let title = self
                    .boards
                    .iter()
                    .find(|board| board.id == result.open_id)
                    .map(|board| board.title.clone())
                    .unwrap_or_else(|| SharedString::from(result.title));

                self.open_board_tab(result.open_id, result.project_id, title, window, cx);
            }
        }
    }

    pub(crate) fn apply_theme(&mut self, theme_name: &gpui::SharedString, cx: &mut Context<Self>) {
        if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme_config);
            cx.refresh_windows();
            cx.notify();
        }
    }
}
