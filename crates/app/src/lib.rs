pub mod app_paths;
pub mod app_settings;
pub mod app_shell;
pub mod board;
pub(crate) mod color_contrast;
pub(crate) mod command_palette;
pub mod document_editor;
pub mod keymap;
pub mod mcp_registration;
pub mod sidebar;
pub mod system_notifications;
pub mod tray;

pub(crate) use storage::{folder_import, home, search, trash, workspace as workspace_data};

#[cfg(test)]
mod test_alloc;

use std::{path::PathBuf, sync::Arc};

use gpui::Global;
use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct DB {
    pub conn: Arc<DatabaseConnection>,
    pub data_dir: PathBuf,
}

impl Global for DB {}
