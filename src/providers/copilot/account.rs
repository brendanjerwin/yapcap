// SPDX-License-Identifier: MPL-2.0

use crate::config::{Config, ManagedCopilotAccountConfig, managed_copilot_account_dir};
use chrono::{DateTime, Utc};
use std::path::PathBuf;

pub fn find_matching_account(
    config: &Config,
    github_user_id: u64,
) -> Option<&ManagedCopilotAccountConfig> {
    config
        .copilot_managed_accounts
        .iter()
        .find(|account| account.github_user_id == github_user_id)
}

pub fn apply_login_account(config: &mut Config, account: ManagedCopilotAccountConfig) {
    let account_id = account.id.clone();
    let github_user_id = account.github_user_id;
    if !config.selected_copilot_account_ids.contains(&account_id) {
        config.selected_copilot_account_ids.push(account_id.clone());
    }
    config
        .copilot_managed_accounts
        .retain(|existing| existing.id != account_id && existing.github_user_id != github_user_id);
    config.copilot_managed_accounts.push(account);
}

pub struct ResolvedAccount {
    pub id: String,
    pub account_dir: PathBuf,
    pub created_at: DateTime<Utc>,
}

pub fn resolve_account_target(
    config: &Config,
    github_user_id: u64,
    now: DateTime<Utc>,
) -> ResolvedAccount {
    let id = crate::providers::copilot::storage::account_id_for_github_user(github_user_id);
    let account_dir = managed_copilot_account_dir(&id);
    let existing = find_matching_account(config, github_user_id);
    ResolvedAccount {
        id,
        account_dir,
        created_at: existing.map_or(now, |account| account.created_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, github_user_id: u64, login: &str) -> ManagedCopilotAccountConfig {
        let now = Utc::now();
        ManagedCopilotAccountConfig {
            id: id.to_string(),
            label: login.to_string(),
            github_user_id,
            login: login.to_string(),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        }
    }

    #[test]
    fn finds_existing_account_by_github_user_id() {
        let config = Config {
            copilot_managed_accounts: vec![sample("copilot-1", 1, "octocat")],
            ..Config::default()
        };
        assert_eq!(
            find_matching_account(&config, 1).map(|a| a.login.as_str()),
            Some("octocat")
        );
        assert!(find_matching_account(&config, 999).is_none());
    }

    #[test]
    fn apply_login_replaces_existing_match_by_id_or_user() {
        let mut config = Config {
            copilot_managed_accounts: vec![sample("copilot-1", 1, "old-login")],
            ..Config::default()
        };
        apply_login_account(&mut config, sample("copilot-1", 1, "new-login"));
        assert_eq!(config.copilot_managed_accounts.len(), 1);
        assert_eq!(config.copilot_managed_accounts[0].login, "new-login");
        assert_eq!(config.selected_copilot_account_ids, vec!["copilot-1"]);
    }

    #[test]
    fn apply_login_appends_new_account() {
        let mut config = Config {
            copilot_managed_accounts: vec![sample("copilot-1", 1, "alice")],
            selected_copilot_account_ids: vec!["copilot-1".to_string()],
            ..Config::default()
        };
        apply_login_account(&mut config, sample("copilot-2", 2, "bob"));
        assert_eq!(config.copilot_managed_accounts.len(), 2);
        assert_eq!(
            config.selected_copilot_account_ids,
            vec!["copilot-1".to_string(), "copilot-2".to_string()]
        );
    }
}
