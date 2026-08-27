use std::{future::Future, path::PathBuf};

use gpui::Global;
use storage::Store;

#[derive(Clone)]
pub struct AppRuntime {
    store: Store,
    data_dir: PathBuf,
    tokio: tokio::runtime::Handle,
}

impl AppRuntime {
    pub fn new(store: impl Into<Store>, data_dir: PathBuf) -> Self {
        let store = store.into();
        let tokio = tokio::runtime::Handle::current();
        Self {
            store,
            data_dir,
            tokio,
        }
    }

    pub fn store(&self) -> Store {
        self.store.clone()
    }

    pub fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    pub fn tokio_handle(&self) -> tokio::runtime::Handle {
        self.tokio.clone()
    }

    pub fn spawn_store<T, F, Fut>(&self, operation: F) -> tokio::task::JoinHandle<T>
    where
        T: Send + 'static,
        F: FnOnce(Store) -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        let store = self.store();
        self.tokio.spawn(async move { operation(store).await })
    }
}

impl Global for AppRuntime {}
