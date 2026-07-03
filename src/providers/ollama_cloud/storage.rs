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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("yapcap-test-{}-{}-{}", "ollama_storage", label, std::process::id()))
    }

    #[test]
    fn write_and_load_session_cookie_round_trip() {
        let dir = temp_dir("session_cookie");
        // ensure clean slate
        let _ = std::fs::remove_dir_all(&dir);

        let cookie = "secret-session-cookie-value";
        write_session_cookie(&dir, cookie).expect("write should succeed");
        let loaded = load_session_cookie(&dir).expect("load should succeed");
        assert_eq!(loaded, cookie);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn create_private_dir_creates_existing_directory() {
        let dir = temp_dir("create_private_dir");
        let _ = std::fs::remove_dir_all(&dir);

        create_private_dir(&dir).expect("create should succeed");
        assert!(dir.exists(), "directory should exist after create_private_dir");
        assert!(dir.is_dir(), "path should be a directory");

        // creating again should not error (idempotent)
        create_private_dir(&dir).expect("redundant create should succeed");

        std::fs::remove_dir_all(&dir).ok();
    }
    #[cfg(unix)]
    #[test]
    fn create_private_dir_results_in_existing_private_directory() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("create_private_dir_perms");
        let _ = std::fs::remove_dir_all(&dir);

        create_private_dir(&dir).expect("create should succeed");
        assert!(dir.exists(), "directory should exist after create_private_dir");
        assert!(dir.is_dir(), "path should be a directory");

        let mode = std::fs::metadata(&dir)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "dir should be 0o700, got {:o}", mode);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn write_session_cookie_creates_file_with_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("write_session_cookie_perms");
        let _ = std::fs::remove_dir_all(&dir);

        write_session_cookie(&dir, "secret-session-value").expect("write should succeed");
        let file = dir.join(SESSION_COOKIE_FILE);
        assert!(file.exists(), "session cookie file should exist");
        assert_eq!(
            std::fs::read_to_string(&file).expect("read"),
            "secret-session-value"
        );

        let mode = std::fs::metadata(&file)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "file should be 0o600, got {:o}", mode);

        let _ = std::fs::remove_dir_all(&dir);
    }
}