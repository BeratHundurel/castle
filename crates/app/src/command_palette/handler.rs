use std::time::Duration;

use gpui::{Context, SharedString, Window};
use gpui_component::ActiveTheme as _;

use crate::DB;
use crate::app_settings::AppSettings;
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

    pub(crate) fn open_theme_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_palette.open = true;
        self.command_palette.mode = CommandPaletteMode::Themes;
        self.command_palette.query.clear();
        self.command_palette.search_generation =
            self.command_palette.search_generation.saturating_add(1);
        self.command_palette.search_debounce_task = None;
        self.command_palette.selected_index = self
            .filtered_theme_names(cx)
            .iter()
            .position(|name| name == cx.theme().theme_name())
            .unwrap_or(0);
        self.command_palette
            .scroll_handle
            .scroll_to_item(self.command_palette.selected_index);
        self.command_palette.suppress_input_event = true;
        self.command_palette.input.update(cx, |input, cx| {
            input.set_placeholder("Search themes", window, cx);
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.command_palette.suppress_input_event = false;
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
                    self.close_command_palette(window, cx);
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
                self.open_text_file(window, cx);
            }
            PaletteCommandKind::NewTab => {
                self.close_command_palette(window, cx);
                self.new_tab(window, cx);
            }
            PaletteCommandKind::CloseAllTabs => {
                self.close_command_palette(window, cx);
                self.close_all_tabs(window, cx);
            }
            PaletteCommandKind::OpenSettings => {
                self.close_command_palette(window, cx);
                self.open_settings(window, cx);
            }
            PaletteCommandKind::SwitchTheme => {
                self.open_theme_switcher(window, cx);
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
        let runtime = tokio::runtime::Handle::current();
        self.command_palette.search_debounce_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(WORKSPACE_SEARCH_DEBOUNCE)
                .await;

            let result = runtime
                .spawn(
                    async move { crate::search::search_workspace(db.as_ref(), &query, 20).await },
                )
                .await;

            this.update(cx, |this, cx| {
                if this.command_palette.mode != CommandPaletteMode::Search
                    || this.command_palette.search_generation != generation
                {
                    return;
                }

                this.command_palette.search_debounce_task = None;
                this.command_palette.search_loading = false;
                match result {
                    Ok(Ok(results)) => {
                        this.command_palette.search_results = results;
                        this.command_palette.search_error = None;
                    }
                    Ok(Err(err)) => {
                        this.command_palette.search_results.clear();
                        this.command_palette.search_error =
                            Some(SharedString::from(format!("Search failed: {err}")));
                    }
                    Err(err) => {
                        this.command_palette.search_results.clear();
                        this.command_palette.search_error = Some(SharedString::from(format!(
                            "Workspace search task failed: {err}"
                        )));
                    }
                }

                cx.notify();
            })
            .ok();
        }));
    }

    fn rebuild_workspace_search_index(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();

        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move { crate::search::rebuild_search_index(db.as_ref()).await })
                .await;

            this.update(cx, |this, cx| {
                if this.command_palette.mode != CommandPaletteMode::Search {
                    return;
                }

                match result {
                    Ok(Ok(())) => {
                        this.command_palette.search_error = None;
                        this.run_workspace_search(cx);
                    }
                    Ok(Err(err)) => {
                        this.command_palette.search_loading = false;
                        this.command_palette.search_results.clear();
                        this.command_palette.search_error =
                            Some(SharedString::from(format!("Search index failed: {err}")));
                        cx.notify();
                    }
                    Err(err) => {
                        this.command_palette.search_loading = false;
                        this.command_palette.search_results.clear();
                        this.command_palette.search_error = Some(SharedString::from(format!(
                            "Search index task failed: {err}"
                        )));
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
        AppSettings::set_theme_name(theme_name.clone(), cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc, time::Duration};

    use entity::note;
    use gpui::AppContext as _;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};

    use super::*;

    #[gpui::test]
    fn workspace_search_applies_results_after_input_changes(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();

        let db = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                note::ActiveModel {
                    title: Set("Search regression needle".to_string()),
                    project_id: Set(None),
                    file_path: Set(None),
                    file_managed_by_app: Set(false),
                    cached_content: Set("A searchable note body".to_string()),
                    file_missing_since: Set(None),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>(db)
            })
            .expect("search test database should initialize");
        let app_db = crate::DB {
            conn: Arc::new(db),
            data_dir: PathBuf::new(),
        };
        let settings_dir = std::env::temp_dir().join(format!(
            "castle-workspace-search-test-{}",
            std::process::id()
        ));

        let mut shell = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(crate::app_settings::AppSettings::load(settings_dir));
            cx.set_global(app_db);
            cx.open_window(Default::default(), |window, cx| {
                let view = AppShell::view(window, cx);
                shell = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("search test window should open")
        });
        let shell = shell.expect("app shell should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.open_workspace_search(window, cx);
            });
        });
        for _ in 0..50 {
            cx.run_until_parked();
            if !shell.read_with(&cx, |shell, _| shell.command_palette.search_loading) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let input = shell.read_with(&cx, |shell, _| shell.command_palette.input.clone());
        cx.update(|window, cx| {
            input.update(cx, |input, cx| input.insert("needle", window, cx));
        });
        cx.run_until_parked();
        cx.executor().advance_clock(WORKSPACE_SEARCH_DEBOUNCE);

        for _ in 0..50 {
            cx.run_until_parked();
            if !shell.read_with(&cx, |shell, _| shell.command_palette.search_loading) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        shell.read_with(&cx, |shell, _| {
            assert_eq!(shell.command_palette.query, "needle");
            assert_eq!(shell.command_palette.search_results.len(), 1);
            assert_eq!(
                shell.command_palette.search_results[0].title,
                "Search regression needle"
            );
            assert!(shell.command_palette.search_error.is_none());
        });
    }
}
