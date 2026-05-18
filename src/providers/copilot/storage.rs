// SPDX-License-Identifier: MPL-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const TOKENS_FILE: &str = "tokens.json";
pub const METADATA_FILE: &str = "metadata.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CopilotTokens {
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CopilotMetadata {
    pub github_user_id: u64,
    pub login: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_authenticated_at: Option<DateTime<Utc>>,
}

pub fn account_id_for_github_user(github_user_id: u64) -> String {
    format!("copilot-{github_user_id}")
}

pub fn write_account(
    account_dir: &Path,
    tokens: &CopilotTokens,
    metadata: &CopilotMetadata,
) -> Result<(), String> {
    create_private_dir(account_dir)?;
    write_json(&account_dir.join(TOKENS_FILE), tokens)?;
    write_json(&account_dir.join(METADATA_FILE), metadata)?;
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load_metadata(account_dir: &Path) -> Result<CopilotMetadata, String> {
    read_json(&account_dir.join(METADATA_FILE))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load_tokens(account_dir: &Path) -> Result<CopilotTokens, String> {
    read_json(&account_dir.join(TOKENS_FILE))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let payload = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode {}: {error}", path.display()))?;
    fs::write(path, payload).map_err(|error| format!("failed to write {}: {error}", path.display()))
}

#[cfg_attr(not(test), allow(dead_code))]
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
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
        .map_err(|error| format!("failed to secure {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn account_dir_under(root: &Path, github_user_id: u64) -> PathBuf {
    root.join(account_id_for_github_user(github_user_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn account_id_uses_github_user_id() {
        assert_eq!(account_id_for_github_user(42), "copilot-42");
    }

    #[test]
    fn write_account_creates_missing_provider_root() {
        let temp = tempdir().unwrap();
        let provider_root = temp.path().join("copilot-accounts");
        assert!(!provider_root.exists());
        let dir = account_dir_under(&provider_root, 42);
        let now = Utc::now();
        let tokens = CopilotTokens {
            access_token: "ghu_test".to_string(),
        };
        let metadata = CopilotMetadata {
            github_user_id: 42,
            login: "octocat".to_string(),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        };
        write_account(&dir, &tokens, &metadata).unwrap();
        assert!(provider_root.is_dir());
        assert!(dir.join(TOKENS_FILE).exists());
        assert!(dir.join(METADATA_FILE).exists());
    }

    #[test]
    fn writes_then_reads_tokens_and_metadata() {
        let temp = tempdir().unwrap();
        let dir = account_dir_under(temp.path(), 7);
        let now = Utc::now();
        let tokens = CopilotTokens {
            access_token: "ghu_test".to_string(),
        };
        let metadata = CopilotMetadata {
            github_user_id: 7,
            login: "octocat".to_string(),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        };
        write_account(&dir, &tokens, &metadata).unwrap();
        assert_eq!(load_tokens(&dir).unwrap(), tokens);
        assert_eq!(load_metadata(&dir).unwrap(), metadata);
        let raw = fs::read_to_string(dir.join(TOKENS_FILE)).unwrap();
        assert!(raw.contains("access_token"));
        assert!(!raw.contains("refresh_token"));
        assert!(!raw.contains("expires_at"));
    }
}
