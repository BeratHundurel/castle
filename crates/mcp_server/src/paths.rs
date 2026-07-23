use std::{env, ffi::OsString, fs, path::PathBuf};

use anyhow::{Context, Result, bail};

const DATABASE_FILE_NAME: &str = "castle.db";

pub(crate) fn database_url(args: impl Iterator<Item = String>) -> Result<String> {
    let mut args = args.peekable();
    let mut database = None;
    while let Some(arg) = args.next() {
        if arg == "--database" {
            database = Some(
                args.next()
                    .context("--database requires a SQLite file path or URL")?,
            );
        } else if let Some(value) = arg.strip_prefix("--database=") {
            database = Some(value.to_string());
        } else {
            bail!("unknown argument: {arg}");
        }
    }

    if let Some(database) = database {
        return Ok(as_sqlite_url(database));
    }
    if let Some(database) = env::var_os("CASTLE_DATABASE_URL")
        .or_else(|| env::var_os("DATABASE_URL"))
        .filter(|value| !value.is_empty())
    {
        return Ok(as_sqlite_url(database.to_string_lossy().into_owned()));
    }

    Ok(as_sqlite_url(
        native_data_dir(|name| env::var_os(name))?
            .join(DATABASE_FILE_NAME)
            .to_string_lossy()
            .into_owned(),
    ))
}

pub(crate) fn prepare_database_file(database_url: &str) -> Result<()> {
    let path = database_url
        .strip_prefix("sqlite:")
        .context("Castle MCP only supports SQLite database URLs")?;
    if path == ":memory:" || path.starts_with("file:") {
        return Ok(());
    }

    let path = PathBuf::from(path);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    if !path.exists() {
        fs::File::create(path)?;
    }
    Ok(())
}

fn as_sqlite_url(value: String) -> String {
    if value.starts_with("sqlite:") {
        value
    } else {
        format!("sqlite:{}", value.replace('\\', "/"))
    }
}

#[cfg(target_os = "windows")]
fn native_data_dir(get_env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    let local_app_data = get_env("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .context("LOCALAPPDATA is unavailable; pass --database explicitly")?;
    Ok(local_app_data.join("castle"))
}

#[cfg(target_os = "macos")]
fn native_data_dir(get_env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    let home = get_env("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .context("HOME is unavailable; pass --database explicitly")?;
    Ok(home.join("Library/Application Support/castle"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn native_data_dir(get_env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    if let Some(data_home) = get_env("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(data_home.join("castle"));
    }
    let home = get_env("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .context("HOME is unavailable; pass --database explicitly")?;
    Ok(home.join(".local/share/castle"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_database_path_becomes_sqlite_url() -> Result<()> {
        let url = database_url(
            ["--database", r"C:\data\castle.db"]
                .map(str::to_string)
                .into_iter(),
        )?;
        assert_eq!(url, "sqlite:C:/data/castle.db");
        Ok(())
    }
}
