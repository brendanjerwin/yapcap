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

pub use account::{OllamaCloudAccount, discover_accounts, remove_managed_config_dir};
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
    Regex::new(r#"aria-label="[^"]*[Ss]ession[^"]*?(\d+(?:\.\d+)?)\s*%"#).unwrap()
});
static RE_WEEKLY_USAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"aria-label="[^"]*[Ww]eekly[^"]*?(\d+(?:\.\d+)?)\s*%"#).unwrap()
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

pub async fn fetch(
    client: &reqwest::Client,
    account: &ManagedOllamaCloudAccountConfig,
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
        .header(
            "Cookie",
            format!("__Secure-session={session_cookie}"),
        )
        .send()
        .await
        .map_err(OllamaCloudError::DashboardRequest)?;

    let response = match response.status() {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            // Try refreshing the cookie from browser storage
            if let Some(fresh) = crate::browser_cookies::find_cookie("__Secure-session", "ollama.com").await {
                session_cookie = fresh.value.clone();
                if let Err(e) = crate::providers::ollama_cloud::storage::write_session_cookie(&account_root, &session_cookie) {
                    tracing::warn!(error = %e, "failed to persist refreshed session cookie");
                }
                // Retry with the fresh cookie
                client
                    .get(DASHBOARD_URL)
                    .header("User-Agent", USER_AGENT)
                    .header("Accept", "text/html")
                    .header(
                        "Cookie",
                        format!("__Secure-session={session_cookie}"),
                    )
                    .send()
                    .await
                    .map_err(OllamaCloudError::DashboardRequest)?
            } else {
                return Err(OllamaCloudError::LoginRequired);
            }
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok());
            return Err(OllamaCloudError::RateLimited {
                retry_after_secs: retry_after,
            });
        }
        status if status.is_server_error() => {
            return Err(OllamaCloudError::DashboardHttp {
                status: status.as_u16(),
            });
        }
        _ => response,
    };

    let response = response
        .error_for_status()
        .map_err(OllamaCloudError::DashboardEndpoint)?;
    let html = response
        .text()
        .await
        .map_err(OllamaCloudError::ReadDashboard)?;

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
}