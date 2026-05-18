// SPDX-License-Identifier: MPL-2.0

pub mod account;
pub mod device_flow;
pub mod headers;
pub mod login;
pub mod parse;
pub mod storage;

use crate::config::{Config, ManagedCopilotAccountConfig, managed_copilot_account_dir};
use crate::error::CopilotError;
use crate::model::UsageSnapshot;
use chrono::Utc;
use std::path::Path;

pub use account::apply_login_account;
pub use login::{
    CopilotLoginEvent, CopilotLoginState, CopilotLoginStatus, prepare, prepare_for_reauth,
};

pub fn sync_managed_accounts(config: &mut Config) -> bool {
    let mut changed = false;
    let mut seen = std::collections::HashSet::new();
    let original_len = config.copilot_managed_accounts.len();
    config.copilot_managed_accounts.retain(|account| {
        if seen.insert(account.github_user_id) {
            true
        } else {
            changed = true;
            false
        }
    });
    if config.copilot_managed_accounts.len() != original_len {
        changed = true;
    }
    changed
}

pub async fn fetch(
    client: &reqwest::Client,
    account: &ManagedCopilotAccountConfig,
) -> Result<UsageSnapshot, CopilotError> {
    fetch_at(
        client,
        &managed_copilot_account_dir(&account.id),
        headers::COPILOT_USER_URL,
    )
    .await
}

async fn fetch_at(
    client: &reqwest::Client,
    account_root: &Path,
    endpoint: &str,
) -> Result<UsageSnapshot, CopilotError> {
    let tokens = storage::load_tokens(account_root)
        .map_err(CopilotError::AccountStorage)
        .and_then(|tokens| {
            if tokens.access_token.trim().is_empty() {
                Err(CopilotError::LoginRequired)
            } else {
                Ok(tokens)
            }
        })?;

    let response = headers::apply_copilot_headers(client.get(endpoint), tokens.access_token.trim())
        .send()
        .await
        .map_err(CopilotError::UsageRequest)?;

    match response.status() {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            return Err(CopilotError::LoginRequired);
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            return Err(CopilotError::RateLimited {
                retry_after_secs: retry_after_secs(response.headers()),
            });
        }
        status if status.is_server_error() => {
            return Err(CopilotError::UsageHttp {
                status: status.as_u16(),
            });
        }
        _ => {}
    }

    let response = response
        .error_for_status()
        .map_err(CopilotError::UsageEndpoint)?;
    let body = response.text().await.map_err(CopilotError::DecodeUsage)?;
    parse::parse(&body, Utc::now())
}

fn retry_after_secs(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::{Duration, sleep};

    async fn server(
        status: u16,
        body: String,
        extra_headers: &[(&str, &str)],
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let headers = extra_headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0u8; 8192];
            let n = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..n]).to_string();
            let raw = format!(
                "HTTP/1.1 {} OK\r\ncontent-type: application/json\r\n{}content-length: {}\r\nconnection: close\r\n\r\n{}",
                status,
                headers,
                body.len(),
                body,
            );
            stream.write_all(raw.as_bytes()).await.unwrap();
            request
        });
        (format!("http://{addr}/copilot_internal/user"), handle)
    }

    fn free_body() -> String {
        r#"{
            "access_type_sku": "free_limited_copilot",
            "login": "octocat",
            "monthly_quotas": { "chat": 500, "completions": 300 },
            "limited_user_quotas": { "chat": 350, "completions": 60 },
            "limited_user_reset_date": "2026-06-03"
        }"#
        .to_string()
    }

    fn write_test_account(root: &std::path::Path) {
        storage::write_account(
            root,
            &storage::CopilotTokens {
                access_token: "ghu_test".to_string(),
            },
            &storage::CopilotMetadata {
                github_user_id: 1,
                login: "octocat".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_authenticated_at: Some(Utc::now()),
            },
        )
        .unwrap();
    }

    #[tokio::test]
    async fn fetch_requests_usage_with_copilot_headers() {
        let temp = tempdir().unwrap();
        write_test_account(temp.path());
        let (endpoint, handle) = server(200, free_body(), &[]).await;

        let snapshot = fetch_at(&reqwest::Client::new(), temp.path(), &endpoint)
            .await
            .unwrap();

        assert_eq!(snapshot.identity.display_name.as_deref(), Some("octocat"));
        assert!((snapshot.windows[1].used_percent - 80.0).abs() < 0.001);
        let request = handle.await.unwrap();
        assert!(request.starts_with("GET /copilot_internal/user HTTP/1.1\r\n"));
        assert!(request.contains("authorization: token ghu_test\r\n"));
        assert!(request.contains("accept: application/json\r\n"));
        assert!(request.contains("editor-version: vscode/1.107.0\r\n"));
        assert!(request.contains("editor-plugin-version: copilot-chat/0.35.0\r\n"));
        assert!(request.contains("user-agent: GitHubCopilotChat/0.35.0\r\n"));
        assert!(request.contains("x-github-api-version: 2026-03-10\r\n"));
    }

    #[tokio::test]
    async fn fetch_maps_unauthorized_to_login_required() {
        let temp = tempdir().unwrap();
        write_test_account(temp.path());
        let (endpoint, handle) = server(401, "{}".to_string(), &[]).await;

        let error = fetch_at(&reqwest::Client::new(), temp.path(), &endpoint)
            .await
            .unwrap_err();

        assert!(matches!(error, CopilotError::LoginRequired));
        assert_eq!(
            handle
                .await
                .unwrap()
                .matches("GET /copilot_internal/user")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn fetch_maps_forbidden_to_login_required() {
        let temp = tempdir().unwrap();
        write_test_account(temp.path());
        let (endpoint, _handle) = server(403, "{}".to_string(), &[]).await;

        let error = fetch_at(&reqwest::Client::new(), temp.path(), &endpoint)
            .await
            .unwrap_err();

        assert!(matches!(error, CopilotError::LoginRequired));
        assert!(error.requires_user_action());
        assert!(!error.is_transient());
    }

    #[tokio::test]
    async fn fetch_maps_retry_after_rate_limit() {
        let temp = tempdir().unwrap();
        write_test_account(temp.path());
        let (endpoint, _handle) = server(429, "{}".to_string(), &[("Retry-After", "42")]).await;

        let error = fetch_at(&reqwest::Client::new(), temp.path(), &endpoint)
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            CopilotError::RateLimited {
                retry_after_secs: Some(42)
            }
        ));
        assert!(error.is_transient());
        assert!(!error.requires_user_action());
    }

    #[tokio::test]
    async fn fetch_maps_rate_limit_without_retry_after() {
        let temp = tempdir().unwrap();
        write_test_account(temp.path());
        let (endpoint, _handle) = server(429, "{}".to_string(), &[]).await;

        let error = fetch_at(&reqwest::Client::new(), temp.path(), &endpoint)
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            CopilotError::RateLimited {
                retry_after_secs: None
            }
        ));
        assert!(error.is_transient());
        assert!(!error.requires_user_action());
    }

    #[tokio::test]
    async fn fetch_maps_server_error_to_transient_usage_http() {
        let temp = tempdir().unwrap();
        write_test_account(temp.path());
        let (endpoint, _handle) = server(500, "{}".to_string(), &[]).await;

        let error = fetch_at(&reqwest::Client::new(), temp.path(), &endpoint)
            .await
            .unwrap_err();

        assert!(matches!(error, CopilotError::UsageHttp { status: 500 }));
        assert!(error.is_transient());
        assert!(!error.requires_user_action());
    }

    #[tokio::test]
    async fn fetch_maps_network_error_to_transient_usage_request() {
        let temp = tempdir().unwrap();
        write_test_account(temp.path());

        let error = fetch_at(
            &reqwest::Client::new(),
            temp.path(),
            "http://127.0.0.1:9/copilot_internal/user",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, CopilotError::UsageRequest(_)));
        assert!(error.is_transient());
        assert!(!error.requires_user_action());
    }

    #[tokio::test]
    async fn fetch_maps_timeout_to_transient_usage_request() {
        let temp = tempdir().unwrap();
        write_test_account(temp.path());
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let endpoint = format!(
            "http://{}/copilot_internal/user",
            listener.local_addr().unwrap()
        );
        let handle = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            sleep(Duration::from_millis(200)).await;
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(20))
            .build()
            .unwrap();

        let error = fetch_at(&client, temp.path(), &endpoint).await.unwrap_err();

        assert!(matches!(error, CopilotError::UsageRequest(_)));
        assert!(error.is_transient());
        assert!(!error.requires_user_action());
        handle.abort();
    }

    #[test]
    fn sync_managed_accounts_removes_duplicate_github_user_ids() {
        let now = Utc::now();
        let mut config = Config {
            copilot_managed_accounts: vec![
                ManagedCopilotAccountConfig {
                    id: "copilot-1".to_string(),
                    label: "a".to_string(),
                    github_user_id: 1,
                    login: "a".to_string(),
                    created_at: now,
                    updated_at: now,
                    last_authenticated_at: None,
                },
                ManagedCopilotAccountConfig {
                    id: "copilot-dup".to_string(),
                    label: "b".to_string(),
                    github_user_id: 1,
                    login: "b".to_string(),
                    created_at: now,
                    updated_at: now,
                    last_authenticated_at: None,
                },
            ],
            ..Config::default()
        };
        let changed = sync_managed_accounts(&mut config);
        assert!(changed);
        assert_eq!(config.copilot_managed_accounts.len(), 1);
        assert_eq!(config.copilot_managed_accounts[0].id, "copilot-1");
    }
}
