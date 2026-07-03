// SPDX-License-Identifier: MPL-2.0

use std::fs;
use std::path::Path;

pub const SESSION_COOKIE_FILE: &str = "session_cookie.txt";

pub fn write_session_cookie(account_dir: &Path, session_cookie: &str) -> Result<(), String> {
    create_private_dir(account_dir)?;
    fs::write(account_dir.join(SESSION_COOKIE_FILE), session_cookie)
        .map_err(|error| format!("failed to write session cookie: {error}"))?;
    set_private_file_permissions(&account_dir.join(SESSION_COOKIE_FILE))?;
    Ok(())
}

pub fn load_session_cookie(account_dir: &Path) -> Result<String, String> {
    fs::read_to_string(account_dir.join(SESSION_COOKIE_FILE))
        .map_err(|error| format!("failed to read session cookie: {error}"))
}

pub fn create_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    set_private_dir_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("failed to set permissions on {}: {error}", path.display()))
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to set permissions on {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}