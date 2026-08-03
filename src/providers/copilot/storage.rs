// SPDX-License-Identifier: MPL-2.0

use crate::account_storage;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    account_storage::write_json(&account_dir.join(TOKENS_FILE), tokens).map_err(stringify)?;
    account_storage::write_json(&account_dir.join(METADATA_FILE), metadata).map_err(stringify)?;
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load_metadata(account_dir: &Path) -> Result<CopilotMetadata, String> {
    account_storage::read_json(&account_dir.join(METADATA_FILE)).map_err(stringify)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load_tokens(account_dir: &Path) -> Result<CopilotTokens, String> {
    account_storage::read_json(&account_dir.join(TOKENS_FILE)).map_err(stringify)
}

pub fn create_private_dir(path: &Path) -> Result<(), String> {
    account_storage::create_private_dir(path).map_err(stringify)
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn account_dir_under(root: &Path, github_user_id: u64) -> PathBuf {
    root.join(account_id_for_github_user(github_user_id))
}

fn stringify(error: account_storage::AccountStorageError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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

    #[cfg(unix)]
    #[test]
    fn write_account_sets_private_directory_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let dir = account_dir_under(temp.path(), 9);
        let now = Utc::now();
        write_account(
            &dir,
            &CopilotTokens {
                access_token: "ghu_test".to_string(),
            },
            &CopilotMetadata {
                github_user_id: 9,
                login: "octocat".to_string(),
                created_at: now,
                updated_at: now,
                last_authenticated_at: Some(now),
            },
        )
        .unwrap();

        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
}
