use super::*;
use crate::config::Config;
use crate::config::ProviderVisibilityMode;

#[test]
fn providers_expose_expected_capabilities() {
    assert_eq!(
        capabilities(ProviderId::Codex),
        ProviderCapabilities {
            supports_delete: true,
            supports_reauthentication: false,
            supports_background_status_refresh: false,
            requires_auth_prompt_on_auth_failure: false,
        }
    );
    assert_eq!(
        capabilities(ProviderId::Claude),
        ProviderCapabilities {
            supports_delete: true,
            supports_reauthentication: true,
            supports_background_status_refresh: false,
            requires_auth_prompt_on_auth_failure: false,
        }
    );
    assert_eq!(
        capabilities(ProviderId::Cursor),
        ProviderCapabilities {
            supports_delete: true,
            supports_reauthentication: true,
            supports_background_status_refresh: true,
            requires_auth_prompt_on_auth_failure: true,
        }
    );
}

#[test]
fn cursor_supports_background_status_refresh() {
    assert!(supports_background_status_refresh(ProviderId::Cursor));
    assert!(!supports_background_status_refresh(ProviderId::Codex));
    assert!(!supports_background_status_refresh(ProviderId::Claude));
}

#[test]
fn cursor_requires_reauth_prompt_on_auth_error() {
    assert!(auth_error_requires_reauth_prompt(ProviderId::Cursor));
    assert!(!auth_error_requires_reauth_prompt(ProviderId::Codex));
    assert!(!auth_error_requires_reauth_prompt(ProviderId::Claude));
}

#[test]
fn each_provider_resolves_accounts() {
    let config = Config::default();
    for provider in ProviderId::ALL {
        let accounts = discover_accounts(provider, &config);
        assert!(
            accounts.is_empty(),
            "default config should have no accounts for {provider:?}"
        );
    }
}

#[test]
fn initialize_provider_visibility_enables_provider_regardless_of_accounts() {
    let mut config = Config {
        cursor_enabled: false,
        ..Config::default()
    };
    assert!(initialize_provider_visibility(
        &mut config,
        &[ProviderId::Cursor]
    ));
    assert!(config.cursor_enabled);
    assert_eq!(
        config.provider_visibility_mode,
        ProviderVisibilityMode::AutoInitPending
    );
}

#[test]
fn initialize_provider_visibility_is_noop_after_initialization() {
    let mut config = Config {
        provider_visibility_mode: ProviderVisibilityMode::UserManaged,
        ..Config::default()
    };

    assert!(!initialize_provider_visibility(
        &mut config,
        &[ProviderId::Codex, ProviderId::Claude, ProviderId::Cursor]
    ));
    assert!(config.codex_enabled);
    assert!(config.claude_enabled);
    assert!(config.cursor_enabled);
}

#[test]
fn action_support_matches_capabilities() {
    let support = capabilities(ProviderId::Cursor).action_support();
    assert!(support.can_delete);
    assert!(support.can_reauthenticate);
    assert!(support.supports_background_status_refresh);
}

#[test]
fn system_active_account_id_only_supported_by_codex_claude_gemini_minimax() {
    use crate::account_storage::{
        NewProviderAccount, ProviderAccountStorage, ProviderAccountTokens,
    };
    use crate::config::{
        ManagedClaudeAccountConfig, ManagedCodexAccountConfig, ManagedGeminiAccountConfig,
        ManagedMinimaxAccountConfig, paths,
    };
    use chrono::Utc;
    use std::fs;
    use std::path::PathBuf;

    let mut env = crate::test_support::test_env();
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let state = temp.path().join("state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&state).unwrap();
    env.set("HOME", &home);
    env.set("XDG_STATE_HOME", &state);
    env.set("MINIMAX_API_KEY", "test-minimax-key");
    env.remove("FLATPAK_ID");

    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    let id_token = "eyJhbGciOiJSUzI1NiJ9.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOiB7ImNoYXRncHRfdXNlcl9pZCI6ICJ1c2VyLWFiYy0xMjMifX0.fakesig";
    fs::write(
        codex_dir.join("auth.json"),
        format!(r#"{{"tokens":{{"id_token":"{id_token}"}}}}"#),
    )
    .unwrap();

    let gemini_dir = home.join(".gemini");
    fs::create_dir_all(&gemini_dir).unwrap();
    fs::write(
        gemini_dir.join("google_accounts.json"),
        r#"{"active":"alice@example.com"}"#,
    )
    .unwrap();

    let storage = ProviderAccountStorage::new(paths().claude_accounts_dir.clone());
    let stored = storage
        .create_account(NewProviderAccount {
            provider: ProviderId::Claude,
            email: "claude@example.com".to_string(),
            provider_account_id: Some("acct-uuid".to_string()),
            organization_id: None,
            organization_name: None,
            tokens: ProviderAccountTokens {
                access_token: "a".to_string(),
                refresh_token: "r".to_string(),
                expires_at: Utc::now(),
                scope: vec![],
                token_id: None,
            },
            snapshot: None,
        })
        .unwrap();
    fs::write(
        home.join(".claude.json"),
        r#"{"oauthAccount":{"accountUuid":"acct-uuid"}}"#,
    )
    .unwrap();

    let config = Config {
        codex_managed_accounts: vec![ManagedCodexAccountConfig {
            id: "codex-1".to_string(),
            label: "Codex".to_string(),
            codex_home: PathBuf::from("/tmp"),
            email: None,
            provider_account_id: Some("user-abc-123".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_authenticated_at: None,
        }],
        claude_managed_accounts: vec![ManagedClaudeAccountConfig {
            id: stored.metadata.account_id.clone(),
            label: "Claude".to_string(),
            config_dir: paths()
                .claude_accounts_dir
                .join(&stored.metadata.account_id),
            email: Some("claude@example.com".to_string()),
            organization: None,
            subscription_type: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_authenticated_at: None,
        }],
        gemini_managed_accounts: vec![ManagedGeminiAccountConfig {
            id: "gemini-1".to_string(),
            label: "Gemini".to_string(),
            account_root: PathBuf::from("/tmp/gemini-1"),
            email: "alice@example.com".to_string(),
            sub: "sub".to_string(),
            hd: None,
            last_tier_id: None,
            last_cloudaicompanion_project: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_authenticated_at: None,
        }],
        minimax_managed_accounts: vec![ManagedMinimaxAccountConfig {
            id: "minimax-1".to_string(),
            label: "Minimax".to_string(),
            api_key_source: "env:MINIMAX_API_KEY".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_authenticated_at: None,
        }],
        ..Config::default()
    };

    let expectations = [
        (ProviderId::Codex, true),
        (ProviderId::Claude, true),
        (ProviderId::Gemini, true),
        (ProviderId::Cursor, false),
        (ProviderId::Copilot, false),
        (ProviderId::Minimax, true),
    ];
    for (provider, expect_some) in expectations {
        let result = system_active_account_id(provider, &config);
        assert_eq!(
            result.is_some(),
            expect_some,
            "provider {provider:?} expected has_active={expect_some}, got {result:?}"
        );
    }
}
