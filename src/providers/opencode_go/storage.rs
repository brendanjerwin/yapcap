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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "yapcap-test-{}-{}-{}",
            "opencode_go_storage",
            label,
            std::process::id()
        ))
    }

    #[test]
    fn workspace_id_round_trip() {
        let dir = temp_dir("workspace_id");
        let _ = std::fs::remove_dir_all(&dir);

        write_workspace_id(&dir, "ws-12345").unwrap();
        let loaded = load_workspace_id(&dir).unwrap();
        assert_eq!(loaded, "ws-12345");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auth_cookie_round_trip() {
        let dir = temp_dir("auth_cookie");
        let _ = std::fs::remove_dir_all(&dir);

        write_auth_cookie(&dir, "cookie-secret-value").unwrap();
        let loaded = load_auth_cookie(&dir).unwrap();
        assert_eq!(loaded, "cookie-secret-value");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_private_dir_creates_directory() {
        let dir = temp_dir("create_private_dir");
        let _ = std::fs::remove_dir_all(&dir);

        create_private_dir(&dir).unwrap();
        assert!(dir.exists());
        assert!(dir.is_dir());

        let _ = std::fs::remove_dir_all(&dir);
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
    fn write_workspace_id_creates_file_with_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("write_workspace_id_perms");
        let _ = std::fs::remove_dir_all(&dir);

        write_workspace_id(&dir, "ws-perms-test").expect("write should succeed");
        let file = dir.join(WORKSPACE_ID_FILE);
        assert!(file.exists(), "workspace id file should exist");
        assert_eq!(
            std::fs::read_to_string(&file).expect("read"),
            "ws-perms-test"
        );

        let mode = std::fs::metadata(&file)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "file should be 0o600, got {:o}", mode);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn write_auth_cookie_creates_file_with_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("write_auth_cookie_perms");
        let _ = std::fs::remove_dir_all(&dir);

        write_auth_cookie(&dir, "secret-cookie-value").expect("write should succeed");
        let file = dir.join(AUTH_COOKIE_FILE);
        assert!(file.exists(), "auth cookie file should exist");
        assert_eq!(
            std::fs::read_to_string(&file).expect("read"),
            "secret-cookie-value"
        );

        let mode = std::fs::metadata(&file)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "file should be 0o600, got {:o}", mode);

        let _ = std::fs::remove_dir_all(&dir);
    }
}