pub mod app_paths;
pub mod app_settings {
    pub use ::app_settings::*;
}
pub mod app_shell;
pub(crate) mod board;
pub(crate) mod color_contrast;
pub(crate) mod command_palette;
pub(crate) mod document_editor;
pub mod keymap;
pub mod mcp_registration;
pub(crate) mod request_tracker;
pub(crate) mod sidebar;
pub mod system_notifications;
pub mod tray;
pub(crate) mod workspace_navigation;

pub(crate) use storage::{folder_import, home, search, trash, workspace as workspace_data};

#[cfg(test)]
mod test_alloc;

pub use app_services::{AppServices, now_ts};

pub fn init_board(cx: &mut gpui::App) {
    board::init(cx);
}
