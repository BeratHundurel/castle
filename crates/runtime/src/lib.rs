use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
};

use gpui::{BackgroundExecutor, Global, Task};
use storage::Store;

#[derive(Clone)]
pub struct AppRuntime {
    inner: Arc<AppRuntimeInner>,
}

struct AppRuntimeInner {
    store: Store,
    data_dir: Arc<PathBuf>,
    tokio: tokio::runtime::Handle,
}

impl AppRuntime {
    pub fn new(store: impl Into<Store>, data_dir: PathBuf) -> Self {
        let store = store.into();
        let tokio = tokio::runtime::Handle::current();
        Self {
            inner: Arc::new(AppRuntimeInner {
                store,
                data_dir: Arc::new(data_dir),
                tokio,
            }),
        }
    }

    pub fn store(&self) -> Store {
        self.inner.store.clone()
    }

    pub fn data_dir(&self) -> &Path {
        self.inner.data_dir.as_path()
    }

    /// Returns the application data directory as a shared path handle.
    pub fn data_dir_handle(&self) -> Arc<PathBuf> {
        self.inner.data_dir.clone()
    }

    /// Runs a Tokio future without exposing its join handle to GPUI's
    /// foreground executor.
    pub fn spawn_tokio<T, Fut>(
        &self,
        background_executor: &BackgroundExecutor,
        future: Fut,
    ) -> Task<Result<T, tokio::task::JoinError>>
    where
        T: Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        spawn_tokio(background_executor, &self.inner.tokio, future)
    }

    /// Starts a Tokio future without returning a join handle to UI code.
    pub fn spawn_tokio_detached<Fut>(&self, future: Fut)
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        drop(self.inner.tokio.spawn(future));
    }

    /// Runs a store operation on Tokio while keeping its join handle off GPUI's
    /// foreground executor.
    pub fn spawn_store<T, F, Fut>(
        &self,
        background_executor: &BackgroundExecutor,
        operation: F,
    ) -> Task<Result<T, tokio::task::JoinError>>
    where
        T: Send + 'static,
        F: FnOnce(Store) -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        let store = self.store();
        self.spawn_tokio(background_executor, async move { operation(store).await })
    }
}

/// Runs a Tokio future without polling its join handle on GPUI's foreground executor.
///
/// Castle's application future runs inside Tokio. Polling a Tokio `JoinHandle`
/// directly from a foreground GPUI task can exhaust Tokio's cooperative budget
/// and continuously re-schedule that task on the UI thread.
fn spawn_tokio<T, Fut>(
    background_executor: &BackgroundExecutor,
    runtime: &tokio::runtime::Handle,
    future: Fut,
) -> Task<Result<T, tokio::task::JoinError>>
where
    T: Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let runtime = runtime.clone();
    background_executor.spawn(async move { AbortOnDrop::new(runtime.spawn(future)).await })
}

struct AbortOnDrop<T> {
    handle: Option<tokio::task::JoinHandle<T>>,
}

impl<T> AbortOnDrop<T> {
    fn new(handle: tokio::task::JoinHandle<T>) -> Self {
        Self {
            handle: Some(handle),
        }
    }
}

impl<T> Unpin for AbortOnDrop<T> {}

impl<T> Future for AbortOnDrop<T> {
    type Output = Result<T, tokio::task::JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().get_mut();
        let Some(handle) = this.handle.as_mut() else {
            unreachable!("completed Tokio task was polled again");
        };

        match Pin::new(handle).poll(cx) {
            Poll::Ready(result) => {
                this.handle.take();
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Global for AppRuntime {}
