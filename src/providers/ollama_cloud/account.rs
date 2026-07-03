// SPDX-License-Identifier: MPL-2.0

use crate::config::{Config, ManagedOllamaCloudAccountConfig, managed_ollama_cloud_account_dir};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaCloudAccount {
    pub id: String,
    pub label: String,
    pub config_dir: PathBuf,
}

pub fn discover_accounts(config: &Config) -> Vec<OllamaCloudAccount> {
    let mut accounts = Vec::new();
    for managed in &config.ollama_cloud_managed_accounts {
        let discovered = OllamaCloudAccount {
            id: managed.id.clone(),
            label: managed.label.clone(),
            config_dir: managed_ollama_cloud_account_dir(&managed.id),
        };
        accounts.push(discovered);
    }
    accounts
}

pub fn apply_login_account(config: &mut Config, account: ManagedOllamaCloudAccountConfig) {
    let account_id = account.id.clone();
    config
        .ollama_cloud_managed_accounts
        .retain(|existing| existing.id != account_id);
    config.ollama_cloud_managed_accounts.push(account);
}

pub fn remove_managed_config_dir(config_dir: &PathBuf) {
    let root = managed_ollama_cloud_account_dir("");
    let Some(root) = root.parent() else {
        return;
    };
    let Ok(root) = root.canonicalize() else {
        return;
    };
    let Ok(metadata) = fs::symlink_metadata(config_dir) else {
        return;
    };
    if metadata.file_type().is_symlink() {
        tracing::warn!(path = %config_dir.display(), "refusing to delete symlinked ollama cloud account config dir");
        return;
    }
    let Ok(config_dir) = config_dir.canonicalize() else {
        return;
    };
    if !config_dir.starts_with(&root) {
        tracing::warn!(path = %config_dir.display(), root = %root.display(), "refusing to delete ollama cloud account outside managed root");
        return;
    }
    if let Err(error) = fs::remove_dir_all(&config_dir) {
        tracing::warn!(path = %config_dir.display(), error = %error, "failed to delete ollama cloud account config dir");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ManagedOllamaCloudAccountConfig};
    use chrono::Utc;

    fn dummy_account(id: &str, label: &str) -> ManagedOllamaCloudAccountConfig {
        let now = Utc::now();
        ManagedOllamaCloudAccountConfig {
            id: id.to_string(),
            label: label.to_string(),
            session_cookie_source: "stored".to_string(),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        }
    }

    #[test]
    fn discover_accounts_empty_config_returns_empty() {
        let mut config = Config::default();
        config.ollama_cloud_managed_accounts.clear();
        let accounts = discover_accounts(&config);
        assert!(accounts.is_empty());
    }

    #[test]
    fn discover_accounts_returns_correct_accounts() {
        let mut config = Config::default();
        config.ollama_cloud_managed_accounts = vec![
            dummy_account("alpha", "Alpha Account"),
            dummy_account("beta", "Beta Account"),
        ];
        let accounts = discover_accounts(&config);
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].id, "alpha");
        assert_eq!(accounts[0].label, "Alpha Account");
        assert_eq!(accounts[1].id, "beta");
        assert_eq!(accounts[1].label, "Beta Account");
    }

    #[test]
    fn apply_login_account_adds_new_account() {
        let mut config = Config::default();
        config.ollama_cloud_managed_accounts.clear();
        apply_login_account(&mut config, dummy_account("alpha", "Alpha Account"));
        assert_eq!(config.ollama_cloud_managed_accounts.len(), 1);
        assert_eq!(config.ollama_cloud_managed_accounts[0].id, "alpha");
        assert_eq!(config.ollama_cloud_managed_accounts[0].label, "Alpha Account");
    }

    #[test]
    fn apply_login_account_replaces_existing_same_id() {
        let mut config = Config::default();
        config.ollama_cloud_managed_accounts = vec![dummy_account("alpha", "Old Label")];
        apply_login_account(&mut config, dummy_account("alpha", "New Label"));
        assert_eq!(config.ollama_cloud_managed_accounts.len(), 1);
        assert_eq!(config.ollama_cloud_managed_accounts[0].id, "alpha");
        assert_eq!(config.ollama_cloud_managed_accounts[0].label, "New Label");
    }
}