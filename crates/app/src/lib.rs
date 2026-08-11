pub mod app_paths;
pub mod app_settings;
pub mod app_shell;
pub mod board;
pub(crate) mod color_contrast;
pub(crate) mod command_palette;
pub mod document_editor;
pub mod keymap;
pub mod mcp_registration;
pub(crate) mod request_tracker;
pub mod sidebar;
pub mod system_notifications;
pub mod tray;
pub(crate) mod workspace_navigation;

pub(crate) use storage::{folder_import, home, search, trash, workspace as workspace_data};

#[cfg(test)]
mod test_alloc;

use std::{future::Future, path::PathBuf};

use gpui::Global;
use storage::Store;

#[derive(Clone)]
pub struct AppServices {
    store: Store,
    data_dir: PathBuf,
    runtime: tokio::runtime::Handle,
    board_layout_persistence: storage::board_positions::BoardLayoutPersistence,
}

impl AppServices {
    pub fn new(store: impl Into<Store>, data_dir: PathBuf) -> Self {
        let store = store.into();
        let runtime = tokio::runtime::Handle::current();
        Self {
            store,
            data_dir,
            board_layout_persistence: storage::board_positions::BoardLayoutPersistence::new(
                runtime.clone(),
            ),
            runtime,
        }
    }

    pub(crate) fn store(&self) -> Store {
        self.store.clone()
    }

    pub(crate) fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    pub(crate) fn runtime(&self) -> tokio::runtime::Handle {
        self.runtime.clone()
    }

    pub(crate) fn board_layout_persistence(
        &self,
    ) -> storage::board_positions::BoardLayoutPersistence {
        self.board_layout_persistence.clone()
    }

    pub(crate) fn spawn_store<T, F, Fut>(&self, operation: F) -> tokio::task::JoinHandle<T>
    where
        T: Send + 'static,
        F: FnOnce(Store) -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        let store = self.store();
        self.runtime.spawn(async move { operation(store).await })
    }
}

impl Global for AppServices {}
