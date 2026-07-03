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

#[derive(Debug, Clone)]
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

pub async fn fetch(
    client: &reqwest::Client,
    account: &ManagedOpencodeGoAccountConfig,
) -> Result<UsageSnapshot, OpencodeGoError> {
    let account_root = managed_opencode_go_account_dir(&account.id);

    let workspace_id = load_workspace_id(&account_root)
        .ok()
        .filter(|id| !id.is_empty())
        .ok_or(OpencodeGoError::LoginRequired)?;
    let auth_cookie = load_auth_cookie(&account_root)
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

    match response.status() {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            return Err(OpencodeGoError::LoginRequired);
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok());
            return Err(OpencodeGoError::RateLimited {
                retry_after_secs: retry_after,
            });
        }
        status if status.is_server_error() => {
            return Err(OpencodeGoError::DashboardHttp {
                status: status.as_u16(),
            });
        }
        _ => {}
    }

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
}