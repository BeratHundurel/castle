use gpui::{Context, Window};

use super::AppShell;

impl AppShell {
    pub(super) fn on_toggle_sidebar_action(&mut self, _: &Window, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.toggle_collapsed(cx));
    }
}
