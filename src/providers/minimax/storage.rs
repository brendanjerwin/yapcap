// SPDX-License-Identifier: MPL-2.0

use std::fs;
use std::path::Path;

pub const API_KEY_FILE: &str = "api_key.txt";

pub fn write_api_key(account_dir: &Path, api_key: &str) -> Result<(), String> {
    create_private_dir(account_dir)?;
    fs::write(account_dir.join(API_KEY_FILE), api_key)
        .map_err(|error| format!("failed to write api key: {error}"))?;
    set_private_file_permissions(&account_dir.join(API_KEY_FILE))?;
    Ok(())
}

pub fn load_api_key(account_dir: &Path) -> Result<String, String> {
    fs::read_to_string(account_dir.join(API_KEY_FILE))
        .map_err(|error| format!("failed to read api key: {error}"))
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
