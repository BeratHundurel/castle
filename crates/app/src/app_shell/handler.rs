use gpui::{Context, Window};

use crate::app_settings::AppSettings;

use super::AppShell;

impl AppShell {
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
}
