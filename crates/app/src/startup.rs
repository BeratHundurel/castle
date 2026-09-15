use anyhow::Result;
#[cfg(any(target_os = "windows", test))]
use std::path::Path;
use std::{ffi::OsStr, ffi::OsString};

const START_IN_TRAY_ARGUMENT: &str = "--start-in-tray";

pub fn starts_in_tray<I>(arguments: I) -> bool
where
    I: IntoIterator<Item = OsString>,
{
    arguments
        .into_iter()
        .any(|argument| argument == OsStr::new(START_IN_TRAY_ARGUMENT))
}

pub fn set_start_at_login(enabled: bool) -> Result<()> {
    #[cfg(target_os = "windows")]
    return windows_registry::set_start_at_login(enabled);

    #[cfg(not(target_os = "windows"))]
    {
        if enabled {
            anyhow::bail!("Start at login is currently supported on Windows only");
        }

        Ok(())
    }
}

#[cfg(any(target_os = "windows", test))]
fn startup_command(executable: &Path) -> String {
    format!("\"{}\" {START_IN_TRAY_ARGUMENT}", executable.display())
}

#[cfg(target_os = "windows")]
mod windows_registry {
    use std::{env, iter::once};

    use anyhow::{Context as _, Result, bail};
    use windows::{
        Win32::{
            Foundation::ERROR_FILE_NOT_FOUND,
            System::Registry::{
                HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegCreateKeyW,
                RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
            },
        },
        core::PCWSTR,
    };

    const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "Castle";

    pub(super) fn set_start_at_login(enabled: bool) -> Result<()> {
        if enabled {
            let executable = env::current_exe().context("Could not find the Castle executable")?;
            let Some(key) = open_run_key(true)? else {
                bail!("Could not open the Windows startup registry key");
            };
            return write_string_value(&key, VALUE_NAME, &super::startup_command(&executable));
        }

        let Some(key) = open_run_key(false)? else {
            return Ok(());
        };
        delete_value(&key, VALUE_NAME)
    }

    fn open_run_key(create: bool) -> Result<Option<RegistryKey>> {
        let key_path = wide(RUN_KEY_PATH);
        let mut key = HKEY::default();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR::from_raw(key_path.as_ptr()),
                None,
                KEY_SET_VALUE,
                &mut key,
            )
        };

        if status.is_ok() {
            return Ok(Some(RegistryKey(key)));
        }
        if status != ERROR_FILE_NOT_FOUND {
            bail!(
                "Could not open the Windows startup registry key ({})",
                status.0
            );
        }
        if !create {
            return Ok(None);
        }

        let status = unsafe {
            RegCreateKeyW(
                HKEY_CURRENT_USER,
                PCWSTR::from_raw(key_path.as_ptr()),
                &mut key,
            )
        };
        if status.is_err() {
            bail!(
                "Could not create the Windows startup registry key ({})",
                status.0
            );
        }

        Ok(Some(RegistryKey(key)))
    }

    fn write_string_value(key: &RegistryKey, name: &str, value: &str) -> Result<()> {
        let name = wide(name);
        let value = wide(value);
        let bytes = unsafe {
            std::slice::from_raw_parts(
                value.as_ptr().cast::<u8>(),
                std::mem::size_of_val(value.as_slice()),
            )
        };
        let status = unsafe {
            RegSetValueExW(
                key.0,
                PCWSTR::from_raw(name.as_ptr()),
                None,
                REG_SZ,
                Some(bytes),
            )
        };
        if status.is_err() {
            bail!("Could not add Castle to Windows startup ({})", status.0);
        }

        Ok(())
    }

    fn delete_value(key: &RegistryKey, name: &str) -> Result<()> {
        let name = wide(name);
        let status = unsafe { RegDeleteValueW(key.0, PCWSTR::from_raw(name.as_ptr())) };
        if status.is_err() && status != ERROR_FILE_NOT_FOUND {
            bail!(
                "Could not remove Castle from Windows startup ({})",
                status.0
            );
        }

        Ok(())
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(once(0)).collect()
    }

    struct RegistryKey(HKEY);

    impl Drop for RegistryKey {
        fn drop(&mut self) {
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_command_quotes_the_executable_path() {
        assert_eq!(
            startup_command(Path::new(r"C:\Program Files\Castle\castle.exe")),
            r#""C:\Program Files\Castle\castle.exe" --start-in-tray"#
        );
    }

    #[test]
    fn startup_mode_requires_the_start_in_tray_argument() {
        assert!(starts_in_tray([OsString::from("--start-in-tray")]));
        assert!(!starts_in_tray([OsString::from("--register-mcp")]));
        assert!(!starts_in_tray(std::iter::empty()));
    }
}
