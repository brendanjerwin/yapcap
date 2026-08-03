// SPDX-License-Identifier: MPL-2.0

use crate::account_storage;
use std::fs;
use std::path::Path;

pub const API_KEY_FILE: &str = "api_key.txt";

pub fn write_api_key(account_dir: &Path, api_key: &str) -> Result<(), String> {
    create_private_dir(account_dir)?;
    let path = account_dir.join(API_KEY_FILE);
    fs::write(&path, api_key).map_err(|error| format!("failed to write api key: {error}"))?;
    account_storage::set_private_file_permissions(&path).map_err(stringify)?;
    Ok(())
}

pub fn load_api_key(account_dir: &Path) -> Result<String, String> {
    fs::read_to_string(account_dir.join(API_KEY_FILE))
        .map_err(|error| format!("failed to read api key: {error}"))
}

pub fn create_private_dir(path: &Path) -> Result<(), String> {
    account_storage::create_private_dir(path).map_err(stringify)
}

fn stringify(error: account_storage::AccountStorageError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_api_key_creates_missing_provider_root() {
        let temp = tempdir().unwrap();
        let provider_root = temp.path().join("minimax-accounts");
        assert!(!provider_root.exists());
        let dir = provider_root.join("minimax-1");

        write_api_key(&dir, "sk-test").unwrap();

        assert!(provider_root.is_dir());
        assert_eq!(load_api_key(&dir).unwrap(), "sk-test");
    }

    #[cfg(unix)]
    #[test]
    fn write_api_key_sets_private_directory_and_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let dir = temp.path().join("minimax-1");

        write_api_key(&dir, "sk-test").unwrap();

        let dir_mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
        let file_mode = fs::metadata(dir.join(API_KEY_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600);
    }
}
