// SPDX-License-Identifier: MPL-2.0

use crate::config::{
    Config, ManagedAntigravityAccountConfig, ManagedClaudeAccountConfig, ManagedCodexAccountConfig,
    ManagedCopilotAccountConfig, ManagedCursorAccountConfig, ManagedGeminiAccountConfig,
    ManagedMinimaxAccountConfig, ProviderEnablement, ProviderVisibilityMode, paths,
};
use crate::model::{
    AccountSelectionStatus, AppState, AuthState, ExtraUsageState, ProviderAccountRuntimeState,
    ProviderCost, ProviderHealth, ProviderId, ProviderIdentity, ProviderRuntimeState,
    UsageHeadline, UsageSnapshot, UsageWindow,
};
use chrono::{DateTime, Duration, Utc};
use std::path::PathBuf;
use std::sync::Once;

const DEMO_ENV: &str = "YAPCAP_DEMO";
const DEMO_ID_PREFIX: &str = "yapcap-demo:";
const CODEX_PRIMARY_ID: &str = "yapcap-demo:codex-primary";
const CODEX_SECONDARY_ID: &str = "yapcap-demo:codex-secondary";
const CODEX_PRO_ID: &str = "yapcap-demo:codex-pro";
const CLAUDE_PRIMARY_ID: &str = "yapcap-demo:claude-primary";
const CLAUDE_MAX_ID: &str = "yapcap-demo:claude-max";
const CURSOR_PRIMARY_ID: &str = "yapcap-demo:cursor-primary";
const GEMINI_PRIMARY_ID: &str = "yapcap-demo:gemini-primary";
const COPILOT_FREE_ID: &str = "yapcap-demo:copilot-casey-free";
const COPILOT_PRO_ID: &str = "yapcap-demo:copilot-morgan-pro";
const MINIMAX_PRIMARY_ID: &str = "yapcap-demo:minimax-primary";
const ANTIGRAVITY_PRIMARY_ID: &str = "yapcap-demo:antigravity-primary";
const ANTIGRAVITY_FREE_ID: &str = "yapcap-demo:antigravity-free";

fn env_truthy() -> bool {
    std::env::var(DEMO_ENV).is_ok_and(|value| {
        let value = value.trim();
        !(value == "0"
            || value.eq_ignore_ascii_case("false")
            || value.eq_ignore_ascii_case("no")
            || value.eq_ignore_ascii_case("off"))
    })
}

pub fn is_active() -> bool {
    if !cfg!(debug_assertions) {
        return false;
    }
    std::env::var(DEMO_ENV).is_ok() && env_truthy()
}

pub fn detection_snapshot() -> crate::detection::DetectionSnapshot {
    crate::detection::DetectionSnapshot::default()
}

pub fn apply_config(config: &mut Config) {
    if !is_active() {
        return;
    }

    config.codex_enablement = ProviderEnablement::Enabled;
    config.claude_enablement = ProviderEnablement::Enabled;
    config.cursor_enablement = ProviderEnablement::Enabled;
    config.gemini_enablement = ProviderEnablement::Enabled;
    config.copilot_enablement = ProviderEnablement::Enabled;
    config.minimax_enablement = ProviderEnablement::Enabled;
    config.antigravity_enablement = ProviderEnablement::Enabled;

    config.codex_managed_accounts = demo_codex_accounts();
    config.claude_managed_accounts = demo_claude_accounts();
    config.cursor_managed_accounts = demo_cursor_accounts();
    config.gemini_managed_accounts = demo_gemini_accounts();
    config.copilot_managed_accounts = demo_copilot_accounts();
    config.minimax_managed_accounts = demo_minimax_accounts();
    config.antigravity_managed_accounts = demo_antigravity_accounts();

    config.provider_visibility_mode = ProviderVisibilityMode::UserManaged;

    config.selected_codex_account_ids = vec![
        CODEX_PRIMARY_ID.to_string(),
        CODEX_SECONDARY_ID.to_string(),
        CODEX_PRO_ID.to_string(),
    ];
    config.set_provider_show_all(ProviderId::Codex, true);
    config.selected_claude_account_ids =
        vec![CLAUDE_PRIMARY_ID.to_string(), CLAUDE_MAX_ID.to_string()];
    config.set_provider_show_all(ProviderId::Claude, true);
    config.selected_cursor_account_ids = vec![CURSOR_PRIMARY_ID.to_string()];
    config.set_provider_show_all(ProviderId::Cursor, false);
    config.selected_gemini_account_ids = vec![GEMINI_PRIMARY_ID.to_string()];
    config.set_provider_show_all(ProviderId::Gemini, false);
    config.selected_copilot_account_ids =
        vec![COPILOT_FREE_ID.to_string(), COPILOT_PRO_ID.to_string()];
    config.set_provider_show_all(ProviderId::Copilot, true);
    config.selected_minimax_account_ids = vec![MINIMAX_PRIMARY_ID.to_string()];
    config.set_provider_show_all(ProviderId::Minimax, false);
    config.selected_antigravity_account_ids = vec![
        ANTIGRAVITY_PRIMARY_ID.to_string(),
        ANTIGRAVITY_FREE_ID.to_string(),
    ];
    config.set_provider_show_all(ProviderId::Antigravity, true);
}

pub fn strip_leaked_state(config: &mut Config) -> bool {
    let mut changed = strip_ids(&mut config.selected_codex_account_ids);
    changed |= retain_len_changed(&mut config.codex_managed_accounts, |account| &account.id);
    changed |= strip_ids(&mut config.selected_claude_account_ids);
    changed |= retain_len_changed(&mut config.claude_managed_accounts, |account| &account.id);
    changed |= strip_ids(&mut config.selected_cursor_account_ids);
    changed |= retain_len_changed(&mut config.cursor_managed_accounts, |account| &account.id);
    changed |= strip_ids(&mut config.selected_gemini_account_ids);
    changed |= retain_len_changed(&mut config.gemini_managed_accounts, |account| &account.id);
    changed |= strip_ids(&mut config.selected_copilot_account_ids);
    changed |= retain_len_changed(&mut config.copilot_managed_accounts, |account| &account.id);
    changed |= strip_ids(&mut config.selected_minimax_account_ids);
    changed |= retain_len_changed(&mut config.minimax_managed_accounts, |account| &account.id);
    changed |= strip_ids(&mut config.selected_antigravity_account_ids);
    changed |= retain_len_changed(&mut config.antigravity_managed_accounts, |account| {
        &account.id
    });
    changed
}

fn strip_ids(ids: &mut Vec<String>) -> bool {
    let before = ids.len();
    ids.retain(|id| !id.starts_with(DEMO_ID_PREFIX));
    ids.len() != before
}

fn retain_len_changed<T>(accounts: &mut Vec<T>, id_of: impl Fn(&T) -> &String) -> bool {
    let before = accounts.len();
    accounts.retain(|account| !id_of(account).starts_with(DEMO_ID_PREFIX));
    accounts.len() != before
}

pub fn apply(config: &Config, state: &mut AppState) {
    if !is_active() {
        return;
    }
    state.provider_accounts.clear();
    for provider in ProviderId::ALL {
        if !state.provider(provider).is_some_and(|entry| entry.enabled) {
            state.upsert_provider(ProviderRuntimeState::disabled(provider));
            continue;
        }
        let runtime_accounts = demo_runtime_accounts(provider);
        let has_accounts = !runtime_accounts.is_empty();
        for account in runtime_accounts {
            state.upsert_account(account);
        }
        state.upsert_provider(ProviderRuntimeState {
            provider,
            enabled: true,
            selected_account_ids: config.selected_account_ids(provider).to_vec(),
            active_account_id: config.selected_account_ids(provider).first().cloned(),
            system_active_account_id: demo_system_active_account_id(provider),
            account_status: if has_accounts {
                AccountSelectionStatus::Ready
            } else {
                AccountSelectionStatus::LoginRequired
            },
            is_refreshing: false,
            refresh_started_at: None,
            legacy_display_snapshot: None,
            error: None,
        });
    }
    state.updated_at = Utc::now();
    static DEMO_WARNED: Once = Once::new();
    DEMO_WARNED.call_once(|| {
        tracing::warn!(
            env = DEMO_ENV,
            "using synthetic usage snapshots (see demo_env)"
        );
    });
}

fn demo_system_active_account_id(provider: ProviderId) -> Option<String> {
    let id = match provider {
        ProviderId::Codex => CODEX_PRIMARY_ID,
        ProviderId::Claude => CLAUDE_PRIMARY_ID,
        ProviderId::Cursor => CURSOR_PRIMARY_ID,
        ProviderId::Gemini => GEMINI_PRIMARY_ID,
        ProviderId::Copilot => return None,
        ProviderId::Minimax => return None,
        ProviderId::Antigravity => return None,
    };
    Some(id.to_string())
}

fn demo_source(provider: ProviderId) -> String {
    match provider {
        ProviderId::Codex | ProviderId::Claude | ProviderId::Gemini | ProviderId::Copilot => {
            "OAuth".to_string()
        }
        ProviderId::Cursor => "Managed Account".to_string(),
        ProviderId::Minimax => "API Key".to_string(),
        ProviderId::Antigravity => "OAuth".to_string(),
    }
}

fn demo_runtime_accounts(provider: ProviderId) -> Vec<ProviderAccountRuntimeState> {
    let now = Utc::now();
    match provider {
        ProviderId::Codex => vec![
            demo_account(
                provider,
                DemoAccount {
                    account_id: CODEX_PRIMARY_ID,
                    label: "plus-1@example.com",
                    last_success_at: now - Duration::minutes(2),
                    health: ProviderHealth::Ok,
                    auth_state: AuthState::Ready,
                    error: None,
                    snapshot: snapshot_codex_primary(),
                },
            ),
            demo_account(
                provider,
                DemoAccount {
                    account_id: CODEX_SECONDARY_ID,
                    label: "plus-2@example.com",
                    last_success_at: now - Duration::minutes(6),
                    health: ProviderHealth::Ok,
                    auth_state: AuthState::Ready,
                    error: None,
                    snapshot: snapshot_codex_secondary(),
                },
            ),
            demo_account(
                provider,
                DemoAccount {
                    account_id: CODEX_PRO_ID,
                    label: "pro@example.com",
                    last_success_at: now - Duration::minutes(1),
                    health: ProviderHealth::Ok,
                    auth_state: AuthState::Ready,
                    error: None,
                    snapshot: snapshot_codex_pro(),
                },
            ),
        ],
        ProviderId::Claude => vec![
            demo_account(
                provider,
                DemoAccount {
                    account_id: CLAUDE_PRIMARY_ID,
                    label: "pro@example.com",
                    last_success_at: now - Duration::minutes(3),
                    health: ProviderHealth::Ok,
                    auth_state: AuthState::Ready,
                    error: None,
                    snapshot: snapshot_claude_primary(),
                },
            ),
            demo_account(
                provider,
                DemoAccount {
                    account_id: CLAUDE_MAX_ID,
                    label: "max@example.com",
                    last_success_at: now - Duration::minutes(2),
                    health: ProviderHealth::Ok,
                    auth_state: AuthState::Ready,
                    error: None,
                    snapshot: snapshot_claude_max(),
                },
            ),
        ],
        ProviderId::Gemini => vec![demo_account(
            provider,
            DemoAccount {
                account_id: GEMINI_PRIMARY_ID,
                label: "pro@example.com",
                last_success_at: now - Duration::minutes(4),
                health: ProviderHealth::Ok,
                auth_state: AuthState::Ready,
                error: None,
                snapshot: snapshot_gemini_primary(),
            },
        )],
        ProviderId::Cursor => vec![demo_account(
            provider,
            DemoAccount {
                account_id: CURSOR_PRIMARY_ID,
                label: "hobby@example.com",
                last_success_at: now - Duration::minutes(1),
                health: ProviderHealth::Ok,
                auth_state: AuthState::Ready,
                error: None,
                snapshot: snapshot_cursor_primary(),
            },
        )],
        ProviderId::Copilot => vec![
            demo_account(
                provider,
                DemoAccount {
                    account_id: COPILOT_FREE_ID,
                    label: "casey-free",
                    last_success_at: now - Duration::minutes(5),
                    health: ProviderHealth::Ok,
                    auth_state: AuthState::Ready,
                    error: None,
                    snapshot: snapshot_copilot_free(),
                },
            ),
            demo_account(
                provider,
                DemoAccount {
                    account_id: COPILOT_PRO_ID,
                    label: "morgan-pro",
                    last_success_at: now - Duration::minutes(5),
                    health: ProviderHealth::Ok,
                    auth_state: AuthState::Ready,
                    error: None,
                    snapshot: snapshot_copilot_pro(),
                },
            ),
        ],
        ProviderId::Minimax => vec![demo_account(
            provider,
            DemoAccount {
                account_id: MINIMAX_PRIMARY_ID,
                label: "jordan-minimax",
                last_success_at: now - Duration::minutes(3),
                health: ProviderHealth::Ok,
                auth_state: AuthState::Ready,
                error: None,
                snapshot: snapshot_minimax_primary(),
            },
        )],
        ProviderId::Antigravity => vec![
            demo_account(
                provider,
                DemoAccount {
                    account_id: ANTIGRAVITY_PRIMARY_ID,
                    label: "pro@example.com",
                    last_success_at: now - Duration::minutes(2),
                    health: ProviderHealth::Ok,
                    auth_state: AuthState::Ready,
                    error: None,
                    snapshot: snapshot_antigravity_primary(),
                },
            ),
            demo_account(
                provider,
                DemoAccount {
                    account_id: ANTIGRAVITY_FREE_ID,
                    label: "free@example.com",
                    last_success_at: now - Duration::minutes(4),
                    health: ProviderHealth::Ok,
                    auth_state: AuthState::Ready,
                    error: None,
                    snapshot: snapshot_antigravity_free(),
                },
            ),
        ],
    }
}

struct DemoAccount {
    account_id: &'static str,
    label: &'static str,
    last_success_at: DateTime<Utc>,
    health: ProviderHealth,
    auth_state: AuthState,
    error: Option<String>,
    snapshot: UsageSnapshot,
}

fn demo_account(provider: ProviderId, account: DemoAccount) -> ProviderAccountRuntimeState {
    ProviderAccountRuntimeState {
        provider,
        account_id: account.account_id.to_string(),
        label: account.label.to_string(),
        source_label: Some(demo_source(provider)),
        last_success_at: Some(account.last_success_at),
        snapshot: Some(account.snapshot),
        health: account.health,
        auth_state: account.auth_state,
        error: account.error,
        retry_after: None,
        consecutive_failures: 0,
    }
}

fn codex_demo_windows(
    now: DateTime<Utc>,
    session_percent: f32,
    weekly_percent: f32,
    session_reset_in: Duration,
    weekly_reset_in: Duration,
) -> Vec<UsageWindow> {
    let session_end = now + session_reset_in;
    let weekly_end = now + weekly_reset_in;
    vec![
        UsageWindow {
            label: "Session".to_string(),
            used_percent: session_percent,
            reset_at: Some(session_end),
            window_seconds: Some(5 * 60 * 60),
            reset_description: None,
            group: None,
        },
        UsageWindow {
            label: "Weekly".to_string(),
            used_percent: weekly_percent,
            reset_at: Some(weekly_end),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: None,
        },
    ]
}

fn snapshot_codex_primary() -> UsageSnapshot {
    let now = Utc::now();
    UsageSnapshot {
        provider: ProviderId::Codex,
        source: "OAuth".to_string(),
        updated_at: now,
        headline: UsageHeadline(0),
        windows: codex_demo_windows(now, 29.0, 83.0, Duration::hours(2), Duration::days(1)),
        provider_cost: Some(ProviderCost {
            used: 320.0,
            limit: None,
            units: "credits".to_string(),
        }),
        extra_usage: None,
        identity: ProviderIdentity {
            email: Some("plus-1@example.com".to_string()),
            account_id: Some("demo-acct-8f2a1c".to_string()),
            plan: Some("plus".to_string()),
            display_name: Some("Plus 1".to_string()),
        },
    }
}

fn snapshot_codex_secondary() -> UsageSnapshot {
    let now = Utc::now();
    UsageSnapshot {
        provider: ProviderId::Codex,
        source: "OAuth".to_string(),
        updated_at: now,
        headline: UsageHeadline(0),
        windows: codex_demo_windows(now, 71.0, 46.0, Duration::minutes(47), Duration::days(3)),
        provider_cost: Some(ProviderCost {
            used: 85.0,
            limit: None,
            units: "credits".to_string(),
        }),
        extra_usage: None,
        identity: ProviderIdentity {
            email: Some("plus-2@example.com".to_string()),
            account_id: Some("demo-acct-31be7d".to_string()),
            plan: Some("plus".to_string()),
            display_name: Some("Plus 2".to_string()),
        },
    }
}

fn snapshot_codex_pro() -> UsageSnapshot {
    let now = Utc::now();
    UsageSnapshot {
        provider: ProviderId::Codex,
        source: "OAuth".to_string(),
        updated_at: now,
        headline: UsageHeadline(0),
        windows: codex_demo_windows(now, 12.0, 38.0, Duration::hours(4), Duration::days(5)),
        provider_cost: Some(ProviderCost {
            used: 540.0,
            limit: None,
            units: "credits".to_string(),
        }),
        extra_usage: None,
        identity: ProviderIdentity {
            email: Some("pro@example.com".to_string()),
            account_id: Some("demo-acct-pro7a4".to_string()),
            plan: Some("pro".to_string()),
            display_name: Some("Pro".to_string()),
        },
    }
}

fn snapshot_claude_primary() -> UsageSnapshot {
    let now = Utc::now();
    let s = now + Duration::hours(3);
    let w = now + Duration::days(2);
    let windows = vec![
        UsageWindow {
            label: "Session".to_string(),
            used_percent: 32.0,
            reset_at: Some(s),
            window_seconds: Some(5 * 60 * 60),
            reset_description: None,
            group: None,
        },
        UsageWindow {
            label: "Weekly".to_string(),
            used_percent: 66.0,
            reset_at: Some(w),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: None,
        },
        UsageWindow {
            label: "Fable".to_string(),
            used_percent: 15.0,
            reset_at: Some(w),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: None,
        },
    ];
    UsageSnapshot {
        provider: ProviderId::Claude,
        source: "OAuth".to_string(),
        updated_at: now,
        headline: UsageHeadline(0),
        windows,
        provider_cost: None,
        extra_usage: Some(ExtraUsageState::Active {
            used_percent: 42.5,
            cost: ProviderCost {
                used: 8.5,
                limit: Some(20.0),
                units: "EUR".to_string(),
            },
        }),
        identity: ProviderIdentity {
            email: Some("pro@example.com".to_string()),
            account_id: None,
            plan: Some("pro".to_string()),
            display_name: Some("Pro".to_string()),
        },
    }
}

fn snapshot_claude_max() -> UsageSnapshot {
    let now = Utc::now();
    let s = now + Duration::hours(2);
    let w = now + Duration::days(4);
    let windows = vec![
        UsageWindow {
            label: "Session".to_string(),
            used_percent: 58.0,
            reset_at: Some(s),
            window_seconds: Some(5 * 60 * 60),
            reset_description: None,
            group: None,
        },
        UsageWindow {
            label: "Weekly".to_string(),
            used_percent: 41.0,
            reset_at: Some(w),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: None,
        },
        UsageWindow {
            label: "Sonnet".to_string(),
            used_percent: 36.0,
            reset_at: Some(w),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: None,
        },
        UsageWindow {
            label: "Opus".to_string(),
            used_percent: 72.0,
            reset_at: Some(w),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: None,
        },
        UsageWindow {
            label: "Cowork".to_string(),
            used_percent: 18.0,
            reset_at: Some(w),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: None,
        },
        UsageWindow {
            label: "Fable".to_string(),
            used_percent: 8.0,
            reset_at: Some(w),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: None,
        },
    ];
    UsageSnapshot {
        provider: ProviderId::Claude,
        source: "OAuth".to_string(),
        updated_at: now,
        headline: UsageHeadline(0),
        windows,
        provider_cost: None,
        extra_usage: Some(ExtraUsageState::Disabled),
        identity: ProviderIdentity {
            email: Some("max@example.com".to_string()),
            account_id: None,
            plan: Some("max".to_string()),
            display_name: Some("Max".to_string()),
        },
    }
}

fn snapshot_gemini_primary() -> UsageSnapshot {
    let now = Utc::now();
    let reset = now + Duration::hours(14);
    let windows = vec![
        UsageWindow {
            label: "Pro".to_string(),
            used_percent: 45.0,
            reset_at: Some(reset),
            window_seconds: None,
            reset_description: None,
            group: None,
        },
        UsageWindow {
            label: "Flash".to_string(),
            used_percent: 20.0,
            reset_at: Some(reset),
            window_seconds: None,
            reset_description: None,
            group: None,
        },
        UsageWindow {
            label: "Lite".to_string(),
            used_percent: 8.0,
            reset_at: Some(reset),
            window_seconds: None,
            reset_description: None,
            group: None,
        },
    ];
    UsageSnapshot {
        provider: ProviderId::Gemini,
        source: "OAuth".to_string(),
        updated_at: now,
        headline: UsageHeadline(0),
        windows,
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity {
            email: Some("pro@example.com".to_string()),
            account_id: None,
            plan: Some(
                crate::providers::gemini::plan_label::plan_label("standard-tier", false)
                    .to_string(),
            ),
            display_name: Some("Pro".to_string()),
        },
    }
}

fn snapshot_cursor_primary() -> UsageSnapshot {
    let now = Utc::now();
    let reset_at = now + Duration::days(20);
    let start = reset_at - Duration::days(30);
    let window_seconds = (reset_at - start).num_seconds();
    let windows = vec![
        window_cursor("Total", 45.0, reset_at, window_seconds),
        window_cursor("Auto + Composer", 24.0, reset_at, window_seconds),
        window_cursor("API", 88.0, reset_at, window_seconds),
    ];
    UsageSnapshot {
        provider: ProviderId::Cursor,
        source: "Managed Account".to_string(),
        updated_at: now,
        headline: UsageHeadline(0),
        windows,
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity {
            email: Some("hobby@example.com".to_string()),
            account_id: None,
            plan: Some("pro".to_string()),
            display_name: Some("Hobby".to_string()),
        },
    }
}

fn snapshot_copilot_free() -> UsageSnapshot {
    let now = Utc::now();
    let reset = now + Duration::days(14);
    let windows = vec![
        UsageWindow {
            label: "chat".to_string(),
            used_percent: 30.0,
            reset_at: Some(reset),
            window_seconds: None,
            reset_description: Some(reset.to_rfc3339()),
            group: None,
        },
        UsageWindow {
            label: "completions".to_string(),
            used_percent: 80.0,
            reset_at: Some(reset),
            window_seconds: None,
            reset_description: Some(reset.to_rfc3339()),
            group: None,
        },
    ];
    UsageSnapshot {
        provider: ProviderId::Copilot,
        source: "Managed Account".to_string(),
        updated_at: now,
        headline: UsageHeadline(1),
        windows,
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity {
            email: None,
            account_id: Some("10101".to_string()),
            plan: Some("Free".to_string()),
            display_name: Some("casey-free".to_string()),
        },
    }
}

fn snapshot_copilot_pro() -> UsageSnapshot {
    let now = Utc::now();
    let reset = now + Duration::days(14);
    UsageSnapshot {
        provider: ProviderId::Copilot,
        source: "Managed Account".to_string(),
        updated_at: now,
        headline: UsageHeadline(0),
        windows: vec![UsageWindow {
            label: "credits".to_string(),
            used_percent: 40.0,
            reset_at: Some(reset),
            window_seconds: None,
            reset_description: Some("+42 over plan".to_string()),
            group: None,
        }],
        provider_cost: Some(ProviderCost {
            used: 28.0,
            limit: Some(70.0),
            units: "USD".to_string(),
        }),
        extra_usage: None,
        identity: ProviderIdentity {
            email: None,
            account_id: Some("20202".to_string()),
            plan: Some("Pro+".to_string()),
            display_name: Some("morgan-pro".to_string()),
        },
    }
}

fn demo_timestamp() -> DateTime<Utc> {
    DateTime::from_timestamp(1_767_225_600, 0).expect("valid demo timestamp")
}

fn demo_minimax_accounts() -> Vec<ManagedMinimaxAccountConfig> {
    let now = demo_timestamp();
    vec![ManagedMinimaxAccountConfig {
        id: MINIMAX_PRIMARY_ID.to_string(),
        label: "jordan-minimax".to_string(),
        api_key_source: "demo".to_string(),
        created_at: now,
        updated_at: now,
        last_authenticated_at: Some(now),
    }]
}

fn snapshot_minimax_primary() -> UsageSnapshot {
    let now = Utc::now();
    UsageSnapshot {
        provider: ProviderId::Minimax,
        source: "API Key".to_string(),
        updated_at: now,
        headline: UsageHeadline(0),
        windows: vec![
            UsageWindow {
                label: "MiniMax-M2 (5h): 640/1000".to_string(),
                used_percent: 36.0,
                reset_at: None,
                window_seconds: Some(5 * 3600),
                reset_description: Some("Resets every 5 hours".to_string()),
                group: None,
            },
            UsageWindow {
                label: "MiniMax-M2 (Weekly): 3800/10000".to_string(),
                used_percent: 62.0,
                reset_at: None,
                window_seconds: Some(7 * 24 * 3600),
                reset_description: Some("Resets weekly".to_string()),
                group: None,
            },
        ],
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity {
            email: None,
            account_id: None,
            plan: Some("Minimax.io".to_string()),
            display_name: Some("jordan-minimax".to_string()),
        },
    }
}

fn demo_codex_accounts() -> Vec<ManagedCodexAccountConfig> {
    let now = demo_timestamp();
    vec![
        ManagedCodexAccountConfig {
            id: CODEX_PRIMARY_ID.to_string(),
            label: "plus-1@example.com".to_string(),
            codex_home: demo_root().join("codex-primary"),
            email: Some("plus-1@example.com".to_string()),
            provider_account_id: Some("demo-acct-8f2a1c".to_string()),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        },
        ManagedCodexAccountConfig {
            id: CODEX_SECONDARY_ID.to_string(),
            label: "plus-2@example.com".to_string(),
            codex_home: demo_root().join("codex-secondary"),
            email: Some("plus-2@example.com".to_string()),
            provider_account_id: Some("demo-acct-31be7d".to_string()),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        },
        ManagedCodexAccountConfig {
            id: CODEX_PRO_ID.to_string(),
            label: "pro@example.com".to_string(),
            codex_home: demo_root().join("codex-pro"),
            email: Some("pro@example.com".to_string()),
            provider_account_id: Some("demo-acct-pro7a4".to_string()),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        },
    ]
}

fn demo_claude_accounts() -> Vec<ManagedClaudeAccountConfig> {
    let now = demo_timestamp();
    vec![
        ManagedClaudeAccountConfig {
            id: CLAUDE_PRIMARY_ID.to_string(),
            label: "pro@example.com".to_string(),
            config_dir: demo_root().join("claude-primary"),
            email: Some("pro@example.com".to_string()),
            organization: Some("YapCap".to_string()),
            subscription_type: Some("pro".to_string()),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        },
        ManagedClaudeAccountConfig {
            id: CLAUDE_MAX_ID.to_string(),
            label: "max@example.com".to_string(),
            config_dir: demo_root().join("claude-max"),
            email: Some("max@example.com".to_string()),
            organization: Some("YapCap".to_string()),
            subscription_type: Some("max".to_string()),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        },
    ]
}

fn demo_cursor_accounts() -> Vec<ManagedCursorAccountConfig> {
    let now = demo_timestamp();
    vec![ManagedCursorAccountConfig {
        id: CURSOR_PRIMARY_ID.to_string(),
        email: "hobby@example.com".to_string(),
        label: "hobby@example.com".to_string(),
        account_root: demo_root().join("cursor-primary"),
        display_name: Some("Hobby".to_string()),
        plan: Some("hobby".to_string()),
        created_at: now,
        updated_at: now,
        last_authenticated_at: Some(now),
    }]
}

fn demo_gemini_accounts() -> Vec<ManagedGeminiAccountConfig> {
    let now = demo_timestamp();
    vec![ManagedGeminiAccountConfig {
        id: GEMINI_PRIMARY_ID.to_string(),
        label: "pro@example.com".to_string(),
        account_root: demo_root().join("gemini-primary"),
        email: "pro@example.com".to_string(),
        sub: "demo-gemini-sub".to_string(),
        hd: None,
        last_tier_id: Some("standard-tier".to_string()),
        last_cloudaicompanion_project: Some("demo-gemini-project".to_string()),
        created_at: now,
        updated_at: now,
        last_authenticated_at: Some(now),
    }]
}

fn demo_copilot_accounts() -> Vec<ManagedCopilotAccountConfig> {
    let now = demo_timestamp();
    vec![
        ManagedCopilotAccountConfig {
            id: COPILOT_FREE_ID.to_string(),
            label: "casey-free".to_string(),
            github_user_id: 10101,
            login: "casey-free".to_string(),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        },
        ManagedCopilotAccountConfig {
            id: COPILOT_PRO_ID.to_string(),
            label: "morgan-pro".to_string(),
            github_user_id: 20202,
            login: "morgan-pro".to_string(),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        },
    ]
}

fn demo_antigravity_accounts() -> Vec<ManagedAntigravityAccountConfig> {
    let now = demo_timestamp();
    vec![
        ManagedAntigravityAccountConfig {
            id: ANTIGRAVITY_PRIMARY_ID.to_string(),
            label: "pro@example.com".to_string(),
            account_root: demo_root().join("antigravity-primary"),
            email: "pro@example.com".to_string(),
            sub: "demo-antigravity-pro-sub".to_string(),
            last_tier_id: Some("g1-pro-tier".to_string()),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        },
        ManagedAntigravityAccountConfig {
            id: ANTIGRAVITY_FREE_ID.to_string(),
            label: "free@example.com".to_string(),
            account_root: demo_root().join("antigravity-free"),
            email: "free@example.com".to_string(),
            sub: "demo-antigravity-free-sub".to_string(),
            last_tier_id: Some("free-tier".to_string()),
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        },
    ]
}

fn snapshot_antigravity_primary() -> UsageSnapshot {
    let now = Utc::now();
    let weekly = now + Duration::days(6);
    let five_hour = now + Duration::hours(4);
    let windows = vec![
        UsageWindow {
            label: "Five Hour Limit".to_string(),
            used_percent: 44.0,
            reset_at: Some(five_hour),
            window_seconds: Some(5 * 3600),
            reset_description: None,
            group: Some("Gemini Models".to_string()),
        },
        UsageWindow {
            label: "Weekly Limit".to_string(),
            used_percent: 12.0,
            reset_at: Some(weekly),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: Some("Gemini Models".to_string()),
        },
        UsageWindow {
            label: "Five Hour Limit".to_string(),
            used_percent: 0.0,
            reset_at: Some(five_hour),
            window_seconds: Some(5 * 3600),
            reset_description: None,
            group: Some("Claude and GPT models".to_string()),
        },
        UsageWindow {
            label: "Weekly Limit".to_string(),
            used_percent: 3.0,
            reset_at: Some(weekly),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: Some("Claude and GPT models".to_string()),
        },
    ];
    UsageSnapshot {
        provider: ProviderId::Antigravity,
        source: "OAuth".to_string(),
        updated_at: now,
        headline: UsageHeadline(1),
        windows,
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity {
            email: Some("pro@example.com".to_string()),
            account_id: None,
            plan: Some(
                crate::providers::antigravity::plan_label::plan_label("g1-pro-tier").to_string(),
            ),
            display_name: Some("Pro".to_string()),
        },
    }
}

fn snapshot_antigravity_free() -> UsageSnapshot {
    let now = Utc::now();
    let weekly = now + Duration::days(3);
    UsageSnapshot {
        provider: ProviderId::Antigravity,
        source: "OAuth".to_string(),
        updated_at: now,
        headline: UsageHeadline(0),
        windows: vec![UsageWindow {
            label: "Weekly Limit".to_string(),
            used_percent: 27.0,
            reset_at: Some(weekly),
            window_seconds: Some(7 * 24 * 3600),
            reset_description: None,
            group: Some("Free Tier".to_string()),
        }],
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity {
            email: Some("free@example.com".to_string()),
            account_id: None,
            plan: Some("Free".to_string()),
            display_name: Some("Free".to_string()),
        },
    }
}

fn demo_root() -> PathBuf {
    paths().cache_dir.join("demo")
}

fn window_cursor(
    label: &str,
    used_percent: f32,
    reset_at: chrono::DateTime<Utc>,
    window_seconds: i64,
) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used_percent,
        reset_at: Some(reset_at),
        window_seconds: Some(window_seconds),
        reset_description: Some(reset_at.to_rfc3339()),
        group: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    #[test]
    fn build_snapshots_valid() {
        for snapshot in [
            snapshot_codex_primary(),
            snapshot_codex_secondary(),
            snapshot_codex_pro(),
            snapshot_claude_primary(),
            snapshot_claude_max(),
            snapshot_gemini_primary(),
            snapshot_cursor_primary(),
            snapshot_antigravity_primary(),
            snapshot_antigravity_free(),
        ] {
            assert!(!snapshot.windows.is_empty());
            assert!(snapshot.identity.email.is_some());
        }
    }

    #[test]
    fn demo_detection_snapshot_detects_nothing() {
        let snapshot = detection_snapshot();
        for provider in ProviderId::ALL {
            assert!(!snapshot.detected(provider));
        }
    }

    #[test]
    fn demo_config_is_multi_account_ready() {
        let _guard = test_support::env_lock();
        unsafe {
            std::env::set_var(DEMO_ENV, "1");
        }
        let mut config = Config::default();
        apply_config(&mut config);
        unsafe {
            std::env::remove_var(DEMO_ENV);
        }

        assert_eq!(config.codex_managed_accounts.len(), 3);
        assert_eq!(config.claude_managed_accounts.len(), 2);
        assert_eq!(config.cursor_managed_accounts.len(), 1);
        assert_eq!(config.gemini_managed_accounts.len(), 1);
        assert_eq!(config.copilot_managed_accounts.len(), 2);
        assert_eq!(config.minimax_managed_accounts.len(), 1);
        assert_eq!(config.antigravity_managed_accounts.len(), 2);
        assert_eq!(config.selected_codex_account_ids.len(), 3);
        assert_eq!(config.selected_claude_account_ids.len(), 2);
        assert_eq!(config.selected_cursor_account_ids.len(), 1);
        assert_eq!(config.selected_gemini_account_ids.len(), 1);
        assert_eq!(config.selected_copilot_account_ids.len(), 2);
        assert_eq!(config.selected_minimax_account_ids.len(), 1);
        assert_eq!(config.selected_antigravity_account_ids.len(), 2);
        for provider in ProviderId::ALL {
            assert_eq!(
                config.provider_enablement(provider),
                ProviderEnablement::Enabled
            );
        }
        assert!(config.show_all_accounts(ProviderId::Codex));
        assert!(config.show_all_accounts(ProviderId::Claude));
        assert!(!config.show_all_accounts(ProviderId::Cursor));
        assert!(!config.show_all_accounts(ProviderId::Gemini));
        assert!(config.show_all_accounts(ProviderId::Copilot));
        assert!(config.show_all_accounts(ProviderId::Antigravity));
        assert_eq!(
            config.provider_visibility_mode,
            ProviderVisibilityMode::UserManaged
        );
    }

    #[test]
    fn demo_replaces_existing_account_selections() {
        let _guard = test_support::env_lock();
        unsafe {
            std::env::set_var(DEMO_ENV, "1");
        }
        let mut config = Config {
            selected_codex_account_ids: vec!["real-codex".to_string()],
            selected_claude_account_ids: vec!["real-claude".to_string()],
            selected_cursor_account_ids: vec!["real-cursor".to_string()],
            selected_gemini_account_ids: vec!["real-gemini".to_string()],
            selected_copilot_account_ids: vec!["real-copilot".to_string()],
            selected_minimax_account_ids: vec!["real-minimax".to_string()],
            selected_antigravity_account_ids: vec!["real-antigravity".to_string()],
            ..Config::default()
        };
        apply_config(&mut config);
        unsafe {
            std::env::remove_var(DEMO_ENV);
        }

        for provider in ProviderId::ALL {
            let selected = config.selected_account_ids(provider);
            assert!(
                !selected.is_empty(),
                "{} should have demo accounts selected",
                provider.label()
            );
            let managed_ids: Vec<&String> = match provider {
                ProviderId::Codex => config
                    .codex_managed_accounts
                    .iter()
                    .map(|a| &a.id)
                    .collect(),
                ProviderId::Claude => config
                    .claude_managed_accounts
                    .iter()
                    .map(|a| &a.id)
                    .collect(),
                ProviderId::Cursor => config
                    .cursor_managed_accounts
                    .iter()
                    .map(|a| &a.id)
                    .collect(),
                ProviderId::Gemini => config
                    .gemini_managed_accounts
                    .iter()
                    .map(|a| &a.id)
                    .collect(),
                ProviderId::Copilot => config
                    .copilot_managed_accounts
                    .iter()
                    .map(|a| &a.id)
                    .collect(),
                ProviderId::Minimax => config
                    .minimax_managed_accounts
                    .iter()
                    .map(|a| &a.id)
                    .collect(),
                ProviderId::Antigravity => config
                    .antigravity_managed_accounts
                    .iter()
                    .map(|a| &a.id)
                    .collect(),
            };
            for id in selected {
                assert!(
                    managed_ids.contains(&id),
                    "{} selection {id} should reference a demo account",
                    provider.label()
                );
            }
        }
    }

    #[test]
    fn demo_enables_all_providers_with_accounts() {
        let _guard = test_support::env_lock();
        unsafe {
            std::env::set_var(DEMO_ENV, "1");
        }
        let mut config = Config::default();
        apply_config(&mut config);
        let detection = detection_snapshot();
        let mut state = crate::runtime::load_initial_state(&config, &detection, None);
        apply(&config, &mut state);
        unsafe {
            std::env::remove_var(DEMO_ENV);
        }

        for provider in ProviderId::ALL {
            assert!(
                state.provider(provider).is_some_and(|entry| entry.enabled),
                "{} should be enabled in demo mode",
                provider.label()
            );
            assert!(
                state
                    .provider_accounts
                    .iter()
                    .any(|account| account.provider == provider),
                "{} should have demo accounts",
                provider.label()
            );
        }
    }

    #[test]
    fn demo_state_marks_one_active_account_per_provider() {
        let _guard = test_support::env_lock();
        unsafe {
            std::env::set_var(DEMO_ENV, "1");
        }
        let mut config = Config::default();
        apply_config(&mut config);
        let mut state = AppState::empty();
        apply(&config, &mut state);
        unsafe {
            std::env::remove_var(DEMO_ENV);
        }

        assert_eq!(
            state
                .provider(ProviderId::Codex)
                .and_then(|provider| provider.system_active_account_id.as_deref()),
            Some(CODEX_PRIMARY_ID)
        );
        assert_eq!(
            state
                .provider(ProviderId::Claude)
                .and_then(|provider| provider.system_active_account_id.as_deref()),
            Some(CLAUDE_PRIMARY_ID)
        );
        assert_eq!(
            state
                .provider(ProviderId::Cursor)
                .and_then(|provider| provider.system_active_account_id.as_deref()),
            Some(CURSOR_PRIMARY_ID)
        );
        assert_eq!(
            state
                .provider(ProviderId::Gemini)
                .and_then(|provider| provider.system_active_account_id.as_deref()),
            Some(GEMINI_PRIMARY_ID)
        );
        assert_eq!(
            state
                .provider(ProviderId::Copilot)
                .and_then(|provider| provider.system_active_account_id.as_deref()),
            None
        );
        for account in &state.provider_accounts {
            assert_eq!(account.health, ProviderHealth::Ok);
            assert!(account.snapshot.is_some());
            assert!(
                account
                    .last_success_at
                    .is_some_and(|updated| { Utc::now() - updated < Duration::minutes(10) })
            );
        }
    }

    #[test]
    fn antigravity_demo_seeds_selected_pro_and_free_accounts() {
        let _guard = test_support::env_lock();
        unsafe {
            std::env::set_var(DEMO_ENV, "1");
        }
        let mut config = Config::default();
        apply_config(&mut config);
        let mut state = AppState::empty();
        apply(&config, &mut state);
        unsafe {
            std::env::remove_var(DEMO_ENV);
        }

        assert_eq!(
            config.selected_antigravity_account_ids,
            vec![
                ANTIGRAVITY_PRIMARY_ID.to_string(),
                ANTIGRAVITY_FREE_ID.to_string(),
            ]
        );
        assert!(config.show_all_accounts(ProviderId::Antigravity));
        let free = state
            .provider_accounts
            .iter()
            .find(|account| account.account_id == ANTIGRAVITY_FREE_ID)
            .and_then(|account| account.snapshot.as_ref())
            .expect("free Antigravity demo snapshot");
        assert_eq!(free.identity.email.as_deref(), Some("free@example.com"));
        assert_eq!(free.identity.plan.as_deref(), Some("Free"));
        assert_eq!(free.windows.len(), 1);
        assert_eq!(free.windows[0].label, "Weekly Limit");
    }

    #[test]
    fn copilot_demo_seeds_free_and_overage_pro_accounts() {
        let _guard = test_support::env_lock();
        unsafe {
            std::env::set_var(DEMO_ENV, "1");
        }
        let mut config = Config::default();
        apply_config(&mut config);
        let mut state = AppState::empty();
        apply(&config, &mut state);
        unsafe {
            std::env::remove_var(DEMO_ENV);
        }

        assert_eq!(
            config.selected_copilot_account_ids,
            vec![
                "yapcap-demo:copilot-casey-free".to_string(),
                "yapcap-demo:copilot-morgan-pro".to_string()
            ]
        );
        assert_eq!(
            state
                .provider(ProviderId::Copilot)
                .map(|provider| provider.selected_account_ids.clone()),
            Some(config.selected_copilot_account_ids.clone())
        );

        let casey = state
            .provider_accounts
            .iter()
            .find(|account| {
                account.provider == ProviderId::Copilot
                    && account.account_id == "yapcap-demo:copilot-casey-free"
            })
            .and_then(|account| account.snapshot.as_ref())
            .expect("casey-free demo snapshot");
        assert_eq!(casey.identity.display_name.as_deref(), Some("casey-free"));
        assert_eq!(casey.identity.plan.as_deref(), Some("Free"));
        assert_eq!(casey.headline, UsageHeadline(1));
        assert_eq!(casey.windows.len(), 2);
        assert_eq!(casey.windows[0].label, "chat");
        assert!((casey.windows[0].used_percent - 30.0).abs() < 0.001);
        assert_eq!(casey.windows[1].label, "completions");
        assert!((casey.windows[1].used_percent - 80.0).abs() < 0.001);

        let morgan = state
            .provider_accounts
            .iter()
            .find(|account| {
                account.provider == ProviderId::Copilot
                    && account.account_id == "yapcap-demo:copilot-morgan-pro"
            })
            .and_then(|account| account.snapshot.as_ref())
            .expect("morgan-pro demo snapshot");
        assert_eq!(morgan.identity.display_name.as_deref(), Some("morgan-pro"));
        assert_eq!(morgan.identity.plan.as_deref(), Some("Pro+"));
        assert_eq!(morgan.headline, UsageHeadline(0));
        assert_eq!(morgan.windows.len(), 1);
        assert_eq!(morgan.windows[0].label, "credits");
        assert!((morgan.windows[0].used_percent - 40.0).abs() < 0.001);
        assert_eq!(
            morgan.windows[0].reset_description.as_deref(),
            Some("+42 over plan")
        );
        let cost = morgan.provider_cost.as_ref().expect("morgan cost card");
        assert!((cost.used - 28.0).abs() < 0.001);
        assert_eq!(cost.limit, Some(70.0));
        assert_eq!(cost.units, "USD");
    }

    #[test]
    fn minimax_demo_seeds_one_account_with_token_windows() {
        let _guard = test_support::env_lock();
        unsafe {
            std::env::set_var(DEMO_ENV, "1");
        }
        let mut config = Config::default();
        apply_config(&mut config);
        let mut state = AppState::empty();
        apply(&config, &mut state);
        unsafe {
            std::env::remove_var(DEMO_ENV);
        }

        assert_eq!(
            config.selected_minimax_account_ids,
            vec![MINIMAX_PRIMARY_ID.to_string()]
        );
        let account = state
            .provider_accounts
            .iter()
            .find(|account| {
                account.provider == ProviderId::Minimax && account.account_id == MINIMAX_PRIMARY_ID
            })
            .expect("minimax demo account");
        assert_eq!(account.health, ProviderHealth::Ok);
        let snapshot = account.snapshot.as_ref().expect("minimax demo snapshot");
        assert_eq!(snapshot.identity.plan.as_deref(), Some("Minimax.io"));
        assert_eq!(snapshot.windows.len(), 2);
    }

    #[test]
    fn codex_demo_snapshots_have_believable_session_and_weekly() {
        let primary = snapshot_codex_primary();
        let secondary = snapshot_codex_secondary();
        assert_eq!(primary.windows.len(), 2);
        assert_eq!(secondary.windows.len(), 2);
        assert!((primary.windows[0].used_percent - 29.0).abs() < f32::EPSILON);
        assert!((primary.windows[1].used_percent - 83.0).abs() < f32::EPSILON);
        assert!((secondary.windows[0].used_percent - 71.0).abs() < f32::EPSILON);
        assert!((secondary.windows[1].used_percent - 46.0).abs() < f32::EPSILON);
        assert_eq!(
            primary.windows[0].reset_at.unwrap() - primary.updated_at,
            Duration::hours(2)
        );
        assert_eq!(
            primary.windows[1].reset_at.unwrap() - primary.updated_at,
            Duration::days(1)
        );
        assert_eq!(
            secondary.windows[0].reset_at.unwrap() - secondary.updated_at,
            Duration::minutes(47)
        );
        assert_eq!(
            secondary.windows[1].reset_at.unwrap() - secondary.updated_at,
            Duration::days(3)
        );
        assert_eq!(
            primary.provider_cost.as_ref().map(|cost| cost.used),
            Some(320.0)
        );
        assert_eq!(
            secondary.provider_cost.as_ref().map(|cost| cost.used),
            Some(85.0)
        );
    }

    #[test]
    fn gemini_demo_primary_has_pro_flash_lite_bars() {
        let snapshot = snapshot_gemini_primary();
        assert_eq!(snapshot.identity.plan.as_deref(), Some("Pro"));
        let labels: Vec<&str> = snapshot.windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, vec!["Pro", "Flash", "Lite"]);
        for window in &snapshot.windows {
            assert!(window.used_percent > 0.0);
            assert!(window.reset_at.is_some());
        }
    }

    #[test]
    fn claude_demo_primary_is_pro_without_sonnet() {
        let snapshot = snapshot_claude_primary();
        assert_eq!(snapshot.identity.plan.as_deref(), Some("pro"));
        assert_eq!(snapshot.windows.len(), 3);
        assert!(
            snapshot
                .windows
                .iter()
                .all(|window| window.label != "Sonnet" && window.label != "Opus")
        );
        assert!(
            snapshot
                .windows
                .iter()
                .any(|window| window.label == "Fable")
        );
        let Some(ExtraUsageState::Active { used_percent, cost }) = snapshot.extra_usage.as_ref()
        else {
            panic!("expected active extra usage");
        };
        assert!((*used_percent - 42.5).abs() < f32::EPSILON);
        assert!((cost.used - 8.5).abs() < f64::EPSILON);
        assert_eq!(cost.limit, Some(20.0));
        assert_eq!(cost.units, "EUR");
    }
}
