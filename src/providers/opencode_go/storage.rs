// SPDX-License-Identifier: MPL-2.0

use std::fs;
use std::path::Path;

pub const WORKSPACE_ID_FILE: &str = "workspace_id.txt";
pub const AUTH_COOKIE_FILE: &str = "auth_cookie.txt";

pub fn write_workspace_id(account_dir: &Path, workspace_id: &str) -> Result<(), String> {
    create_private_dir(account_dir)?;
    fs::write(account_dir.join(WORKSPACE_ID_FILE), workspace_id)
        .map_err(|error| format!("failed to write workspace id: {error}"))?;
    set_private_file_permissions(&account_dir.join(WORKSPACE_ID_FILE))?;
    Ok(())
}

pub fn write_auth_cookie(account_dir: &Path, auth_cookie: &str) -> Result<(), String> {
    create_private_dir(account_dir)?;
    fs::write(account_dir.join(AUTH_COOKIE_FILE), auth_cookie)
        .map_err(|error| format!("failed to write auth cookie: {error}"))?;
    set_private_file_permissions(&account_dir.join(AUTH_COOKIE_FILE))?;
    Ok(())
}

pub fn load_workspace_id(account_dir: &Path) -> Result<String, String> {
    fs::read_to_string(account_dir.join(WORKSPACE_ID_FILE))
        .map_err(|error| format!("failed to read workspace id: {error}"))
}

pub fn load_auth_cookie(account_dir: &Path) -> Result<String, String> {
    fs::read_to_string(account_dir.join(AUTH_COOKIE_FILE))
        .map_err(|error| format!("failed to read auth cookie: {error}"))
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