use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use super::StoredSettings;

#[derive(Clone, Default)]
pub(super) struct SettingsPersistence {
    latest_revision: Arc<AtomicU64>,
    pub(super) write_gate: Arc<tokio::sync::Mutex<()>>,
}

pub(super) struct SettingsWriteRequest {
    path: PathBuf,
    values: StoredSettings,
    revision: u64,
    latest_revision: Arc<AtomicU64>,
    write_gate: Arc<tokio::sync::Mutex<()>>,
}

impl SettingsPersistence {
    pub(super) fn prepare(&self, path: &Path, values: &StoredSettings) -> SettingsWriteRequest {
        let revision = self
            .latest_revision
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        SettingsWriteRequest {
            path: path.to_path_buf(),
            values: values.clone(),
            revision,
            latest_revision: self.latest_revision.clone(),
            write_gate: self.write_gate.clone(),
        }
    }

    pub(super) fn schedule(write: SettingsWriteRequest) {
        tokio::runtime::Handle::current().spawn(Self::write(write));
    }

    pub(super) async fn write(write: SettingsWriteRequest) {
        let _guard = write.write_gate.lock().await;
        if write.latest_revision.load(Ordering::Acquire) != write.revision {
            return;
        }
        let result =
            tokio::task::spawn_blocking(move || persist_values(&write.path, &write.values)).await;
        if let Err(error) = result {
            eprintln!("Settings writer failed: {error}");
        }
    }

    pub(super) fn write_sync(path: &Path, values: &StoredSettings) {
        persist_values(path, values);
    }
}

fn persist_values(path: &Path, values: &StoredSettings) {
    if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!(
            "Failed to create settings directory {}: {error}",
            parent.display()
        );
        return;
    }

    match serde_json::to_string_pretty(values) {
        Ok(contents) => {
            if let Err(error) = fs::write(path, contents) {
                eprintln!("Failed to write settings to {}: {error}", path.display());
            }
        }
        Err(error) => {
            eprintln!("Failed to serialize settings: {error}");
        }
    }
}
