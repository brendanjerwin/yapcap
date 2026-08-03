// SPDX-License-Identifier: MPL-2.0

pub mod account;
pub mod login;
pub mod storage;

use crate::config::{Config, ManagedOllamaCloudAccountConfig, managed_ollama_cloud_account_dir};
use crate::error::OllamaCloudError;
use crate::model::{ProviderId, UsageSnapshot};
use chrono::Utc;
use regex::Regex;
use std::sync::LazyLock;

pub use account::{discover_accounts, remove_managed_config_dir};
pub use login::{
    OllamaCloudLoginEvent, OllamaCloudLoginState, OllamaCloudLoginStatus, prepare as prepare_login,
};
pub use storage::load_session_cookie;

const DASHBOARD_URL: &str = "https://ollama.com/settings";
const USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Gecko/20100101 Firefox/148.0";

// Regex patterns for parsing usage data from the Ollama Cloud settings page.
// The page uses aria-label attributes like "Session usage 42% used" and
// data-time attributes for reset timestamps.
static RE_SESSION_USAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)aria-label="[^"]*session[^"]*?(\d+(?:\.\d+)?)\s*%"#).unwrap()
});
static RE_WEEKLY_USAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)aria-label="[^"]*weekly[^"]*?(\d+(?:\.\d+)?)\s*%"#).unwrap()
});
static RE_RESET_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"data-time="([^"]+)""#).unwrap()
});

pub fn sync_managed_accounts(config: &mut Config) -> bool {
    let mut changed = false;
    let original_len = config.ollama_cloud_managed_accounts.len();
    config
        .ollama_cloud_managed_accounts
        .retain(|account| {
            if account.session_cookie_source.starts_with("env:")
                || !account.session_cookie_source.is_empty()
            {
                true
            } else {
                changed = true;
                false
            }
        });
    if config.ollama_cloud_managed_accounts.len() != original_len {
        changed = true;
    }
    changed
}

/// What to do with an HTTP response from the dashboard.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DashboardResponseAction {
    Proceed,
    RefreshCookie,
    RateLimited { retry_after_secs: Option<u64> },
    ServerError { status: u16 },
}

/// Decide what to do based on the HTTP status code from the dashboard.
pub(crate) fn handle_status_code(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
) -> DashboardResponseAction {
    match status {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            DashboardResponseAction::RefreshCookie
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            let retry_after = headers
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok());
            DashboardResponseAction::RateLimited { retry_after_secs: retry_after }
        }
        s if s.is_server_error() => {
            DashboardResponseAction::ServerError { status: s.as_u16() }
        }
        _ => DashboardResponseAction::Proceed,
    }
}

pub async fn fetch(
    client: &reqwest::Client,
    account: &ManagedOllamaCloudAccountConfig,
    cookie_source: &dyn crate::browser_cookies::CookieSource,
) -> Result<UsageSnapshot, OllamaCloudError> {
    let account_root = managed_ollama_cloud_account_dir(&account.id);
    let mut session_cookie = load_session_cookie(&account_root)
        .ok()
        .filter(|c| !c.is_empty())
        .ok_or(OllamaCloudError::LoginRequired)?;

    let response = client
        .get(DASHBOARD_URL)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "text/html")
        .header("Cookie", format!("__Secure-session={session_cookie}"))
        .send()
        .await
        .map_err(OllamaCloudError::DashboardRequest)?;

    let response = match handle_status_code(response.status(), response.headers()) {
        DashboardResponseAction::Proceed => response,
        DashboardResponseAction::RefreshCookie => {
            if let Some(fresh) = cookie_source.find_cookie("__Secure-session", "ollama.com").await {
                session_cookie = fresh.value.clone();
                if let Err(e) = crate::providers::ollama_cloud::storage::write_session_cookie(&account_root, &session_cookie) {
                    tracing::warn!(error = %e, "failed to persist refreshed session cookie");
                }
                client
                    .get(DASHBOARD_URL)
                    .header("User-Agent", USER_AGENT)
                    .header("Accept", "text/html")
                    .header("Cookie", format!("__Secure-session={session_cookie}"))
                    .send()
                    .await
                    .map_err(OllamaCloudError::DashboardRequest)?
            } else {
                return Err(OllamaCloudError::LoginRequired);
            }
        }
        DashboardResponseAction::RateLimited { retry_after_secs } => {
            return Err(OllamaCloudError::RateLimited { retry_after_secs });
        }
        DashboardResponseAction::ServerError { status } => {
            return Err(OllamaCloudError::DashboardHttp { status });
        }
    };

    let response = response
        .error_for_status()
        .map_err(OllamaCloudError::DashboardEndpoint)?;
    let html = response.text().await.map_err(OllamaCloudError::ReadDashboard)?;

    parse(&html, Utc::now())
}
pub fn parse(
    html: &str,
    updated_at: chrono::DateTime<Utc>,
) -> Result<UsageSnapshot, OllamaCloudError> {
    let session_percent = RE_SESSION_USAGE
        .captures(html)
        .and_then(|caps| caps[1].parse::<f32>().ok());

    let weekly_percent = RE_WEEKLY_USAGE
        .captures(html)
        .and_then(|caps| caps[1].parse::<f32>().ok());

    // Try to find reset timestamps near the usage elements
    let reset_times: Vec<chrono::DateTime<Utc>> = RE_RESET_TIME
        .captures_iter(html)
        .filter_map(|caps| {
            let time_str = caps[1].trim();
            chrono::DateTime::parse_from_rfc3339(time_str)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
        .collect();

    if session_percent.is_none() && weekly_percent.is_none() {
        return Err(OllamaCloudError::ParseDashboard);
    }

    let mut windows = Vec::new();

    if let Some(used_percent) = session_percent {
        let used_percent = used_percent.clamp(0.0, 100.0);
        let reset_at = reset_times.first().copied();
        windows.push(crate::model::UsageWindow {
            label: "Session".to_string(),
            used_percent,
            reset_at,
            window_seconds: Some(5 * 3600),
            reset_description: Some("Rolling session window".to_string()),
            group: None,
        });
    }
    if let Some(used_percent) = weekly_percent {
        let used_percent = used_percent.clamp(0.0, 100.0);
        let reset_at = reset_times.get(1).copied().or(reset_times.first().copied());
        windows.push(crate::model::UsageWindow {
            label: "Weekly".to_string(),
            used_percent,
            reset_at,
            window_seconds: Some(7 * 24 * 3600),
            reset_description: Some("Weekly window".to_string()),
            group: None,
        });
    }

    Ok(UsageSnapshot {
        provider: ProviderId::OllamaCloud,
        source: "Dashboard".to_string(),
        updated_at,
        headline: crate::model::UsageHeadline(0),
        windows,
        provider_cost: None,
        extra_usage: None,
        identity: crate::model::ProviderIdentity {
            email: None,
            account_id: None,
            plan: Some("Ollama Cloud".to_string()),
            display_name: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_status_code_200_ok_proceeds() {
        let headers = reqwest::header::HeaderMap::new();
        let status = reqwest::StatusCode::from_u16(200).unwrap();
        assert_eq!(handle_status_code(status, &headers), DashboardResponseAction::Proceed);
    }

    #[test]
    fn handle_status_code_401_refreshes_cookie() {
        let headers = reqwest::header::HeaderMap::new();
        let status = reqwest::StatusCode::from_u16(401).unwrap();
        assert_eq!(handle_status_code(status, &headers), DashboardResponseAction::RefreshCookie);
    }

    #[test]
    fn handle_status_code_403_refreshes_cookie() {
        let headers = reqwest::header::HeaderMap::new();
        let status = reqwest::StatusCode::from_u16(403).unwrap();
        assert_eq!(handle_status_code(status, &headers), DashboardResponseAction::RefreshCookie);
    }

    #[test]
    fn handle_status_code_429_without_retry_after_is_rate_limited_none() {
        let headers = reqwest::header::HeaderMap::new();
        let status = reqwest::StatusCode::from_u16(429).unwrap();
        assert_eq!(
            handle_status_code(status, &headers),
            DashboardResponseAction::RateLimited { retry_after_secs: None }
        );
    }

    #[test]
    fn handle_status_code_429_with_retry_after_is_rate_limited_some() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "60".parse().unwrap());
        let status = reqwest::StatusCode::from_u16(429).unwrap();
        assert_eq!(
            handle_status_code(status, &headers),
            DashboardResponseAction::RateLimited { retry_after_secs: Some(60) }
        );
    }

    #[test]
    fn handle_status_code_500_is_server_error_500() {
        let headers = reqwest::header::HeaderMap::new();
        let status = reqwest::StatusCode::from_u16(500).unwrap();
        assert_eq!(
            handle_status_code(status, &headers),
            DashboardResponseAction::ServerError { status: 500 }
        );
    }

    #[test]
    fn handle_status_code_503_is_server_error_503() {
        let headers = reqwest::header::HeaderMap::new();
        let status = reqwest::StatusCode::from_u16(503).unwrap();
        assert_eq!(
            handle_status_code(status, &headers),
            DashboardResponseAction::ServerError { status: 503 }
        );
    }

    #[test]
    fn handle_status_code_302_redirect_proceeds() {
        let headers = reqwest::header::HeaderMap::new();
        let status = reqwest::StatusCode::from_u16(302).unwrap();
        assert_eq!(handle_status_code(status, &headers), DashboardResponseAction::Proceed);
    }

    #[test]
    fn parse_aria_label_format() {
        let html = r#"<html><body>
        <div aria-label="Session usage 42% used" data-time="2026-07-10T12:00:00Z"></div>
        <div aria-label="Weekly usage 60% used" data-time="2026-07-07T08:00:00Z"></div>
        </body></html>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.provider, ProviderId::OllamaCloud);
        assert_eq!(snapshot.windows.len(), 2);
        assert!((snapshot.windows[0].used_percent - 42.0).abs() < 0.01);
        assert!((snapshot.windows[1].used_percent - 60.0).abs() < 0.01);
    }

    #[test]
    fn parse_no_data_returns_error() {
        let html = "<html><body>no usage data here</body></html>";
        let result = parse(html, Utc::now());
        assert!(result.is_err());
        match result {
            Err(OllamaCloudError::ParseDashboard) => {}
            _ => panic!("expected ParseDashboard error"),
        }
    }

    // ---- aria-label format: only session ----
    #[test]
    fn parse_aria_label_only_session() {
        let html = r#"<div aria-label="Session usage 10% used" data-time="2026-07-10T12:00:00Z"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.windows[0].label, "Session");
        assert!((snapshot.windows[0].used_percent - 10.0).abs() < 0.01);
    }

    // ---- aria-label format: only weekly ----
    #[test]
    fn parse_aria_label_only_weekly() {
        let html = r#"<div aria-label="Weekly usage 25% used" data-time="2026-07-07T08:00:00Z"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.windows[0].label, "Weekly");
        assert!((snapshot.windows[0].used_percent - 25.0).abs() < 0.01);
    }

    // ---- both session and weekly ----
    #[test]
    fn parse_aria_label_both() {
        let html = r#"<div aria-label="Session usage 42% used" data-time="2026-07-10T12:00:00Z"></div>
        <div aria-label="Weekly usage 60% used" data-time="2026-07-07T08:00:00Z"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].label, "Session");
        assert_eq!(snapshot.windows[1].label, "Weekly");
    }

    // ---- empty html returns ParseDashboard ----
    #[test]
    fn parse_empty_html_returns_error() {
        let result = parse("", Utc::now());
        assert!(matches!(result, Err(OllamaCloudError::ParseDashboard)));
    }

    // ---- percentage clamping: >100 clamped to 100 ----
    #[test]
    fn parse_clamps_over_100() {
        let html = r#"<div aria-label="Session usage 150% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert!((snapshot.windows[0].used_percent - 100.0).abs() < 0.01);
    }

    // ---- percentage clamping: negative clamped to 0 ----
    #[test]
    fn parse_clamps_negative() {
        let html = r#"<div aria-label="Session usage 0% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert!(snapshot.windows[0].used_percent.abs() < 0.01);
    }

    // ---- data-time attributes parsed for reset_at ----
    #[test]
    fn parse_data_time_reset_at() {
        let html = r#"<div aria-label="Session usage 42% used" data-time="2026-07-10T12:00:00Z"></div>
        <div aria-label="Weekly usage 60% used" data-time="2026-07-07T08:00:00Z"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        let session_reset = snapshot.windows[0].reset_at.expect("session reset_at");
        assert_eq!(session_reset.to_rfc3339(), "2026-07-10T12:00:00+00:00");
        let weekly_reset = snapshot.windows[1].reset_at.expect("weekly reset_at");
        assert_eq!(weekly_reset.to_rfc3339(), "2026-07-07T08:00:00+00:00");
    }

    // ---- weekly reset falls back to first reset time when only one data-time present ----
    #[test]
    fn parse_weekly_reset_falls_back_to_first() {
        let html = r#"<div aria-label="Session usage 42% used" data-time="2026-07-10T12:00:00Z"></div>
        <div aria-label="Weekly usage 60% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        let weekly_reset = snapshot.windows[1].reset_at.expect("weekly reset_at");
        assert_eq!(weekly_reset.to_rfc3339(), "2026-07-10T12:00:00+00:00");
    }

    // ---- identity plan is 'Ollama Cloud' ----
    #[test]
    fn parse_identity_plan() {
        let html = r#"<div aria-label="Session usage 42% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.identity.plan.as_deref(), Some("Ollama Cloud"));
    }

    // ---- source field is 'Dashboard' ----
    #[test]
    fn parse_source_field() {
        let html = r#"<div aria-label="Session usage 42% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.source, "Dashboard");
    }

    // ---- provider is OllamaCloud ----
    #[test]
    fn parse_provider_id() {
        let html = r#"<div aria-label="Session usage 42% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.provider, ProviderId::OllamaCloud);
    }

    // ---- window labels are 'Session' and 'Weekly' ----
    #[test]
    fn parse_window_labels() {
        let html = r#"<div aria-label="Session usage 42% used"></div>
        <div aria-label="Weekly usage 60% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.windows[0].label, "Session");
        assert_eq!(snapshot.windows[1].label, "Weekly");
    }

    // ---- window_seconds: 5*3600 for session, 7*24*3600 for weekly ----
    #[test]
    fn parse_window_seconds() {
        let html = r#"<div aria-label="Session usage 42% used"></div>
        <div aria-label="Weekly usage 60% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.windows[0].window_seconds, Some(5 * 3600));
        assert_eq!(snapshot.windows[1].window_seconds, Some(7 * 24 * 3600));
    }

    // ---- reset_description present ----
    #[test]
    fn parse_reset_description_present() {
        let html = r#"<div aria-label="Session usage 42% used"></div>
        <div aria-label="Weekly usage 60% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert!(snapshot.windows[0].reset_description.is_some());
        assert!(snapshot.windows[1].reset_description.is_some());
        assert_eq!(snapshot.windows[0].reset_description.as_deref(), Some("Rolling session window"));
        assert_eq!(snapshot.windows[1].reset_description.as_deref(), Some("Weekly window"));
    }

    // ---- updated_at passed through ----
    #[test]
    fn parse_updated_at_passed_through() {
        let html = r#"<div aria-label="Session usage 42% used"></div>"#;
        let ts = chrono::DateTime::parse_from_rfc3339("2026-01-15T09:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let snapshot = parse(html, ts).unwrap();
        assert_eq!(snapshot.updated_at, ts);
    }

    // ---- decimal percentages parsed ----
    #[test]
    fn parse_decimal_percentage() {
        let html = r#"<div aria-label="Session usage 33.5% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert!((snapshot.windows[0].used_percent - 33.5).abs() < 0.01);
    }

    // ---- invalid data-time is ignored gracefully ----
    #[test]
    fn parse_invalid_data_time_ignored() {
        let html = r#"<div aria-label="Session usage 42% used" data-time="not-a-date"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert!(snapshot.windows[0].reset_at.is_none());
    }

    // ---- exactly 100% is not clamped ----
    #[test]
    fn parse_exactly_100() {
        let html = r#"<div aria-label="Session usage 100% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert!((snapshot.windows[0].used_percent - 100.0).abs() < 0.01);
    }

    // ---- exactly 0% is not clamped ----
    #[test]
    fn parse_exactly_0() {
        let html = r#"<div aria-label="Session usage 0% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert!(snapshot.windows[0].used_percent.abs() < 0.01);
    }

    // ---- case-insensitive session/weekly keywords ----
    #[test]
    fn parse_case_insensitive_keywords() {
        let html = r#"<div aria-label="session usage 42% used"></div>
        <div aria-label="WEEKLY usage 60% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].label, "Session");
        assert_eq!(snapshot.windows[1].label, "Weekly");
    }

    // ---- weekly-only with no reset time yields None reset_at ----
    #[test]
    fn parse_weekly_only_no_reset_time() {
        let html = r#"<div aria-label="Weekly usage 60% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert!(snapshot.windows[0].reset_at.is_none());
    }

    // ---- headline is zero ----
    #[test]
    fn parse_headline_zero() {
        let html = r#"<div aria-label="Session usage 42% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert_eq!(snapshot.headline.0, 0);
    }

    // ---- provider_cost and extra_usage are None ----
    #[test]
    fn parse_no_cost_no_extra() {
        let html = r#"<div aria-label="Session usage 42% used"></div>"#;
        let snapshot = parse(html, Utc::now()).unwrap();
        assert!(snapshot.provider_cost.is_none());
        assert!(snapshot.extra_usage.is_none());
    }
    // ---- sync_managed_accounts: assigned scenarios ----

    #[test]
    fn sync_managed_accounts_empty_list_no_change() {
        let mut config = Config {
            ollama_cloud_managed_accounts: vec![],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(!changed);
        assert_eq!(config.ollama_cloud_managed_accounts.len(), 0);
    }

    #[test]
    fn sync_managed_accounts_keeps_stored_source() {
        let mut config = Config {
            ollama_cloud_managed_accounts: vec![ManagedOllamaCloudAccountConfig {
                id: "stored".to_string(),
                label: "stored".to_string(),
                session_cookie_source: "stored".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_authenticated_at: None,
            }],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(!changed);
        assert_eq!(config.ollama_cloud_managed_accounts.len(), 1);
        assert_eq!(config.ollama_cloud_managed_accounts[0].id, "stored");
    }

    #[test]
    fn sync_managed_accounts_keeps_env_source() {
        let mut config = Config {
            ollama_cloud_managed_accounts: vec![ManagedOllamaCloudAccountConfig {
                id: "env".to_string(),
                label: "env".to_string(),
                session_cookie_source: "env:FOO".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_authenticated_at: None,
            }],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(!changed);
        assert_eq!(config.ollama_cloud_managed_accounts.len(), 1);
        assert_eq!(config.ollama_cloud_managed_accounts[0].id, "env");
    }

    #[test]
    fn sync_managed_accounts_drops_empty_source() {
        let mut config = Config {
            ollama_cloud_managed_accounts: vec![ManagedOllamaCloudAccountConfig {
                id: "drop".to_string(),
                label: "drop".to_string(),
                session_cookie_source: String::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_authenticated_at: None,
            }],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(changed);
        assert_eq!(config.ollama_cloud_managed_accounts.len(), 0);
    }
}