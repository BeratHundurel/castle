use gpui_kit::{Context, Pixels, Window, px};

use gpui_kit::Focusable as _;
use settings::{AppSettings, CheatsheetView, SettingsDocumentView};

use super::AppShell;

impl AppShell {
    pub(crate) fn on_command_palette_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.command_palette.update(cx, |palette, cx| {
            palette.set_active_project(self.workspace.active_project_id);
            palette.open_commands(window, cx);
        });
        self.refresh_workspace(cx);
    }

    pub(crate) fn open_workspace_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_palette
            .update(cx, |palette, cx| palette.open_workspace_search(window, cx));
    }

    pub(crate) fn open_theme_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_palette
            .update(cx, |palette, cx| palette.open_theme_switcher(window, cx));
    }

    pub(crate) fn on_close_command_palette_action(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.command_palette
            .update(cx, |palette, cx| palette.close(window, cx));
    }

    pub(crate) fn select_prev_command_palette_item(&mut self, cx: &mut Context<Self>) {
        self.command_palette
            .update(cx, |palette, cx| palette.select_previous(cx));
    }

    pub(crate) fn select_next_command_palette_item(&mut self, cx: &mut Context<Self>) {
        self.command_palette
            .update(cx, |palette, cx| palette.select_next(cx));
    }

    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_view
            .update(cx, |settings, cx| settings.open(window, cx));
    }

    pub(crate) fn open_settings_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self
            .tabs
            .open_tabs
            .iter()
            .position(|tab| matches!(&tab.kind, super::OpenTabKind::Settings { .. }))
        {
            self.activate_tab(index, window, cx);
            return;
        }

        let view = SettingsDocumentView::view(window, cx);
        Self::observe_settings_document(&view, window, cx);
        self.replace_or_push_active(
            super::OpenTabKind::Settings { view },
            "Settings".into(),
            window,
            cx,
        );
    }

    pub(crate) fn open_cheatsheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.on_close_command_palette_action(window, cx);
        if let Some(index) = self
            .tabs
            .open_tabs
            .iter()
            .position(|tab| matches!(&tab.kind, super::OpenTabKind::Cheatsheet { .. }))
        {
            self.activate_tab(index, window, cx);
            return;
        }

        self.cancel_pending_board_open();
        let view = CheatsheetView::view((self.shortcuts)(cx), window, cx);
        self.replace_or_push_active(
            super::OpenTabKind::Cheatsheet { view: view.clone() },
            "Cheatsheet".into(),
            window,
            cx,
        );
        view.focus_handle(cx).focus(window, cx);
    }

    pub(crate) fn open_board_template_picker(
        &mut self,
        project_id: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project_name = project_id.and_then(|project_id| {
            self.workspace
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .map(|project| project.name.clone())
        });
        self.board_template_picker.update(cx, |picker, cx| {
            picker.open(project_id, project_name, window, cx);
        });
    }

    pub(super) fn on_toggle_sidebar_action(&mut self, _: &Window, cx: &mut Context<Self>) {
        let visible = self.sidebar.read(cx).is_collapsed();
        self.set_sidebar_visible(visible, cx);
    }

    pub(crate) fn set_sidebar_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_collapsed(!visible, cx));
        AppSettings::set_show_sidebar(visible, cx);
        cx.notify();
    }

    pub(super) fn sync_sidebar_with_window_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let window_is_narrow = width <= px(super::SIDEBAR_AUTO_COLLAPSE_WIDTH);
        if window_is_narrow == self.window_is_narrow {
            return;
        }

        self.window_is_narrow = window_is_narrow;
        let visible = !window_is_narrow && AppSettings::show_sidebar(cx);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_collapsed(!visible, cx));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AppShell, CycleNextTab, OpenCheatsheetAction, OpenTabKind, test_shell_integration,
    };
    use command_palette::CommandPaletteAction;
    use gpui_kit::component::WindowExt as _;
    use gpui_kit::{
        AppContext as _, Focusable as _, KeyBinding, TestAppContext, VisualTestContext,
    };
    use settings::{AppSettings, StoredTab};

    #[gpui_kit::test]
    fn cheatsheet_shortcut_and_palette_share_a_persistent_regular_tab(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("test runtime");
        let _guard = runtime.enter();
        cx.executor().allow_parking();
        let directory = tempfile::tempdir().expect("isolated workspace");
        let store = runtime
            .block_on(storage::Store::connect(
                storage::StoreOptions::new("sqlite::memory:").connection_pool(1, 1),
            ))
            .expect("test store");
        let mut shell = None;
        let window = cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::load(directory.path()));
            cx.set_global(runtime::AppRuntime::new(
                store,
                directory.path().to_path_buf(),
            ));
            cx.bind_keys([
                KeyBinding::new("f1", OpenCheatsheetAction, Some("AppShell")),
                KeyBinding::new("ctrl-p", CommandPaletteAction, Some("AppShell")),
                KeyBinding::new("ctrl-tab", CycleNextTab, Some("AppShell")),
            ]);
            cx.open_window(Default::default(), |window, cx| {
                let view = AppShell::view(window, test_shell_integration(), cx);
                view.focus_handle(cx).focus(window, cx);
                shell = Some(view.clone());
                cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
            })
            .expect("test window")
        });
        let shell = shell.expect("shell exists");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_keystrokes("f1");
        let cheatsheet = shell.read_with(&cx, |shell, cx| {
            assert_eq!(shell.tabs.open_tabs.len(), 1);
            let OpenTabKind::Cheatsheet { view } = &shell.tabs.open_tabs[0].kind else {
                panic!("F1 should open the cheatsheet in place of the empty Home tab");
            };
            assert_eq!(
                AppSettings::tab_session(cx).tabs,
                vec![StoredTab::Cheatsheet]
            );
            view.clone()
        });
        cx.update(|window, cx| {
            assert!(cheatsheet.focus_handle(cx).is_focused(window));
            assert!(!window.has_active_dialog(cx));
            shell.update(cx, |shell, cx| shell.new_tab(window, cx));
        });
        cx.simulate_keystrokes("ctrl-p");
        cx.simulate_input("cheatsheet");
        cx.simulate_keystrokes("enter");
        shell.read_with(&cx, |shell, cx| {
            assert!(!shell.command_palette.read(cx).is_open());
            assert_eq!(shell.tabs.open_tabs.len(), 2);
            assert_eq!(shell.tabs.active_tab_index, 0);
            let OpenTabKind::Cheatsheet { view } = &shell.tabs.open_tabs[0].kind else {
                panic!("the palette should activate the existing cheatsheet tab");
            };
            assert_eq!(view.entity_id(), cheatsheet.entity_id());
        });
        cx.update(|window, cx| assert!(cheatsheet.focus_handle(cx).is_focused(window)));
        cx.simulate_keystrokes("ctrl-tab");
        shell.read_with(&cx, |shell, _| assert_eq!(shell.tabs.active_tab_index, 1));
        cx.simulate_keystrokes("ctrl-p f1");
        shell.read_with(&cx, |shell, cx| {
            assert!(!shell.command_palette.read(cx).is_open())
        });
        cx.update(|window, cx| {
            assert!(cheatsheet.focus_handle(cx).is_focused(window));
            let restored = AppShell::view(window, test_shell_integration(), cx);
            assert!(matches!(
                restored.read(cx).tabs.open_tabs[0].kind,
                OpenTabKind::Cheatsheet { .. }
            ));
            assert_eq!(restored.read(cx).tabs.active_tab_index, 0);
            shell.update(cx, |shell, cx| shell.close_tab(0, window, cx));
            assert_eq!(shell.read(cx).tabs.open_tabs.len(), 1);
            assert!(matches!(
                shell.read(cx).tabs.open_tabs[0].kind,
                OpenTabKind::Chooser
            ));
            assert_eq!(AppSettings::tab_session(cx).tabs, vec![StoredTab::Chooser]);
        });
    }
}
