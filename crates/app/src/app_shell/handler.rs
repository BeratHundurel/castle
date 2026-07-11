use gpui::{Context, Pixels, Window, px};

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
