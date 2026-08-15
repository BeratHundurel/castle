use std::{
    future::Future,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use gpui::Global;
use storage::Store;

#[derive(Clone)]
pub struct AppServices {
    store: Store,
    data_dir: PathBuf,
    runtime: tokio::runtime::Handle,
}

impl AppServices {
    pub fn new(store: impl Into<Store>, data_dir: PathBuf) -> Self {
        let store = store.into();
        let runtime = tokio::runtime::Handle::current();
        Self {
            store,
            data_dir,
            runtime,
        }
    }

    pub fn store(&self) -> Store {
        self.store.clone()
    }

    pub fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    pub fn runtime(&self) -> tokio::runtime::Handle {
        self.runtime.clone()
    }

    pub fn spawn_store<T, F, Fut>(&self, operation: F) -> tokio::task::JoinHandle<T>
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

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}
