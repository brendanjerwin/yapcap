// SPDX-License-Identifier: MPL-2.0

use crate::config::Config;
use crate::model::ProviderId;
use crate::providers::cursor;

pub const MAX_MULTI_ACCOUNT_SELECTION: usize = 4;

pub fn select_account_after_login(config: &mut Config, provider: ProviderId, account_id: String) {
    let show_all = config.show_all_accounts(provider);
    let ids = config.selected_account_ids_mut(provider);
    if !show_all {
        ids.clear();
        ids.push(account_id);
        return;
    }

    if !ids.contains(&account_id) && ids.len() < MAX_MULTI_ACCOUNT_SELECTION {
        ids.push(account_id);
    }
    ids.truncate(MAX_MULTI_ACCOUNT_SELECTION);
}

#[must_use]
pub fn provider_show_all_account_selection(config: &Config, provider: ProviderId) -> Vec<String> {
    let available_ids: Vec<String> = match provider {
        ProviderId::Codex => config
            .codex_managed_accounts
            .iter()
            .map(|a| a.id.clone())
            .collect(),
        ProviderId::Claude => config
            .claude_managed_accounts
            .iter()
            .map(|a| a.id.clone())
            .collect(),
        ProviderId::Cursor => config
            .cursor_managed_accounts
            .iter()
            .map(|a| cursor::managed_account_id(&a.id))
            .collect(),
        ProviderId::Gemini => config
            .gemini_managed_accounts
            .iter()
            .map(|a| a.id.clone())
            .collect(),
        ProviderId::Copilot => config
            .copilot_managed_accounts
            .iter()
            .map(|a| a.id.clone())
            .collect(),
    };
    let active_id = config
        .selected_account_ids(provider)
        .first()
        .map(String::as_str);

    show_all_account_selection(&available_ids, active_id)
}

#[must_use]
fn show_all_account_selection(available_ids: &[String], active_id: Option<&str>) -> Vec<String> {
    let active_id = active_id.filter(|id| available_ids.iter().any(|available| available == id));
    let mut selected = Vec::with_capacity(available_ids.len().min(MAX_MULTI_ACCOUNT_SELECTION));

    if let Some(id) = active_id {
        selected.push(id.to_string());
    }

    for id in available_ids {
        if selected.len() >= MAX_MULTI_ACCOUNT_SELECTION {
            break;
        }
        if active_id == Some(id.as_str()) {
            continue;
        }
        selected.push(id.clone());
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ManagedCodexAccountConfig;
    use chrono::Utc;
    use std::path::PathBuf;

    #[test]
    fn show_all_selection_keeps_active_account_then_fills_to_cap() {
        let available_ids = ["one", "two", "three", "four", "five"]
            .map(str::to_string)
            .to_vec();

        let selected = show_all_account_selection(&available_ids, Some("five"));

        assert_eq!(selected, ["five", "one", "two", "three"]);
    }

    #[test]
    fn show_all_selection_keeps_all_accounts_when_at_cap() {
        let available_ids = ["one", "two", "three", "four"].map(str::to_string).to_vec();

        let selected = show_all_account_selection(&available_ids, Some("two"));

        assert_eq!(selected, ["two", "one", "three", "four"]);
    }

    #[test]
    fn show_all_selection_uses_stable_order_without_available_active_account() {
        let available_ids = ["one", "two", "three", "four", "five"]
            .map(str::to_string)
            .to_vec();

        let selected = show_all_account_selection(&available_ids, Some("missing"));

        assert_eq!(selected, ["one", "two", "three", "four"]);
    }

    #[test]
    fn codex_show_all_selection_keeps_active_account_then_caps_to_four() {
        let mut config = Config {
            codex_managed_accounts: (1..=5).map(codex_account).collect(),
            selected_codex_account_ids: vec!["codex-5".to_string()],
            ..Config::default()
        };
        config.set_provider_show_all(ProviderId::Codex, true);

        let selected = provider_show_all_account_selection(&config, ProviderId::Codex);

        assert_eq!(selected, ["codex-5", "codex-1", "codex-2", "codex-3"]);
        assert_eq!(config.codex_managed_accounts.len(), 5);
    }

    #[test]
    fn login_selection_replaces_selection_when_show_all_is_off() {
        let mut config = Config {
            selected_copilot_account_ids: vec!["copilot-1".to_string()],
            ..Config::default()
        };

        select_account_after_login(&mut config, ProviderId::Copilot, "copilot-2".to_string());

        assert_eq!(config.selected_copilot_account_ids, ["copilot-2"]);
    }

    #[test]
    fn login_selection_appends_when_show_all_is_on() {
        let mut config = Config {
            selected_copilot_account_ids: vec!["copilot-1".to_string()],
            ..Config::default()
        };
        config.set_provider_show_all(ProviderId::Copilot, true);

        select_account_after_login(&mut config, ProviderId::Copilot, "copilot-2".to_string());

        assert_eq!(
            config.selected_copilot_account_ids,
            ["copilot-1", "copilot-2"]
        );
    }

    #[test]
    fn login_selection_caps_show_all_selection() {
        let mut config = Config {
            selected_copilot_account_ids: ["copilot-1", "copilot-2", "copilot-3", "copilot-4"]
                .map(str::to_string)
                .to_vec(),
            ..Config::default()
        };
        config.set_provider_show_all(ProviderId::Copilot, true);

        select_account_after_login(&mut config, ProviderId::Copilot, "copilot-5".to_string());

        assert_eq!(
            config.selected_copilot_account_ids,
            ["copilot-1", "copilot-2", "copilot-3", "copilot-4"]
        );
    }

    fn codex_account(index: usize) -> ManagedCodexAccountConfig {
        let id = format!("codex-{index}");
        ManagedCodexAccountConfig {
            id: id.clone(),
            label: id,
            codex_home: PathBuf::from(format!("/tmp/yapcap/codex-{index}")),
            email: Some(format!("user{index}@example.com")),
            provider_account_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_authenticated_at: None,
        }
    }
}
