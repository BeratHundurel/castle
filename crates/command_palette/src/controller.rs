use std::time::Duration;

use gpui::{Context, SharedString, Window};
use gpui_component::ActiveTheme as _;
use runtime::AppRuntime;
use settings::AppSettings;
use storage::workspace::search;

use crate::{
    CommandPaletteEvent, CommandPaletteMode, CommandPaletteView, PaletteCommand, PaletteCommandKind,
};

const WORKSPACE_SEARCH_DEBOUNCE: Duration = Duration::from_millis(180);

impl CommandPaletteView {
    pub fn open_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.mode = CommandPaletteMode::Commands;
        self.reset_query("Type a command", window, cx);
        cx.notify();
    }

    pub fn open_workspace_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.mode = CommandPaletteMode::Search;
        self.search_results.clear();
        self.search_loading = true;
        self.search_error = None;
        self.search_preview_scroll_pending.set(true);
        self.reset_query("Search notes, boards, cards, and entries", window, cx);
        self.rebuild_workspace_search_index(cx);
        cx.notify();
    }

    pub fn open_theme_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.mode = CommandPaletteMode::Themes;
        self.query.clear();
        self.cancel_search();
        self.selected_index = self
            .filtered_theme_names(cx)
            .iter()
            .position(|name| name == cx.theme().theme_name())
            .unwrap_or(0);
        self.scroll_handle.scroll_to_item(self.selected_index);
        self.suppress_input_event = true;
        self.input.update(cx, |input, cx| {
            input.set_placeholder("Search themes", window, cx);
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.suppress_input_event = false;
        cx.notify();
    }

    pub fn close(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }

        self.open = false;
        self.mode = CommandPaletteMode::Commands;
        self.query.clear();
        self.cancel_search();
        self.search_results.clear();
        self.search_loading = false;
        self.search_error = None;
        self.selected_index = 0;
        self.scroll_handle.scroll_to_item(0);
        cx.emit(CommandPaletteEvent::Closed);
        cx.notify();
    }

    pub fn execute_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }

        match self.mode {
            CommandPaletteMode::Commands => {
                let commands = self.command_palette_commands();
                if let Some(command) = commands
                    .get(self.selected_index.min(commands.len().saturating_sub(1)))
                    .cloned()
                {
                    self.execute_palette_command(command, window, cx);
                }
            }
            CommandPaletteMode::Themes => {
                let themes = self.filtered_theme_names(cx);
                if let Some(theme_name) = themes
                    .get(self.selected_index.min(themes.len().saturating_sub(1)))
                    .cloned()
                {
                    self.apply_theme(&theme_name, cx);
                    self.close(window, cx);
                }
            }
            CommandPaletteMode::Search => {
                if let Some(result) = self
                    .search_results
                    .get(
                        self.selected_index
                            .min(self.search_results.len().saturating_sub(1)),
                    )
                    .cloned()
                {
                    self.open_search_result(result, window, cx);
                }
            }
        }
    }

    pub fn select_previous(&mut self, cx: &mut Context<Self>) {
        self.move_selection(-1, cx);
    }

    pub fn select_next(&mut self, cx: &mut Context<Self>) {
        self.move_selection(1, cx);
    }

    pub(super) fn execute_palette_command(
        &mut self,
        command: PaletteCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let event = match command.kind {
            PaletteCommandKind::OpenNote {
                note_id,
                project_id,
                title,
            } => CommandPaletteEvent::OpenNote {
                note_id,
                project_id,
                title,
            },
            PaletteCommandKind::OpenBoard {
                board_id,
                project_id,
                title,
            } => CommandPaletteEvent::OpenBoard {
                board_id,
                project_id,
                title,
            },
            PaletteCommandKind::NewNote { project_id, title } => {
                CommandPaletteEvent::NewNote { project_id, title }
            }
            PaletteCommandKind::NewBoard { project_id, title } => {
                CommandPaletteEvent::NewBoard { project_id, title }
            }
            PaletteCommandKind::OpenFile => CommandPaletteEvent::OpenFile,
            PaletteCommandKind::NewTab => CommandPaletteEvent::NewTab,
            PaletteCommandKind::CloseAllTabs => CommandPaletteEvent::CloseAllTabs,
            PaletteCommandKind::OpenSettings => CommandPaletteEvent::OpenSettings,
            PaletteCommandKind::CreateCardFromSelection => {
                CommandPaletteEvent::CreateCardFromSelection
            }
            PaletteCommandKind::InsertBoardView => CommandPaletteEvent::InsertBoardView,
            PaletteCommandKind::SwitchTheme => {
                self.open_theme_switcher(window, cx);
                return;
            }
            PaletteCommandKind::SearchWorkspace => {
                self.open_workspace_search(window, cx);
                return;
            }
        };

        self.close(window, cx);
        cx.emit(event);
    }

    pub(super) fn run_workspace_search(&mut self, cx: &mut Context<Self>) {
        let query = self.query.trim().to_string();
        self.search_generation = self.search_generation.saturating_add(1);
        let generation = self.search_generation;
        self.search_debounce_task = None;
        self.search_loading = true;
        self.search_error = None;

        let app_runtime = cx.global::<AppRuntime>().clone();
        self.search_debounce_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(WORKSPACE_SEARCH_DEBOUNCE)
                .await;
            let result = app_runtime
                .spawn_store(move |store| async move {
                    search::search_workspace(&store, &query, 20).await
                })
                .await;

            this.update(cx, |this, cx| {
                if this.mode != CommandPaletteMode::Search || this.search_generation != generation {
                    return;
                }

                this.search_debounce_task = None;
                this.search_loading = false;
                match result {
                    Ok(Ok(results)) => {
                        this.search_results = results;
                        this.search_error = None;
                        this.search_preview_scroll_pending.set(true);
                    }
                    Ok(Err(err)) => {
                        this.search_results.clear();
                        this.search_error =
                            Some(SharedString::from(format!("Search failed: {err}")));
                    }
                    Err(err) => {
                        this.search_results.clear();
                        this.search_error = Some(SharedString::from(format!(
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
        let app_runtime = cx.global::<AppRuntime>().clone();
        cx.spawn(async move |this, cx| {
            let result = app_runtime
                .spawn_store(|store| async move { search::rebuild_search_index(&store).await })
                .await;

            this.update(cx, |this, cx| {
                if this.mode != CommandPaletteMode::Search {
                    return;
                }

                match result {
                    Ok(Ok(())) => {
                        this.search_error = None;
                        this.run_workspace_search(cx);
                    }
                    Ok(Err(err)) => {
                        this.search_loading = false;
                        this.search_results.clear();
                        this.search_error =
                            Some(SharedString::from(format!("Search index failed: {err}")));
                        cx.notify();
                    }
                    Err(err) => {
                        this.search_loading = false;
                        this.search_results.clear();
                        this.search_error = Some(SharedString::from(format!(
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

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }

        let len = match self.mode {
            CommandPaletteMode::Commands => self.command_palette_commands().len(),
            CommandPaletteMode::Themes => self.filtered_theme_names(cx).len(),
            CommandPaletteMode::Search => self.search_results.len(),
        };
        if len == 0 {
            self.selected_index = 0;
            cx.notify();
            return;
        }

        let current = self.selected_index.min(len.saturating_sub(1));
        self.selected_index = if delta.is_negative() {
            current.checked_sub(1).unwrap_or(len - 1)
        } else {
            (current + 1) % len
        };
        if self.mode == CommandPaletteMode::Search {
            self.search_preview_scroll_pending.set(true);
        }
        self.scroll_handle.scroll_to_item(self.selected_index);

        if self.mode == CommandPaletteMode::Themes
            && let Some(theme_name) = self
                .filtered_theme_names(cx)
                .get(self.selected_index)
                .cloned()
        {
            self.apply_theme(&theme_name, cx);
        }
        cx.notify();
    }

    pub(super) fn open_search_result(
        &mut self,
        result: search::SearchResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close(window, cx);
        cx.emit(CommandPaletteEvent::OpenSearchResult(result));
    }

    pub(super) fn apply_theme(&mut self, theme_name: &SharedString, cx: &mut Context<Self>) {
        AppSettings::set_theme_name(theme_name.clone(), cx);
        cx.notify();
    }

    fn reset_query(
        &mut self,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.query.clear();
        self.cancel_search();
        self.selected_index = 0;
        self.scroll_handle.scroll_to_item(0);
        self.suppress_input_event = true;
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx);
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.suppress_input_event = false;
    }

    fn cancel_search(&mut self) {
        self.search_generation = self.search_generation.saturating_add(1);
        self.search_debounce_task = None;
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
                Ok::<_, sea_orm::DbErr>(db)
            })
            .expect("search test database should initialize");
        let app_runtime = AppRuntime::new(Arc::new(db), PathBuf::new());
        let settings_dir = std::env::temp_dir().join(format!(
            "castle-workspace-search-test-{}",
            std::process::id()
        ));

        let mut palette = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(AppSettings::load(settings_dir));
            cx.set_global(app_runtime);
            cx.open_window(Default::default(), |window, cx| {
                let view = CommandPaletteView::view(window, cx);
                palette = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("search test window should open")
        });
        let palette = palette.expect("command palette should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);

        cx.update(|window, cx| {
            palette.update(cx, |palette, cx| {
                palette.open_workspace_search(window, cx);
            });
        });
        for _ in 0..50 {
            cx.run_until_parked();
            if !palette.read_with(&cx, |palette, _| palette.search_loading) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let input = palette.read_with(&cx, |palette, _| palette.input.clone());
        cx.update(|window, cx| {
            input.update(cx, |input, cx| input.insert("needle", window, cx));
        });
        cx.run_until_parked();
        cx.executor().advance_clock(WORKSPACE_SEARCH_DEBOUNCE);

        for _ in 0..50 {
            cx.run_until_parked();
            if !palette.read_with(&cx, |palette, _| palette.search_loading) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        palette.read_with(&cx, |palette, _| {
            assert_eq!(palette.query, "needle");
            assert_eq!(palette.search_results.len(), 1);
            assert_eq!(palette.search_results[0].title, "Search regression needle");
            assert!(palette.search_error.is_none());
        });
    }
}
