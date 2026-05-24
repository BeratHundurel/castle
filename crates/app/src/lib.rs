pub mod board;
pub mod castle_app;
pub mod markdown_editor;
pub mod sidebar;

use std::{path::PathBuf, sync::Arc};

use gpui::Global;
use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct DB {
    pub conn: Arc<DatabaseConnection>,
    pub data_dir: PathBuf,
}

impl Global for DB {}
