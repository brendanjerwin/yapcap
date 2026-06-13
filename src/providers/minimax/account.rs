// SPDX-License-Identifier: MPL-2.0

use crate::config::{Config, ManagedMinimaxAccountConfig, managed_minimax_account_dir};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimaxAccount {
    pub id: String,
    pub label: String,
    pub config_dir: PathBuf,
}

pub fn discover_accounts(config: &Config) -> Vec<MinimaxAccount> {
    let mut accounts = Vec::new();
    for managed in &config.minimax_managed_accounts {
        let discovered = MinimaxAccount {
            id: managed.id.clone(),
            label: managed.label.clone(),
            config_dir: managed_minimax_account_dir(&managed.id),
        };
        accounts.push(discovered);
    }
    accounts
}

pub fn apply_login_account(config: &mut Config, account: ManagedMinimaxAccountConfig) {
    let account_id = account.id.clone();
    config
        .minimax_managed_accounts
        .retain(|existing| existing.id != account_id);
    config.minimax_managed_accounts.push(account);
}

pub fn remove_managed_config_dir(config_dir: &PathBuf) {
    let root = managed_minimax_account_dir("");
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
        tracing::warn!(path = %config_dir.display(), "refusing to delete symlinked minimax account config dir");
        return;
    }
    let Ok(config_dir) = config_dir.canonicalize() else {
        return;
    };
    if !config_dir.starts_with(&root) {
        tracing::warn!(path = %config_dir.display(), root = %root.display(), "refusing to delete minimax account outside managed root");
        return;
    }
    if let Err(error) = fs::remove_dir_all(&config_dir) {
        tracing::warn!(path = %config_dir.display(), error = %error, "failed to delete minimax account config dir");
    }
}
