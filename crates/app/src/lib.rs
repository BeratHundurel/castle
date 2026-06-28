pub mod app_shell;
pub mod board;
pub(crate) mod command_palette;
pub mod keymap;
pub mod markdown_editor;
pub mod search;
pub mod sidebar;
pub(crate) mod workspace_data;

use std::{path::PathBuf, sync::Arc};

use gpui::Global;
use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct DB {
    pub conn: Arc<DatabaseConnection>,
    pub data_dir: PathBuf,
}

impl Global for DB {}
