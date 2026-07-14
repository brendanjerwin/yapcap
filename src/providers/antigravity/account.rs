// SPDX-License-Identifier: MPL-2.0

use crate::account_selection::select_account_after_login;
use crate::config::{Config, ManagedAntigravityAccountConfig, managed_antigravity_account_dir};
use crate::model::ProviderId;
use chrono::Utc;
use std::fs;
use std::path::Path;

pub fn normalized_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

pub fn new_account_id() -> String {
    let millis = Utc::now().timestamp_millis();
    format!("antigravity-{millis}-{}", std::process::id())
}

pub fn create_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    set_private_dir_permissions(path)?;
    Ok(())
}

pub(crate) fn find_matching_account<'a>(
    config: &'a Config,
    email: &str,
) -> Option<&'a ManagedAntigravityAccountConfig> {
    let needle = normalized_email(email);
    config
        .antigravity_managed_accounts
        .iter()
        .find(|account| normalized_email(&account.email) == needle)
}

pub fn apply_login_account(config: &mut Config, account: ManagedAntigravityAccountConfig) {
    let account_id = account.id.clone();
    config
        .antigravity_managed_accounts
        .retain(|existing| existing.id != account_id);
    config.antigravity_managed_accounts.push(account);
    dedupe_managed_accounts(config);
    select_account_after_login(config, ProviderId::Antigravity, account_id);
}

pub fn sync_managed_accounts(config: &mut Config) -> bool {
    let changed = dedupe_managed_accounts(config);
    let dirs_changed = resync_managed_dirs(config);
    changed || dirs_changed
}

fn resync_managed_dirs(config: &mut Config) -> bool {
    let mut changed = false;
    for account in &mut config.antigravity_managed_accounts {
        let canonical = managed_antigravity_account_dir(&account.id);
        if account.account_root != canonical {
            account.account_root = canonical;
            changed = true;
        }
    }
    changed
}

fn dedupe_managed_accounts(config: &mut Config) -> bool {
    let original_selected = config.selected_antigravity_account_ids.clone();
    let mut selected_ids = original_selected.clone();
    let original_len = config.antigravity_managed_accounts.len();
    let mut deduped: Vec<ManagedAntigravityAccountConfig> = Vec::new();

    for account in config.antigravity_managed_accounts.drain(..) {
        let email_key = normalized_email(&account.email);

        if let Some(index) = deduped
            .iter()
            .position(|existing| normalized_email(&existing.email) == email_key)
        {
            let existing = deduped.remove(index);
            let keep_existing = prefer_managed_account(&existing, &account, &original_selected);
            let (mut winner, loser) = if keep_existing {
                (existing, account)
            } else {
                (account, existing)
            };
            let loser_id = loser.id.clone();
            let winner_id = winner.id.clone();
            merge_account_metadata(&mut winner, &loser);
            for id in &mut selected_ids {
                if id == loser_id.as_str() {
                    id.clone_from(&winner_id);
                }
            }
            deduped.push(winner);
            continue;
        }

        deduped.push(account);
    }

    let changed = deduped.len() != original_len || selected_ids != original_selected;
    config.antigravity_managed_accounts = deduped;
    config.selected_antigravity_account_ids = selected_ids;
    changed
}

fn prefer_managed_account(
    existing: &ManagedAntigravityAccountConfig,
    candidate: &ManagedAntigravityAccountConfig,
    selected_ids: &[String],
) -> bool {
    if selected_ids.iter().any(|id| id == existing.id.as_str()) {
        return true;
    }
    if selected_ids.iter().any(|id| id == candidate.id.as_str()) {
        return false;
    }
    let existing_auth = existing.last_authenticated_at.or(Some(existing.updated_at));
    let candidate_auth = candidate
        .last_authenticated_at
        .or(Some(candidate.updated_at));
    existing_auth >= candidate_auth
}

fn merge_account_metadata(
    target: &mut ManagedAntigravityAccountConfig,
    source: &ManagedAntigravityAccountConfig,
) {
    if target.last_tier_id.is_none() {
        target.last_tier_id.clone_from(&source.last_tier_id);
    }
    if source.created_at < target.created_at {
        target.created_at = source.created_at;
    }
    if source.updated_at > target.updated_at {
        target.updated_at = source.updated_at;
    }
    if source.last_authenticated_at > target.last_authenticated_at {
        target.last_authenticated_at = source.last_authenticated_at;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample(id: &str, email: &str) -> ManagedAntigravityAccountConfig {
        let now = Utc::now();
        ManagedAntigravityAccountConfig {
            id: id.to_string(),
            label: email.to_string(),
            account_root: PathBuf::from(format!("/tmp/{id}")),
            email: email.to_string(),
            sub: "sub".to_string(),
            last_tier_id: None,
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        }
    }

    #[test]
    fn normalizes_email_to_trimmed_lowercase() {
        assert_eq!(normalized_email("  Foo@Example.COM "), "foo@example.com");
    }

    #[test]
    fn dedupes_by_normalized_email() {
        let mut config = Config {
            selected_antigravity_account_ids: vec!["antigravity-b".to_string()],
            antigravity_managed_accounts: vec![
                sample("antigravity-a", "user@example.com"),
                sample("antigravity-b", "USER@example.com"),
            ],
            ..Config::default()
        };
        let changed = dedupe_managed_accounts(&mut config);
        assert!(changed);
        assert_eq!(config.antigravity_managed_accounts.len(), 1);
        assert_eq!(config.antigravity_managed_accounts[0].id, "antigravity-b");
        assert_eq!(
            config.selected_antigravity_account_ids,
            vec!["antigravity-b"]
        );
    }

    #[test]
    fn apply_login_inserts_and_selects() {
        let mut config = Config::default();
        apply_login_account(&mut config, sample("antigravity-1", "user@example.com"));
        assert_eq!(config.antigravity_managed_accounts.len(), 1);
        assert_eq!(
            config.selected_antigravity_account_ids,
            vec!["antigravity-1"]
        );
    }
}
