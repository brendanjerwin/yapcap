// SPDX-License-Identifier: MPL-2.0

pub mod account;
pub mod login;
pub mod storage;

use crate::config::{Config, ManagedMinimaxAccountConfig, managed_minimax_account_dir};
use crate::error::MinimaxError;
use crate::model::{ProviderId, UsageSnapshot};
use chrono::Utc;
use serde::Deserialize;
use std::path::Path;

pub use account::{discover_accounts, remove_managed_config_dir};
pub use login::{
    MinimaxLoginEvent, MinimaxLoginState, MinimaxLoginStatus, prepare as prepare_login,
};
pub use storage::load_api_key;

const MINIMAX_API_URL: &str = "https://www.minimax.io/v1/token_plan/remains";
const MINIMAX_API_KEY_ENV: &str = "MINIMAX_API_KEY";

#[derive(Debug, Deserialize)]
struct MinimaxTokenPlanResponse {
    model_remains: Option<Vec<MinimaxModelRemains>>,
    base_resp: Option<MinimaxBaseResp>,
}

#[derive(Debug, Deserialize)]
struct MinimaxModelRemains {
    current_interval_total_count: Option<i64>,
    current_interval_usage_count: Option<i64>,
    current_weekly_total_count: Option<i64>,
    current_weekly_usage_count: Option<i64>,
    current_interval_remaining_percent: Option<i64>,
    current_weekly_remaining_percent: Option<i64>,
    model_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MinimaxBaseResp {
    status_code: Option<i32>,
    status_msg: Option<String>,
}

pub fn sync_managed_accounts(config: &mut Config) -> bool {
    let mut changed = false;
    let original_len = config.minimax_managed_accounts.len();
    config.minimax_managed_accounts.retain(|account| {
        if account.api_key_source.starts_with("env:") || !account.api_key_source.is_empty() {
            true
        } else {
            changed = true;
            false
        }
    });
    if config.minimax_managed_accounts.len() != original_len {
        changed = true;
    }
    changed
}

pub async fn fetch(
    client: &reqwest::Client,
    account: &ManagedMinimaxAccountConfig,
) -> Result<UsageSnapshot, MinimaxError> {
    fetch_at(
        client,
        &managed_minimax_account_dir(&account.id),
        MINIMAX_API_URL,
    )
    .await
}

async fn fetch_at(
    client: &reqwest::Client,
    account_root: &Path,
    endpoint: &str,
) -> Result<UsageSnapshot, MinimaxError> {
    let api_key = load_api_key(account_root)
        .ok()
        .filter(|key| !key.is_empty())
        .or_else(|| std::env::var(MINIMAX_API_KEY_ENV).ok())
        .ok_or(MinimaxError::LoginRequired)?;

    if api_key.is_empty() {
        return Err(MinimaxError::LoginRequired);
    }

    let response = client
        .get(endpoint)
        .bearer_auth(&api_key)
        .send()
        .await
        .map_err(MinimaxError::UsageRequest)?;

    match response.status() {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            return Err(MinimaxError::LoginRequired);
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok());
            return Err(MinimaxError::RateLimited {
                retry_after_secs: retry_after,
            });
        }
        status if status.is_server_error() => {
            return Err(MinimaxError::UsageHttp {
                status: status.as_u16(),
            });
        }
        _ => {}
    }

    let response = response
        .error_for_status()
        .map_err(MinimaxError::UsageEndpoint)?;
    let body = response.text().await.map_err(MinimaxError::UsageEndpoint)?;
    parse(&body, Utc::now())
}

pub fn parse(body: &str, updated_at: chrono::DateTime<Utc>) -> Result<UsageSnapshot, MinimaxError> {
    let response: MinimaxTokenPlanResponse =
        serde_json::from_str(body).map_err(MinimaxError::DecodeUsage)?;

    // Check for API error in base_resp first
    if let Some(base_resp) = &response.base_resp
        && let (Some(status_code), Some(status_msg)) =
            (&base_resp.status_code, &base_resp.status_msg)
        && *status_code != 0
    {
        return Err(MinimaxError::ApiError {
            status_code: *status_code,
            status_msg: (*status_msg).clone(),
        });
    }

    let model_remains = response.model_remains.ok_or(MinimaxError::ParseTokenPlan)?;

    if model_remains.is_empty() {
        return Err(MinimaxError::ParseTokenPlan);
    }

    let mut windows = Vec::new();

    for model in model_remains {
        let model_name = model
            .model_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        // Handle 5-hour window (interval)
        if let Some(remaining_percent) = model.current_interval_remaining_percent {
            let used_percent = (100.0 - remaining_percent as f32).clamp(0.0, 100.0);
            let label = if let (Some(total), Some(used)) = (
                model.current_interval_total_count,
                model.current_interval_usage_count,
            ) {
                if total > 0 {
                    format!("{} (5h): {}/{}", model_name, total - used, total)
                } else {
                    format!("{} (5h): {}", model_name, remaining_percent)
                }
            } else {
                format!("{} (5h): {}% remaining", model_name, remaining_percent)
            };
            windows.push(crate::model::UsageWindow {
                label,
                used_percent,
                reset_at: None,
                window_seconds: Some(5 * 3600),
                reset_description: Some("Resets every 5 hours".to_string()),
                group: None,
            });
        }

        // Handle weekly window
        if let Some(remaining_percent) = model.current_weekly_remaining_percent {
            let used_percent = (100.0 - remaining_percent as f32).clamp(0.0, 100.0);
            let label = if let (Some(total), Some(used)) = (
                model.current_weekly_total_count,
                model.current_weekly_usage_count,
            ) {
                if total > 0 {
                    format!("{} (Weekly): {}/{}", model_name, total - used, total)
                } else {
                    format!("{} (Weekly): {}", model_name, remaining_percent)
                }
            } else {
                format!("{} (Weekly): {}% remaining", model_name, remaining_percent)
            };
            windows.push(crate::model::UsageWindow {
                label,
                used_percent,
                reset_at: None,
                window_seconds: Some(7 * 24 * 3600),
                reset_description: Some("Resets weekly".to_string()),
                group: None,
            });
        }
    }

    if windows.is_empty() {
        return Err(MinimaxError::ParseTokenPlan);
    }

    Ok(UsageSnapshot {
        provider: ProviderId::Minimax,
        source: "API Key".to_string(),
        updated_at,
        headline: crate::model::UsageHeadline(0),
        windows,
        provider_cost: None,
        extra_usage: None,
        identity: crate::model::ProviderIdentity {
            email: None,
            account_id: None,
            plan: Some("Minimax.io".to_string()),
            display_name: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn parse_token_plan_response() {
        let body = r#"{"model_remains": [{"current_interval_total_count": 1000, "current_interval_usage_count": 500, "current_weekly_total_count": 10000, "current_weekly_usage_count": 8000, "model_name": "general", "current_interval_remaining_percent": 50, "current_weekly_remaining_percent": 20}], "base_resp": {"status_code": 0, "status_msg": "success"}}"#;
        let result = parse(body, Utc::now()).unwrap();
        assert_eq!(result.provider, ProviderId::Minimax);
        assert_eq!(result.windows.len(), 2);
        assert_eq!(result.windows[0].label, "general (5h): 500/1000");
        assert_eq!(result.windows[1].label, "general (Weekly): 2000/10000");
    }

    #[test]
    fn parse_empty_limits() {
        let body = r#"{"model_remains": [{"current_interval_total_count": 0, "current_interval_usage_count": 0, "current_weekly_total_count": 0, "current_weekly_usage_count": 0, "model_name": "general"}], "base_resp": {"status_code": 0, "status_msg": "success"}}"#;
        let result = parse(body, Utc::now());
        assert!(matches!(result, Err(MinimaxError::ParseTokenPlan)));
    }

    #[test]
    fn parse_missing_model_remains() {
        let body = r#"{"base_resp": {"status_code": 0, "status_msg": "success"}}"#;
        let result = parse(body, Utc::now());
        assert!(matches!(result, Err(MinimaxError::ParseTokenPlan)));
    }

    #[test]
    fn parse_api_error_with_base_resp() {
        let body = r#"{"base_resp": {"status_code": 2062, "status_msg": "no active token plan subscription"}, "model_remains": null}"#;
        let result = parse(body, Utc::now());
        assert!(matches!(
            result,
            Err(MinimaxError::ApiError { status_code: 2062, status_msg: ref msg }) if msg == "no active token plan subscription"
        ));
    }

    #[test]
    fn sync_managed_accounts_filters_empty_sources() {
        let mut config = Config {
            minimax_managed_accounts: vec![
                ManagedMinimaxAccountConfig {
                    id: "minimax-1".to_string(),
                    label: "Test".to_string(),
                    api_key_source: "env:MINIMAX_API_KEY".to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    last_authenticated_at: None,
                },
                ManagedMinimaxAccountConfig {
                    id: "minimax-2".to_string(),
                    label: "Empty".to_string(),
                    api_key_source: "".to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    last_authenticated_at: None,
                },
            ],
            ..Config::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(changed);
        assert_eq!(config.minimax_managed_accounts.len(), 1);
        assert_eq!(config.minimax_managed_accounts[0].id, "minimax-1");
    }
}
