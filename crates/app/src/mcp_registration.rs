use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use toml_edit::{DocumentMut, Item, Table, value};

const MCP_SERVER_NAME: &str = "castle";
const CODEX_CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistrationStatus {
    Enabled,
    Available,
    ServerUnavailable,
}

pub fn register_installed() -> Result<()> {
    let server_path = installed_server_path()?;
    let codex_home = codex_home_from(|name| env::var_os(name))?;
    register(&codex_home, &server_path)
}

pub fn unregister() -> Result<()> {
    let codex_home = codex_home_from(|name| env::var_os(name))?;
    unregister_from(&codex_home)
}

pub fn status() -> Result<RegistrationStatus> {
    let codex_home = codex_home_from(|name| env::var_os(name))?;
    if is_registered(&codex_home)? {
        return Ok(RegistrationStatus::Enabled);
    }

    match installed_server_path() {
        Ok(_) => Ok(RegistrationStatus::Available),
        Err(_) => Ok(RegistrationStatus::ServerUnavailable),
    }
}

fn register(codex_home: &Path, server_path: &Path) -> Result<()> {
    let config_path = codex_home.join(CODEX_CONFIG_FILE_NAME);
    let mut document = match fs::read_to_string(&config_path) {
        Ok(contents) => contents
            .parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", config_path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", config_path.display()));
        }
    };

    let mcp_servers = ensure_table(document.as_table_mut(), "mcp_servers")?;
    if mcp_servers.contains_key(MCP_SERVER_NAME) {
        return Ok(());
    }

    let mut castle = Table::new();
    castle.insert("command", value(server_path.to_string_lossy().into_owned()));
    mcp_servers.insert(MCP_SERVER_NAME, Item::Table(castle));

    fs::create_dir_all(codex_home)
        .with_context(|| format!("failed to create {}", codex_home.display()))?;

    fs::write(&config_path, document.to_string())
        .with_context(|| format!("failed to update {}", config_path.display()))?;

    Ok(())
}

fn unregister_from(codex_home: &Path) -> Result<()> {
    let config_path = codex_home.join(CODEX_CONFIG_FILE_NAME);
    let contents = match fs::read_to_string(&config_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", config_path.display()));
        }
    };
    let mut document = contents
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", config_path.display()))?;

    let Some(mcp_servers) = document
        .as_table_mut()
        .get_mut("mcp_servers")
        .and_then(Item::as_table_mut)
    else {
        return Ok(());
    };
    if mcp_servers.remove(MCP_SERVER_NAME).is_none() {
        return Ok(());
    }

    fs::write(&config_path, document.to_string())
        .with_context(|| format!("failed to update {}", config_path.display()))?;
    Ok(())
}

fn is_registered(codex_home: &Path) -> Result<bool> {
    let config_path = codex_home.join(CODEX_CONFIG_FILE_NAME);
    let contents = match fs::read_to_string(&config_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", config_path.display()));
        }
    };
    let document = contents
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", config_path.display()))?;

    Ok(document
        .get("mcp_servers")
        .and_then(Item::as_table)
        .is_some_and(|servers| servers.contains_key(MCP_SERVER_NAME)))
}

fn installed_server_path() -> Result<PathBuf> {
    let executable = env::current_exe().context("failed to locate Castle")?;
    server_path_beside(&executable)
}

fn server_path_beside(executable: &Path) -> Result<PathBuf> {
    let install_directory = executable
        .parent()
        .context("Castle's installation directory is unavailable")?;
    let expected = install_directory.join(server_executable_name());
    if expected.is_file() {
        return Ok(expected);
    }

    #[cfg(target_os = "windows")]
    if let Some(file_name) = executable.file_name().and_then(|name| name.to_str())
        && file_name
            .get(.."Castle-".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Castle-"))
    {
        let portable_name = format!("Castle-MCP-{}", &file_name["Castle-".len()..]);
        let portable_path = install_directory.join(portable_name);
        if portable_path.is_file() {
            return Ok(portable_path);
        }
    }

    #[cfg(target_os = "windows")]
    if let Some(path) = fs::read_dir(install_directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    let name = name.to_ascii_lowercase();
                    name.starts_with("castle-mcp-") && name.ends_with(".exe")
                })
        })
        .min()
    {
        return Ok(path);
    }

    bail!(
        "Castle MCP server was not found beside Castle at {}",
        expected.display()
    )
}

fn ensure_table<'a>(parent: &'a mut Table, key: &str) -> Result<&'a mut Table> {
    if !parent.contains_key(key) {
        parent.insert(key, Item::Table(Table::new()));
    }

    parent
        .get_mut(key)
        .and_then(Item::as_table_mut)
        .with_context(|| format!("Codex configuration key `{key}` is not a table"))
}

fn codex_home_from(get_env: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    if let Some(codex_home) = get_env("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(codex_home);
    }

    let home = get_env(home_environment_variable())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .context("the user home directory is unavailable")?;

    Ok(home.join(".codex"))
}

#[cfg(target_os = "windows")]
fn home_environment_variable() -> &'static str {
    "USERPROFILE"
}

#[cfg(not(target_os = "windows"))]
fn home_environment_variable() -> &'static str {
    "HOME"
}

#[cfg(target_os = "windows")]
fn server_executable_name() -> &'static str {
    "Castle-MCP.exe"
}

#[cfg(not(target_os = "windows"))]
fn server_executable_name() -> &'static str {
    "castle-mcp"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_castle_without_replacing_other_settings() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config_path = directory.path().join(CODEX_CONFIG_FILE_NAME);
        fs::write(
            &config_path,
            "model = \"gpt-test\"\n\n[mcp_servers.docs]\nurl = \"https://example.com/mcp\"\n",
        )?;

        register(
            directory.path(),
            Path::new(r"C:\Program Files\Castle\Castle-MCP.exe"),
        )?;

        let document = fs::read_to_string(config_path)?.parse::<DocumentMut>()?;
        assert_eq!(document["model"].as_str(), Some("gpt-test"));
        assert_eq!(
            document["mcp_servers"]["docs"]["url"].as_str(),
            Some("https://example.com/mcp")
        );
        assert_eq!(
            document["mcp_servers"][MCP_SERVER_NAME]["command"].as_str(),
            Some(r"C:\Program Files\Castle\Castle-MCP.exe")
        );
        Ok(())
    }

    #[test]
    fn preserves_an_existing_castle_server() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config_path = directory.path().join(CODEX_CONFIG_FILE_NAME);
        fs::write(
            &config_path,
            "[mcp_servers.castle]\ncommand = \"custom-castle-mcp\"\n",
        )?;

        register(directory.path(), Path::new("new-castle-mcp"))?;

        let document = fs::read_to_string(config_path)?.parse::<DocumentMut>()?;
        assert_eq!(
            document["mcp_servers"][MCP_SERVER_NAME]["command"].as_str(),
            Some("custom-castle-mcp")
        );
        Ok(())
    }

    #[test]
    fn unregisters_castle_without_replacing_other_settings() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config_path = directory.path().join(CODEX_CONFIG_FILE_NAME);
        fs::write(
            &config_path,
            "model = \"gpt-test\"\n\n[mcp_servers.docs]\nurl = \"https://example.com/mcp\"\n\n[mcp_servers.castle]\ncommand = \"castle-mcp\"\n",
        )?;

        unregister_from(directory.path())?;

        let document = fs::read_to_string(config_path)?.parse::<DocumentMut>()?;
        assert_eq!(document["model"].as_str(), Some("gpt-test"));
        assert!(document["mcp_servers"].get("castle").is_none());
        assert_eq!(
            document["mcp_servers"]["docs"]["url"].as_str(),
            Some("https://example.com/mcp")
        );
        Ok(())
    }

    #[test]
    fn reports_registered_server() -> Result<()> {
        let directory = tempfile::tempdir()?;
        fs::write(
            directory.path().join(CODEX_CONFIG_FILE_NAME),
            "[mcp_servers.castle]\ncommand = \"castle-mcp\"\n",
        )?;

        assert!(is_registered(directory.path())?);
        Ok(())
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn finds_the_matching_portable_server() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let app_path = directory.path().join("Castle-0.1.0-windows-x86_64.exe");
        let server_path = directory.path().join("Castle-MCP-0.1.0-windows-x86_64.exe");
        fs::write(&server_path, [])?;

        assert_eq!(server_path_beside(&app_path)?, server_path);
        Ok(())
    }

    #[test]
    fn uses_codex_home_when_it_is_configured() -> Result<()> {
        let path = codex_home_from(|name| {
            (name == "CODEX_HOME").then(|| OsString::from(r"C:\CodexData"))
        })?;

        assert_eq!(path, PathBuf::from(r"C:\CodexData"));
        Ok(())
    }
}
