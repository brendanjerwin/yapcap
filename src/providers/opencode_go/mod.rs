// SPDX-License-Identifier: MPL-2.0

pub mod account;
pub mod login;
pub mod storage;

use crate::config::{Config, ManagedOpencodeGoAccountConfig, managed_opencode_go_account_dir};
use crate::error::OpencodeGoError;
use crate::model::{ProviderId, UsageSnapshot};
use chrono::Utc;
use regex::Regex;
use std::sync::LazyLock;

pub use account::{OpencodeGoAccount, discover_accounts, remove_managed_config_dir};
pub use login::{
    OpencodeGoLoginEvent, OpencodeGoLoginState, OpencodeGoLoginStatus, prepare as prepare_login,
};
pub use storage::{load_auth_cookie, load_workspace_id};

const DASHBOARD_URL_PREFIX: &str = "https://opencode.ai/workspace";
const DASHBOARD_URL_SUFFIX: &str = "/go";
const USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Gecko/20100101 Firefox/148.0";

// Regex patterns for SolidJS SSR hydration output.
// Field order may vary, so we try both orderings for each window.
static RE_ROLLING_PCT_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"rollingUsage:\$R\[\d+\]=\{[^}]*usagePercent:(-?\d+(?:\.\d+)?)[^}]*resetInSec:(-?\d+(?:\.\d+)?)[^}]*\}",
    )
    .unwrap()
});
static RE_ROLLING_RESET_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"rollingUsage:\$R\[\d+\]=\{[^}]*resetInSec:(-?\d+(?:\.\d+)?)[^}]*usagePercent:(-?\d+(?:\.\d+)?)[^}]*\}",
    )
    .unwrap()
});
static RE_WEEKLY_PCT_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"weeklyUsage:\$R\[\d+\]=\{[^}]*usagePercent:(-?\d+(?:\.\d+)?)[^}]*resetInSec:(-?\d+(?:\.\d+)?)[^}]*\}",
    )
    .unwrap()
});
static RE_WEEKLY_RESET_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"weeklyUsage:\$R\[\d+\]=\{[^}]*resetInSec:(-?\d+(?:\.\d+)?)[^}]*usagePercent:(-?\d+(?:\.\d+)?)[^}]*\}",
    )
    .unwrap()
});
static RE_MONTHLY_PCT_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"monthlyUsage:\$R\[\d+\]=\{[^}]*usagePercent:(-?\d+(?:\.\d+)?)[^}]*resetInSec:(-?\d+(?:\.\d+)?)[^}]*\}",
    )
    .unwrap()
});
static RE_MONTHLY_RESET_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"monthlyUsage:\$R\[\d+\]=\{[^}]*resetInSec:(-?\d+(?:\.\d+)?)[^}]*usagePercent:(-?\d+(?:\.\d+)?)[^}]*\}",
    )
    .unwrap()
});

#[derive(Debug, Clone, PartialEq)]
struct ScrapedWindowUsage {
    usage_percent: f64,
    reset_in_sec: f64,
}

fn parse_ssr_window(
    html: &str,
    re_pct_first: &Regex,
    re_reset_first: &Regex,
) -> Option<ScrapedWindowUsage> {
    if let Some(caps) = re_pct_first.captures(html) {
        if let (Ok(usage_percent), Ok(reset_in_sec)) =
            (caps[1].parse::<f64>(), caps[2].parse::<f64>())
        {
            if usage_percent.is_finite() && reset_in_sec.is_finite() {
                return Some(ScrapedWindowUsage {
                    usage_percent,
                    reset_in_sec,
                });
            }
        }
    }
    if let Some(caps) = re_reset_first.captures(html) {
        if let (Ok(reset_in_sec), Ok(usage_percent)) =
            (caps[1].parse::<f64>(), caps[2].parse::<f64>())
        {
            if usage_percent.is_finite() && reset_in_sec.is_finite() {
                return Some(ScrapedWindowUsage {
                    usage_percent,
                    reset_in_sec,
                });
            }
        }
    }
    None
}

/// Parse human-readable time strings like "1 hour 56 minutes", "6 days 2 hours"
/// into seconds.
fn parse_human_readable_time(time_str: &str) -> Option<f64> {
    let normalized = time_str.to_lowercase();
    let normalized = normalized.trim();
    if matches!(
        normalized,
        "reset-now" | "reset now" | "now" | "resets now"
    ) {
        return Some(0.0);
    }

    static RE_DAYS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*days?").unwrap());
    static RE_HOURS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*hours?").unwrap());
    static RE_MINUTES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*minutes?").unwrap());
    static RE_SECONDS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*seconds?").unwrap());

    let mut total_seconds = 0.0;
    let mut has_duration = false;

    if let Some(day_match) = RE_DAYS.captures(normalized) {
        total_seconds += day_match[1].parse::<f64>().unwrap_or(0.0) * 86400.0;
        has_duration = true;
    }
    if let Some(hour_match) = RE_HOURS.captures(normalized) {
        total_seconds += hour_match[1].parse::<f64>().unwrap_or(0.0) * 3600.0;
        has_duration = true;
    }
    if let Some(min_match) = RE_MINUTES.captures(normalized) {
        total_seconds += min_match[1].parse::<f64>().unwrap_or(0.0) * 60.0;
        has_duration = true;
    }
    if let Some(sec_match) = RE_SECONDS.captures(normalized) {
        total_seconds += sec_match[1].parse::<f64>().unwrap_or(0.0);
        has_duration = true;
    }

    if has_duration {
        Some(total_seconds)
    } else {
        None
    }
}

/// Parse the newer data-slot HTML format.
fn parse_data_slot_format(html: &str) -> Vec<(String, ScrapedWindowUsage)> {
    static RE_LABEL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"data-slot="usage-label">([^<]+)<"#).unwrap());
    static RE_USAGE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"data-slot="usage-value">[^0-9]*(\d+(?:\.\d+)?)"#).unwrap());
    static RE_RESET: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"data-slot="(reset-time|reset-now)">([\s\S]*?)</span>"#).unwrap()
    });

    let mut results = Vec::new();
    let items: Vec<&str> = html.split(r#"data-slot="usage-item""#).collect();

    for (i, content) in items.iter().enumerate() {
        if i == 0 {
            continue;
        }

        // Extract label
        let Some(label_match) = RE_LABEL.captures(content) else {
            continue;
        };
        let label = label_match[1].trim().to_lowercase();

        // Extract usage percentage
        let Some(usage_match) = RE_USAGE.captures(content) else {
            continue;
        };
        let usage_percent: f64 = match usage_match[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Extract reset time
        let Some(reset_match) = RE_RESET.captures(content) else {
            continue;
        };

        // Clean up SolidJS comments and "Resets in" prefix
        let reset_content = reset_match[2]
            .replace("<!--$-->", "")
            .replace("<!--/-->%", "")
            .replace("Resets in ", "")
            .replace("Reset in ", "")
            .trim()
            .to_string();

        let reset_in_sec = if &reset_match[1] == "reset-now" {
            0.0
        } else {
            match parse_human_readable_time(&reset_content) {
                Some(v) => v,
                None => continue,
            }
        };

        if !usage_percent.is_finite() || !reset_in_sec.is_finite() {
            continue;
        }

        let window_key = if label.contains("rolling") {
            "rolling".to_string()
        } else if label.contains("weekly") {
            "weekly".to_string()
        } else if label.contains("monthly") {
            "monthly".to_string()
        } else {
            continue;
        };

        results.push((
            window_key,
            ScrapedWindowUsage {
                usage_percent,
                reset_in_sec,
            },
        ));
    }

    results
}

pub fn sync_managed_accounts(config: &mut Config) -> bool {
    let mut changed = false;
    let original_len = config.opencode_go_managed_accounts.len();
    config
        .opencode_go_managed_accounts
        .retain(|account| {
            if account.auth_cookie_source.starts_with("env:") || !account.auth_cookie_source.is_empty() {
                true
            } else {
                changed = true;
                false
            }
        });
    if config.opencode_go_managed_accounts.len() != original_len {
        changed = true;
    }
    changed
}


/// What to do with an HTTP response from the dashboard.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DashboardResponseAction {
    /// Proceed normally — the response body is the dashboard HTML.
    Proceed,
    /// The cookie is invalid — try refreshing from browser storage.
    RefreshCookie,
    /// Rate limited — retry after the given number of seconds (if known).
    RateLimited { retry_after_secs: Option<u64> },
    /// Server error.
    ServerError { status: u16 },
}

/// Decide what to do based on the HTTP status code from the dashboard.
/// This is a pure function extracted from `fetch()` for testability.
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
    account: &ManagedOpencodeGoAccountConfig,
    cookie_source: &dyn crate::browser_cookies::CookieSource,
) -> Result<UsageSnapshot, OpencodeGoError> {
    let account_root = managed_opencode_go_account_dir(&account.id);


    let workspace_id = load_workspace_id(&account_root)
        .ok()
        .filter(|id| !id.is_empty())
        .ok_or(OpencodeGoError::LoginRequired)?;
    let mut auth_cookie = load_auth_cookie(&account_root)
        .ok()
        .filter(|c| !c.is_empty())
        .ok_or(OpencodeGoError::LoginRequired)?;

    let url = format!("{DASHBOARD_URL_PREFIX}/{workspace_id}{DASHBOARD_URL_SUFFIX}");

    let response = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "text/html")
        .header("Cookie", format!("auth={auth_cookie}"))
        .send()
        .await
        .map_err(OpencodeGoError::DashboardRequest)?;

    let response = match handle_status_code(response.status(), response.headers()) {
        DashboardResponseAction::Proceed => response,
        DashboardResponseAction::RefreshCookie => {
            if let Some(fresh) = cookie_source.find_cookie("auth", "opencode.ai").await {
                auth_cookie = fresh.value.clone();
                if let Err(e) = crate::providers::opencode_go::storage::write_auth_cookie(&account_root, &auth_cookie) {
                    tracing::warn!(error = %e, "failed to persist refreshed auth cookie");
                }
                client
                    .get(&url)
                    .header("User-Agent", USER_AGENT)
                    .header("Accept", "text/html")
                    .header("Cookie", format!("auth={auth_cookie}"))
                    .send()
                    .await
                    .map_err(OpencodeGoError::DashboardRequest)?
            } else {
                return Err(OpencodeGoError::LoginRequired);
            }
        }
        DashboardResponseAction::RateLimited { retry_after_secs } => {
            return Err(OpencodeGoError::RateLimited { retry_after_secs });
        }
        DashboardResponseAction::ServerError { status } => {
            return Err(OpencodeGoError::DashboardHttp { status });
        }
    };

    let response = response
        .error_for_status()
        .map_err(OpencodeGoError::DashboardEndpoint)?;
    let html = response.text().await.map_err(OpencodeGoError::ReadDashboard)?;

    parse(&html, Utc::now(), &workspace_id)
}

pub fn parse(
    html: &str,
    updated_at: chrono::DateTime<Utc>,
    workspace_id: &str,
) -> Result<UsageSnapshot, OpencodeGoError> {
    // Try SolidJS SSR format first
    let rolling = parse_ssr_window(html, &RE_ROLLING_PCT_FIRST, &RE_ROLLING_RESET_FIRST);
    let weekly = parse_ssr_window(html, &RE_WEEKLY_PCT_FIRST, &RE_WEEKLY_RESET_FIRST);
    let monthly = parse_ssr_window(html, &RE_MONTHLY_PCT_FIRST, &RE_MONTHLY_RESET_FIRST);

    // Fall back to data-slot HTML format if SSR found nothing
    let (rolling, weekly, monthly) = if rolling.is_none() && weekly.is_none() && monthly.is_none() {
        let slots = parse_data_slot_format(html);
        let mut r = None;
        let mut w = None;
        let mut m = None;
        for (key, usage) in slots {
            match key.as_str() {
                "rolling" => r = Some(usage),
                "weekly" => w = Some(usage),
                "monthly" => m = Some(usage),
                _ => {}
            }
        }
        (r, w, m)
    } else {
        (rolling, weekly, monthly)
    };

    if rolling.is_none() && weekly.is_none() && monthly.is_none() {
        return Err(OpencodeGoError::ParseDashboard);
    }

    let mut windows = Vec::new();

    if let Some(usage) = rolling {
        let used_percent = usage.usage_percent.clamp(0.0, 100.0) as f32;
        let reset_at = Some(updated_at + chrono::Duration::seconds(usage.reset_in_sec.max(0.0) as i64));
        windows.push(crate::model::UsageWindow {
            label: "5h".to_string(),
            used_percent,
            reset_at,
            window_seconds: Some(5 * 3600),
            reset_description: Some("Rolling 5 hour window".to_string()),
        });
    }

    if let Some(usage) = weekly {
        let used_percent = usage.usage_percent.clamp(0.0, 100.0) as f32;
        let reset_at = Some(updated_at + chrono::Duration::seconds(usage.reset_in_sec.max(0.0) as i64));
        windows.push(crate::model::UsageWindow {
            label: "Weekly".to_string(),
            used_percent,
            reset_at,
            window_seconds: Some(7 * 24 * 3600),
            reset_description: Some("Weekly window".to_string()),
        });
    }

    if let Some(usage) = monthly {
        let used_percent = usage.usage_percent.clamp(0.0, 100.0) as f32;
        let reset_at = Some(updated_at + chrono::Duration::seconds(usage.reset_in_sec.max(0.0) as i64));
        windows.push(crate::model::UsageWindow {
            label: "Monthly".to_string(),
            used_percent,
            reset_at,
            window_seconds: Some(30 * 24 * 3600),
            reset_description: Some("Monthly window".to_string()),
        });
    }

    Ok(UsageSnapshot {
        provider: ProviderId::OpencodeGo,
        source: "Dashboard".to_string(),
        updated_at,
        headline: crate::model::UsageHeadline(0),
        windows,
        provider_cost: None,
        extra_usage: None,
        identity: crate::model::ProviderIdentity {
            email: None,
            account_id: Some(workspace_id.to_string()),
            plan: Some("OpenCode Go".to_string()),
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
    fn parse_ssr_format() {
        let html = r#"<html><body>
        rollingUsage:$R[1]={usagePercent:42.5,resetInSec:12345}
        weeklyUsage:$R[2]={resetInSec:54321,usagePercent:60}
        monthlyUsage:$R[3]={usagePercent:75,resetInSec:99999}
        </body></html>"#;
        let snapshot = parse(html, Utc::now(), "wrk_test").unwrap();
        assert_eq!(snapshot.provider, ProviderId::OpencodeGo);
        assert_eq!(snapshot.windows.len(), 3);
        assert!((snapshot.windows[0].used_percent - 42.5).abs() < 0.01);
        assert!((snapshot.windows[1].used_percent - 60.0).abs() < 0.01);
        assert!((snapshot.windows[2].used_percent - 75.0).abs() < 0.01);
    }

    #[test]
    fn parse_data_slot_format_fallback() {
        let html = r#"<html><body>
        <div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">25%</span>
          <span data-slot="reset-time">Resets in 2 hours 30 minutes</span>
        </div>
        <div data-slot="usage-item">
          <span data-slot="usage-label">Weekly Usage</span>
          <span data-slot="usage-value">50%</span>
          <span data-slot="reset-time">Resets in 3 days 4 hours</span>
        </div>
        </body></html>"#;
        let snapshot = parse(html, Utc::now(), "wrk_test").unwrap();
        assert_eq!(snapshot.windows.len(), 2);
        assert!((snapshot.windows[0].used_percent - 25.0).abs() < 0.01);
        assert!((snapshot.windows[1].used_percent - 50.0).abs() < 0.01);
    }

    #[test]
    fn parse_no_data_returns_error() {
        let html = "<html><body>no usage data here</body></html>";
        let result = parse(html, Utc::now(), "wrk_test");
        assert!(result.is_err());
        match result {
            Err(OpencodeGoError::ParseDashboard) => {}
            _ => panic!("expected ParseDashboard error"),
        }
    }
    // ---- SSR format: rolling/weekly/monthly pct-first vs reset-first orderings ----

    #[test]
    fn parse_ssr_rolling_pct_first() {
        let html = r#"rollingUsage:$R[0]={usagePercent:42,resetInSec:3600}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].label, "5h");
        assert!((snap.windows[0].used_percent - 42.0).abs() < 0.01);
    }

    #[test]
    fn parse_ssr_rolling_reset_first() {
        let html = r#"rollingUsage:$R[0]={resetInSec:3600,usagePercent:42}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert!((snap.windows[0].used_percent - 42.0).abs() < 0.01);
    }

    #[test]
    fn parse_ssr_weekly_pct_first() {
        let html = r#"weeklyUsage:$R[1]={usagePercent:70,resetInSec:7200}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].label, "Weekly");
        assert!((snap.windows[0].used_percent - 70.0).abs() < 0.01);
    }

    #[test]
    fn parse_ssr_weekly_reset_first() {
        let html = r#"weeklyUsage:$R[1]={resetInSec:7200,usagePercent:70}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert!((snap.windows[0].used_percent - 70.0).abs() < 0.01);
    }

    #[test]
    fn parse_ssr_monthly_pct_first() {
        let html = r#"monthlyUsage:$R[2]={usagePercent:80,resetInSec:10800}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].label, "Monthly");
        assert!((snap.windows[0].used_percent - 80.0).abs() < 0.01);
    }

    #[test]
    fn parse_ssr_monthly_reset_first() {
        let html = r#"monthlyUsage:$R[2]={resetInSec:10800,usagePercent:80}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert!((snap.windows[0].used_percent - 80.0).abs() < 0.01);
    }

    // ---- SSR format: decimal / boundary percentages ----

    #[test]
    fn parse_ssr_decimal_percent() {
        let html = r#"rollingUsage:$R[0]={usagePercent:42.5,resetInSec:60}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert!((snap.windows[0].used_percent - 42.5).abs() < 0.01);
    }

    #[test]
    fn parse_ssr_zero_percent() {
        let html = r#"rollingUsage:$R[0]={usagePercent:0,resetInSec:60}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert!((snap.windows[0].used_percent - 0.0).abs() < 0.01);
    }

    #[test]
    fn parse_ssr_hundred_percent() {
        let html = r#"rollingUsage:$R[0]={usagePercent:100,resetInSec:60}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert!((snap.windows[0].used_percent - 100.0).abs() < 0.01);
    }

    // ---- Percentage clamping > 100 ----

    #[test]
    fn parse_ssr_clamps_over_100() {
        let html = r#"rollingUsage:$R[0]={usagePercent:150,resetInSec:60}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert!((snap.windows[0].used_percent - 100.0).abs() < 0.01,
            "expected clamping to 100, got {}", snap.windows[0].used_percent);
    }

    #[test]
    fn parse_data_slot_clamps_over_100() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">200%</span>
          <span data-slot="reset-now"></span>
        </div>"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert!((snap.windows[0].used_percent - 100.0).abs() < 0.01);
    }

    // ---- Data-slot format: labels + reset-now vs reset-time ----

    #[test]
    fn parse_data_slot_rolling() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">25%</span>
          <span data-slot="reset-time">Resets in 1 hour 0 minutes</span>
        </div>"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].label, "5h");
        assert!((snap.windows[0].used_percent - 25.0).abs() < 0.01);
    }

    #[test]
    fn parse_data_slot_weekly() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Weekly Usage</span>
          <span data-slot="usage-value">50%</span>
          <span data-slot="reset-time">Resets in 2 days 3 hours</span>
        </div>"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].label, "Weekly");
        assert!((snap.windows[0].used_percent - 50.0).abs() < 0.01);
    }

    #[test]
    fn parse_data_slot_monthly() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Monthly Usage</span>
          <span data-slot="usage-value">75%</span>
          <span data-slot="reset-time">Resets in 10 days</span>
        </div>"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].label, "Monthly");
        assert!((snap.windows[0].used_percent - 75.0).abs() < 0.01);
    }

    #[test]
    fn parse_data_slot_reset_now() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">42%</span>
          <span data-slot="reset-now"></span>
        </div>"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        // reset-now => reset_in_sec 0 => reset_at == updated_at
        let ts = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00")
            .unwrap().with_timezone(&Utc);
        let html2 = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">42%</span>
          <span data-slot="reset-now"></span>
        </div>"#;
        let snap2 = parse(html2, ts, "ws1").unwrap();
        assert_eq!(snap2.windows[0].reset_at, Some(ts));
    }

    #[test]
    fn parse_data_slot_all_three() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">10%</span>
          <span data-slot="reset-time">Resets in 1 hour</span>
        </div>
        <div data-slot="usage-item">
          <span data-slot="usage-label">Weekly Usage</span>
          <span data-slot="usage-value">20%</span>
          <span data-slot="reset-time">Resets in 2 days</span>
        </div>
        <div data-slot="usage-item">
          <span data-slot="usage-label">Monthly Usage</span>
          <span data-slot="usage-value">30%</span>
          <span data-slot="reset-time">Resets in 5 days</span>
        </div>"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 3);
    }

    // ---- parse_human_readable_time ----

    #[test]
    fn human_readable_hour_minutes() {
        let secs = parse_human_readable_time("1 hour 56 minutes").unwrap();
        assert!((secs - (3600.0 + 56.0 * 60.0)).abs() < 0.01);
    }

    #[test]
    fn human_readable_days_hours() {
        let secs = parse_human_readable_time("6 days 2 hours").unwrap();
        assert!((secs - (6.0 * 86400.0 + 2.0 * 3600.0)).abs() < 0.01);
    }

    #[test]
    fn human_readable_seconds() {
        let secs = parse_human_readable_time("0 seconds").unwrap();
        assert!((secs - 0.0).abs() < 0.01);
    }

    #[test]
    fn human_readable_reset_now() {
        assert_eq!(parse_human_readable_time("reset-now"), Some(0.0));
        assert_eq!(parse_human_readable_time("reset now"), Some(0.0));
        assert_eq!(parse_human_readable_time("now"), Some(0.0));
        assert_eq!(parse_human_readable_time("resets now"), Some(0.0));
    }

    #[test]
    fn human_readable_empty() {
        assert_eq!(parse_human_readable_time(""), None);
    }

    #[test]
    fn human_readable_garbage() {
        assert_eq!(parse_human_readable_time("garbage"), None);
    }

    #[test]
    fn human_readable_seconds_only() {
        let secs = parse_human_readable_time("30 seconds").unwrap();
        assert!((secs - 30.0).abs() < 0.01);
    }

    #[test]
    fn human_readable_singular_units() {
        let secs = parse_human_readable_time("1 day 1 hour 1 minute 1 second").unwrap();
        assert!((secs - (86400.0 + 3600.0 + 60.0 + 1.0)).abs() < 0.01);
    }

    #[test]
    fn human_readable_case_insensitive() {
        let secs = parse_human_readable_time("1 HOUR 2 MINUTES").unwrap();
        assert!((secs - (3600.0 + 120.0)).abs() < 0.01);
    }

    #[test]
    fn human_readable_decimal_days() {
        let secs = parse_human_readable_time("1.5 days").unwrap();
        assert!((secs - 1.5 * 86400.0).abs() < 0.01);
    }

    // ---- parse_data_slot_format directly (reset-now vs reset-time, label classification) ----

    #[test]
    fn data_slot_format_reset_time_and_reset_now() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">5%</span>
          <span data-slot="reset-now"></span>
        </div>
        <div data-slot="usage-item">
          <span data-slot="usage-label">Weekly Usage</span>
          <span data-slot="usage-value">15%</span>
          <span data-slot="reset-time">Resets in 1 day</span>
        </div>"#;
        let slots = parse_data_slot_format(html);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].0, "rolling");
        assert!((slots[0].1.reset_in_sec - 0.0).abs() < 0.01);
        assert_eq!(slots[1].0, "weekly");
        assert!((slots[1].1.reset_in_sec - 86400.0).abs() < 0.01);
    }

    #[test]
    fn data_slot_format_skips_unknown_label() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Unknown Window</span>
          <span data-slot="usage-value">5%</span>
          <span data-slot="reset-now"></span>
        </div>"#;
        let slots = parse_data_slot_format(html);
        assert!(slots.is_empty(), "unknown label should be skipped");
    }

    #[test]
    fn data_slot_format_missing_label_skipped() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-value">5%</span>
          <span data-slot="reset-now"></span>
        </div>"#;
        let slots = parse_data_slot_format(html);
        assert!(slots.is_empty());
    }

    #[test]
    fn data_slot_format_missing_usage_skipped() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="reset-now"></span>
        </div>"#;
        let slots = parse_data_slot_format(html);
        assert!(slots.is_empty());
    }

    #[test]
    fn data_slot_format_missing_reset_skipped() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">5%</span>
        </div>"#;
        let slots = parse_data_slot_format(html);
        assert!(slots.is_empty());
    }

    #[test]
    fn data_slot_format_unparsable_reset_time_skipped() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">5%</span>
          <span data-slot="reset-time">garbage reset text</span>
        </div>"#;
        let slots = parse_data_slot_format(html);
        assert!(slots.is_empty());
    }

    #[test]
    fn data_slot_format_decimal_percent() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">42.5%</span>
          <span data-slot="reset-now"></span>
        </div>"#;
        let slots = parse_data_slot_format(html);
        assert_eq!(slots.len(), 1);
        assert!((slots[0].1.usage_percent - 42.5).abs() < 0.01);
    }

    #[test]
    fn data_slot_format_strips_solidjs_comments() {
        let html = r#"<div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">42%</span>
          <span data-slot="reset-time"><!--$-->Resets in 2 hours<!--/-->%</span>
        </div>"#;
        let slots = parse_data_slot_format(html);
        assert_eq!(slots.len(), 1);
        assert!((slots[0].1.reset_in_sec - 7200.0).abs() < 0.01);
    }

    // ---- parse_ssr_window directly: None cases ----

    #[test]
    fn parse_ssr_window_returns_none_when_no_match() {
        let re_pct = Regex::new(r"rollingUsage:\$R\[\d+\]=\{[^}]*usagePercent:(-?\d+(?:\.\d+)?)[^}]*resetInSec:(-?\d+(?:\.\d+)?)[^}]*\}").unwrap();
        let re_reset = Regex::new(r"rollingUsage:\$R\[\d+\]=\{[^}]*resetInSec:(-?\d+(?:\.\d+)?)[^}]*usagePercent:(-?\d+(?:\.\d+)?)[^}]*\}").unwrap();
        assert_eq!(parse_ssr_window("nothing here", &re_pct, &re_reset), None);
    }

    #[test]
    fn parse_ssr_window_returns_none_on_nan() {
        // Non-numeric capture groups cannot occur because the regex only matches digits/dots,
        // so to exercise the parse-failure branch we use a regex that captures a non-numeric
        // placeholder and confirm parse_ssr_window returns None.
        let re_pct = Regex::new(r"key:\{usagePercent:([a-z]+),resetInSec:([a-z]+)\}").unwrap();
        let re_reset = Regex::new(r"key:\{resetInSec:([a-z]+),usagePercent:([a-z]+)\}").unwrap();
        let html = "key:{usagePercent:abc,resetInSec:def}";
        assert_eq!(parse_ssr_window(html, &re_pct, &re_reset), None);
    }

    #[test]
    fn parse_ssr_window_pct_first_wins_over_reset_first() {
        let re_pct = Regex::new(r"key:\{usagePercent:(\d+),resetInSec:(\d+)\}").unwrap();
        let re_reset = Regex::new(r"key:\{resetInSec:(\d+),usagePercent:(\d+)\}").unwrap();
        let html = "key:{usagePercent:42,resetInSec:60}";
        let r = parse_ssr_window(html, &re_pct, &re_reset).unwrap();
        assert!((r.usage_percent - 42.0).abs() < 0.01);
        assert!((r.reset_in_sec - 60.0).abs() < 0.01);
    }

    // ---- Partial data: only rolling / only weekly / only monthly / rolling+weekly ----

    #[test]
    fn parse_partial_only_rolling() {
        let html = r#"rollingUsage:$R[0]={usagePercent:10,resetInSec:60}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].label, "5h");
    }

    #[test]
    fn parse_partial_only_weekly() {
        let html = r#"weeklyUsage:$R[0]={usagePercent:10,resetInSec:60}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].label, "Weekly");
    }

    #[test]
    fn parse_partial_only_monthly() {
        let html = r#"monthlyUsage:$R[0]={usagePercent:10,resetInSec:60}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].label, "Monthly");
    }

    #[test]
    fn parse_partial_rolling_and_weekly_no_monthly() {
        let html = r#"rollingUsage:$R[0]={usagePercent:10,resetInSec:60}
        weeklyUsage:$R[1]={usagePercent:20,resetInSec:120}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows.len(), 2);
        assert_eq!(snap.windows[0].label, "5h");
        assert_eq!(snap.windows[1].label, "Weekly");
    }

    // ---- Workspace ID passed through to snapshot identity ----

    #[test]
    fn parse_workspace_id_passed_through() {
        let html = r#"rollingUsage:$R[0]={usagePercent:42,resetInSec:60}"#;
        let snap = parse(html, Utc::now(), "wrk_abc123").unwrap();
        assert_eq!(snap.identity.account_id, Some("wrk_abc123".to_string()));
        assert_eq!(snap.identity.plan, Some("OpenCode Go".to_string()));
        assert_eq!(snap.provider, ProviderId::OpencodeGo);
        assert_eq!(snap.source, "Dashboard");
    }

    // ---- reset_at computation: uses updated_at + reset_in_sec ----

    #[test]
    fn parse_reset_at_uses_updated_at_plus_reset_in_sec() {
        let ts = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00")
            .unwrap().with_timezone(&Utc);
        let html = r#"rollingUsage:$R[0]={usagePercent:50,resetInSec:3600}
        weeklyUsage:$R[1]={usagePercent:60,resetInSec:7200}
        monthlyUsage:$R[2]={usagePercent:70,resetInSec:10800}"#;
        let snap = parse(html, ts, "ws1").unwrap();
        assert_eq!(snap.windows[0].reset_at, Some(ts + chrono::Duration::seconds(3600)));
        assert_eq!(snap.windows[1].reset_at, Some(ts + chrono::Duration::seconds(7200)));
        assert_eq!(snap.windows[2].reset_at, Some(ts + chrono::Duration::seconds(10800)));
    }

    #[test]
    fn parse_negative_reset_clamped_to_zero() {
        let ts = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00+00:00")
            .unwrap().with_timezone(&Utc);
        let html = r#"rollingUsage:$R[0]={usagePercent:50,resetInSec:-100}"#;
        let snap = parse(html, ts, "ws1").unwrap();
        // reset_in_sec.max(0.0) -> 0, reset_at == ts
        assert_eq!(snap.windows[0].reset_at, Some(ts));
    }

    // ---- window_seconds per window ----

    #[test]
    fn parse_window_seconds_correct() {
        let html = r#"rollingUsage:$R[0]={usagePercent:1,resetInSec:60}
        weeklyUsage:$R[1]={usagePercent:2,resetInSec:60}
        monthlyUsage:$R[2]={usagePercent:3,resetInSec:60}"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        assert_eq!(snap.windows[0].window_seconds, Some(5 * 3600));
        assert_eq!(snap.windows[1].window_seconds, Some(7 * 24 * 3600));
        assert_eq!(snap.windows[2].window_seconds, Some(30 * 24 * 3600));
    }

    // ---- SSR takes precedence over data-slot (mixed) ----

    #[test]
    fn parse_ssr_takes_precedence_over_data_slot() {
        let html = r#"rollingUsage:$R[0]={usagePercent:99,resetInSec:60}
        <div data-slot="usage-item">
          <span data-slot="usage-label">Rolling Usage</span>
          <span data-slot="usage-value">1%</span>
          <span data-slot="reset-now"></span>
        </div>"#;
        let snap = parse(html, Utc::now(), "ws1").unwrap();
        // SSR found rolling => data-slot fallback not consulted
        assert_eq!(snap.windows.len(), 1);
        assert!((snap.windows[0].used_percent - 99.0).abs() < 0.01);
    }

    // ---- sync_managed_accounts retains env and non-empty sources, drops empty ----

    #[test]
    fn sync_managed_accounts_drops_empty_non_env_source() {
        let mut config = Config {
            opencode_go_managed_accounts: vec![
                ManagedOpencodeGoAccountConfig {
                    id: "keep-env".to_string(),
                    label: "env".to_string(),
                    workspace_id: String::new(),
                    auth_cookie_source: "env:VAR".to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    last_authenticated_at: None,
                },
                ManagedOpencodeGoAccountConfig {
                    id: "keep-path".to_string(),
                    label: "path".to_string(),
                    workspace_id: String::new(),
                    auth_cookie_source: "/some/path".to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    last_authenticated_at: None,
                },
                ManagedOpencodeGoAccountConfig {
                    id: "drop".to_string(),
                    label: "drop".to_string(),
                    workspace_id: String::new(),
                    auth_cookie_source: String::new(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    last_authenticated_at: None,
                },
            ],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(changed);
        assert_eq!(config.opencode_go_managed_accounts.len(), 2);
        assert!(config.opencode_go_managed_accounts.iter().all(|a| a.id != "drop"));
    }

    #[test]
    fn sync_managed_accounts_no_change_when_all_kept() {
        let mut config = Config {
            opencode_go_managed_accounts: vec![ManagedOpencodeGoAccountConfig {
                id: "keep".to_string(),
                label: "keep".to_string(),
                workspace_id: String::new(),
                auth_cookie_source: "env:VAR".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_authenticated_at: None,
            }],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(!changed);
        assert_eq!(config.opencode_go_managed_accounts.len(), 1);
    }
    // ---- sync_managed_accounts: assigned scenarios ----

    #[test]
    fn sync_managed_accounts_empty_list_no_change() {
        let mut config = Config {
            opencode_go_managed_accounts: vec![],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(!changed);
        assert_eq!(config.opencode_go_managed_accounts.len(), 0);
    }

    #[test]
    fn sync_managed_accounts_keeps_stored_source() {
        let mut config = Config {
            opencode_go_managed_accounts: vec![ManagedOpencodeGoAccountConfig {
                id: "stored".to_string(),
                label: "stored".to_string(),
                workspace_id: String::new(),
                auth_cookie_source: "stored".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_authenticated_at: None,
            }],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(!changed);
        assert_eq!(config.opencode_go_managed_accounts.len(), 1);
        assert_eq!(config.opencode_go_managed_accounts[0].id, "stored");
    }

    #[test]
    fn sync_managed_accounts_keeps_env_source() {
        let mut config = Config {
            opencode_go_managed_accounts: vec![ManagedOpencodeGoAccountConfig {
                id: "env".to_string(),
                label: "env".to_string(),
                workspace_id: String::new(),
                auth_cookie_source: "env:FOO".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_authenticated_at: None,
            }],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(!changed);
        assert_eq!(config.opencode_go_managed_accounts.len(), 1);
        assert_eq!(config.opencode_go_managed_accounts[0].id, "env");
    }

    #[test]
    fn sync_managed_accounts_drops_single_empty_source() {
        let mut config = Config {
            opencode_go_managed_accounts: vec![ManagedOpencodeGoAccountConfig {
                id: "drop".to_string(),
                label: "drop".to_string(),
                workspace_id: String::new(),
                auth_cookie_source: String::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_authenticated_at: None,
            }],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(changed);
        assert_eq!(config.opencode_go_managed_accounts.len(), 0);
    }

    #[test]
    fn sync_managed_accounts_mixed_stored_and_empty() {
        let mut config = Config {
            opencode_go_managed_accounts: vec![
                ManagedOpencodeGoAccountConfig {
                    id: "keep".to_string(),
                    label: "keep".to_string(),
                    workspace_id: String::new(),
                    auth_cookie_source: "stored".to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    last_authenticated_at: None,
                },
                ManagedOpencodeGoAccountConfig {
                    id: "drop".to_string(),
                    label: "drop".to_string(),
                    workspace_id: String::new(),
                    auth_cookie_source: String::new(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    last_authenticated_at: None,
                },
            ],
            ..Default::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(changed);
        assert_eq!(config.opencode_go_managed_accounts.len(), 1);
        assert_eq!(config.opencode_go_managed_accounts[0].id, "keep");
    }
}