use gpui_kit::{Context, Pixels, Window, px};

use settings::AppSettings;

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
