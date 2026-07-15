use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

const APP_DIRECTORY_NAME: &str = "castle";
const DATABASE_FILE_NAME: &str = "castle.db";

pub struct AppPaths {
    pub data_dir: PathBuf,
    pub database_url: String,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        #[cfg(debug_assertions)]
        if let Ok(database_url) = env::var("DATABASE_URL")
            && !database_url.trim().is_empty()
        {
            return Self::from_database_url(database_url);
        }

        let data_dir = native_data_dir()?;
        let database_url = database_url_for_path(&data_dir.join(DATABASE_FILE_NAME));

        Ok(Self {
            data_dir,
            database_url,
        })
    }

    #[cfg(debug_assertions)]
    fn from_database_url(database_url: String) -> Result<Self> {
        let database_path = database_path_from_url(&database_url)?;
        let data_dir = database_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        Ok(Self {
            data_dir,
            database_url,
        })
    }

    pub fn database_path(&self) -> Result<PathBuf> {
        database_path_from_url(&self.database_url)
    }
}

fn native_data_dir() -> Result<PathBuf> {
    native_data_dir_from(|name| env::var_os(name))
}

#[cfg(target_os = "windows")]
fn native_data_dir_from(get_env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    let local_app_data = get_env("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .context("LOCALAPPDATA is not available; Castle cannot determine its data directory")?;

    Ok(local_app_data.join(APP_DIRECTORY_NAME))
}

#[cfg(target_os = "macos")]
fn native_data_dir_from(get_env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    let home = get_env("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .context("HOME is not available; Castle cannot determine its data directory")?;

    Ok(home
        .join("Library")
        .join("Application Support")
        .join(APP_DIRECTORY_NAME))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn native_data_dir_from(get_env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    if let Some(data_home) = get_env("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(data_home.join(APP_DIRECTORY_NAME));
    }

    let home = get_env("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .context("HOME is not available; Castle cannot determine its data directory")?;

    Ok(home.join(".local").join("share").join(APP_DIRECTORY_NAME))
}

fn database_url_for_path(path: &Path) -> String {
    format!("sqlite:{}", path.to_string_lossy().replace('\\', "/"))
}

fn database_path_from_url(database_url: &str) -> Result<PathBuf> {
    let path = database_url
        .strip_prefix("sqlite:")
        .context("DATABASE_URL must use the sqlite: scheme")?;

    if path.is_empty() || path == ":memory:" {
        bail!("DATABASE_URL must point to a SQLite database file");
    }

    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_override_keeps_all_app_data_beside_the_database() -> Result<()> {
        let paths = AppPaths::from_database_url("sqlite:custom/location/castle.db".to_string())?;

        assert_eq!(paths.data_dir, PathBuf::from("custom/location"));
        assert_eq!(
            paths.database_path()?,
            PathBuf::from("custom/location/castle.db")
        );

        Ok(())
    }

    #[test]
    fn database_paths_preserve_url_characters_in_directory_names() -> Result<()> {
        let paths = AppPaths::from_database_url("sqlite:custom/#notes/castle.db".to_string())?;

        assert_eq!(paths.data_dir, PathBuf::from("custom/#notes"));
        assert_eq!(
            paths.database_path()?,
            PathBuf::from("custom/#notes/castle.db")
        );

        Ok(())
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_data_uses_local_app_data() -> Result<()> {
        let path = native_data_dir_from(|name| {
            (name == "LOCALAPPDATA").then(|| OsString::from(r"C:\Users\Ada\AppData\Local"))
        })?;

        assert_eq!(path, PathBuf::from(r"C:\Users\Ada\AppData\Local\castle"));
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_data_uses_application_support() -> Result<()> {
        let path =
            native_data_dir_from(|name| (name == "HOME").then(|| OsString::from("/Users/ada")))?;

        assert_eq!(
            path,
            PathBuf::from("/Users/ada/Library/Application Support/castle")
        );
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_data_prefers_xdg_data_home() -> Result<()> {
        let path = native_data_dir_from(|name| {
            (name == "XDG_DATA_HOME").then(|| OsString::from("/home/ada/data"))
        })?;

        assert_eq!(path, PathBuf::from("/home/ada/data/castle"));
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_data_falls_back_to_the_home_directory() -> Result<()> {
        let path =
            native_data_dir_from(|name| (name == "HOME").then(|| OsString::from("/home/ada")))?;

        assert_eq!(path, PathBuf::from("/home/ada/.local/share/castle"));
        Ok(())
    }
}
