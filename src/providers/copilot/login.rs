// SPDX-License-Identifier: MPL-2.0

use super::account::{ResolvedAccount, resolve_account_target};
use super::device_flow::{
    DEFAULT_DEVICE_CODE_URL, DEFAULT_IDENTITY_URL, DEFAULT_TOKEN_URL, DeviceCode, PollOutcome,
    fetch_identity, poll_token, request_device_code,
};
use super::storage::{CopilotMetadata, CopilotTokens, write_account};
use crate::config::{Config, ManagedCopilotAccountConfig};
use chrono::Utc;
use cosmic::iced::Task;
use cosmic::iced::futures::SinkExt;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct CopilotLoginState {
    pub flow_id: String,
    pub status: CopilotLoginStatus,
    pub user_code: Option<String>,
    pub verification_uri: Option<String>,
    pub output: Vec<String>,
    pub error: Option<String>,
    pub code_copied: bool,
    #[allow(dead_code)]
    pub expected_github_user_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopilotLoginStatus {
    Running,
    Failed,
}

#[derive(Debug, Clone)]
pub enum CopilotLoginEvent {
    Code {
        flow_id: String,
        user_code: String,
        verification_uri: String,
    },
    Output {
        flow_id: String,
        line: String,
    },
    Finished {
        flow_id: String,
        result: Box<Result<CopilotLoginSuccess, String>>,
    },
}

#[derive(Debug, Clone)]
pub struct CopilotLoginSuccess {
    pub account: ManagedCopilotAccountConfig,
}

#[derive(Debug, Clone, Default)]
struct ReauthOptions {
    expected_github_user_id: Option<u64>,
}

pub fn prepare(config: Config) -> Result<(CopilotLoginState, Task<CopilotLoginEvent>), String> {
    prepare_with_options(config, ReauthOptions::default())
}

pub fn prepare_for_reauth(
    config: Config,
    account_id: &str,
) -> Result<(CopilotLoginState, Task<CopilotLoginEvent>), String> {
    let expected = config
        .copilot_managed_accounts
        .iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| format!("Copilot account {account_id} no longer exists"))?
        .github_user_id;
    prepare_with_options(
        config,
        ReauthOptions {
            expected_github_user_id: Some(expected),
        },
    )
}

fn prepare_with_options(
    config: Config,
    options: ReauthOptions,
) -> Result<(CopilotLoginState, Task<CopilotLoginEvent>), String> {
    let account_root = crate::config::paths().copilot_accounts_dir;
    crate::providers::copilot::storage::create_private_dir(&account_root)?;
    let flow_id = new_flow_id();

    let state = CopilotLoginState {
        flow_id: flow_id.clone(),
        status: CopilotLoginStatus::Running,
        user_code: None,
        verification_uri: None,
        output: Vec::new(),
        error: None,
        code_copied: false,
        expected_github_user_id: options.expected_github_user_id,
    };
    let stream = cosmic::iced::stream::channel(100, move |mut output| async move {
        run_login(flow_id, config, options, &mut output).await;
    });
    Ok((state, Task::stream(stream)))
}

async fn run_login(
    flow_id: String,
    config: Config,
    options: ReauthOptions,
    output: &mut cosmic::iced::futures::channel::mpsc::Sender<CopilotLoginEvent>,
) {
    let endpoints = Endpoints {
        device_code: DEFAULT_DEVICE_CODE_URL,
        token: DEFAULT_TOKEN_URL,
        identity: DEFAULT_IDENTITY_URL,
    };
    let result = run_login_inner(&flow_id, &config, &options, output, &endpoints).await;
    let _ = output
        .send(CopilotLoginEvent::Finished {
            flow_id,
            result: Box::new(result),
        })
        .await;
}

struct Endpoints<'a> {
    device_code: &'a str,
    token: &'a str,
    identity: &'a str,
}

async fn run_login_inner(
    flow_id: &str,
    config: &Config,
    options: &ReauthOptions,
    output: &mut cosmic::iced::futures::channel::mpsc::Sender<CopilotLoginEvent>,
    endpoints: &Endpoints<'_>,
) -> Result<CopilotLoginSuccess, String> {
    let client = crate::runtime::http_client();
    let device_code = request_device_code(&client, endpoints.device_code).await?;
    let _ = output
        .send(CopilotLoginEvent::Code {
            flow_id: flow_id.to_string(),
            user_code: device_code.user_code.clone(),
            verification_uri: device_code.verification_uri.clone(),
        })
        .await;
    open_browser(&device_code.verification_uri);
    let access_token =
        poll_for_token(&client, endpoints.token, &device_code, output, flow_id).await?;
    let identity = fetch_identity(&client, endpoints.identity, &access_token).await?;
    verify_reauth_github_user_id(options.expected_github_user_id, identity.id)?;
    let now = Utc::now();
    let ResolvedAccount {
        id,
        account_dir,
        created_at,
    } = resolve_account_target(config, identity.id, now);
    let tokens = CopilotTokens { access_token };
    let metadata = CopilotMetadata {
        github_user_id: identity.id,
        login: identity.login.clone(),
        created_at,
        updated_at: now,
        last_authenticated_at: Some(now),
    };
    write_account(&account_dir, &tokens, &metadata)?;
    let account = ManagedCopilotAccountConfig {
        id,
        label: identity.login.clone(),
        github_user_id: identity.id,
        login: identity.login,
        created_at,
        updated_at: now,
        last_authenticated_at: Some(now),
    };
    Ok(CopilotLoginSuccess { account })
}

async fn poll_for_token(
    client: &reqwest::Client,
    endpoint: &str,
    device_code: &DeviceCode,
    output: &mut cosmic::iced::futures::channel::mpsc::Sender<CopilotLoginEvent>,
    flow_id: &str,
) -> Result<String, String> {
    let mut interval = device_code.interval.max(1);
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        match poll_token(client, endpoint, &device_code.device_code).await? {
            PollOutcome::Token(token) => return Ok(token),
            PollOutcome::Pending => continue,
            PollOutcome::SlowDown => {
                interval += 5;
                let _ = output
                    .send(CopilotLoginEvent::Output {
                        flow_id: flow_id.to_string(),
                        line: "GitHub asked us to slow down polling.".to_string(),
                    })
                    .await;
            }
            PollOutcome::Expired => {
                return Err(
                    "Device code expired before sign-in completed. Please try again.".to_string(),
                );
            }
            PollOutcome::AccessDenied => {
                return Err("Sign-in was cancelled in the browser.".to_string());
            }
            PollOutcome::Other(other) => {
                return Err(format!("Copilot sign-in failed: {other}"));
            }
        }
    }
}

fn new_flow_id() -> String {
    let millis = chrono::Utc::now().timestamp_millis();
    format!("copilot-{millis}-{}", std::process::id())
}

fn open_browser(url: &str) {
    if let Err(error) = std::process::Command::new("xdg-open").arg(url).spawn() {
        tracing::warn!(url = %url, error = %error, "failed to open Copilot device URL");
    }
}

fn verify_reauth_github_user_id(expected: Option<u64>, actual: u64) -> Result<(), String> {
    if expected.is_some_and(|expected| expected != actual) {
        Err("This is a different GitHub account. The existing account was not updated.".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn commit_login_for_test(
    config: &Config,
    account_root: std::path::PathBuf,
    identity_id: u64,
    login: String,
    access_token: String,
) -> Result<CopilotLoginSuccess, String> {
    let now = Utc::now();
    let id = crate::providers::copilot::storage::account_id_for_github_user(identity_id);
    let account_dir = account_root.join(&id);
    let existing = crate::providers::copilot::account::find_matching_account(config, identity_id);
    let created_at = existing.map_or(now, |account| account.created_at);
    let tokens = CopilotTokens { access_token };
    let metadata = CopilotMetadata {
        github_user_id: identity_id,
        login: login.clone(),
        created_at,
        updated_at: now,
        last_authenticated_at: Some(now),
    };
    write_account(&account_dir, &tokens, &metadata)?;
    let account = ManagedCopilotAccountConfig {
        id,
        label: login.clone(),
        github_user_id: identity_id,
        login,
        created_at,
        updated_at: now,
        last_authenticated_at: Some(now),
    };
    Ok(CopilotLoginSuccess { account })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn commit_login_writes_files_and_dedupes_by_id() {
        let temp = tempdir().unwrap();
        let config = Config::default();
        let first = commit_login_for_test(
            &config,
            temp.path().to_path_buf(),
            42,
            "octocat".to_string(),
            "ghu_first".to_string(),
        )
        .unwrap();
        assert_eq!(first.account.id, "copilot-42");
        let account_dir = temp.path().join("copilot-42");
        assert!(account_dir.join("tokens.json").exists());
        assert!(account_dir.join("metadata.json").exists());
        let raw = std::fs::read_to_string(account_dir.join("tokens.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 1);
        assert_eq!(value.get("access_token").unwrap(), "ghu_first");

        let config_with_existing = Config {
            copilot_managed_accounts: vec![first.account.clone()],
            ..Config::default()
        };
        let renamed = commit_login_for_test(
            &config_with_existing,
            temp.path().to_path_buf(),
            42,
            "octocat-renamed".to_string(),
            "ghu_second".to_string(),
        )
        .unwrap();
        assert_eq!(renamed.account.login, "octocat-renamed");
        assert_eq!(renamed.account.id, "copilot-42");
        assert_eq!(renamed.account.created_at, first.account.created_at);
    }

    #[test]
    fn reauth_accepts_matching_github_user_id() {
        assert!(verify_reauth_github_user_id(Some(42), 42).is_ok());
    }

    #[test]
    fn reauth_rejects_different_github_user_id() {
        let error = verify_reauth_github_user_id(Some(42), 7).unwrap_err();
        assert_eq!(
            error,
            "This is a different GitHub account. The existing account was not updated."
        );
    }

    #[test]
    fn add_account_allows_any_github_user_id() {
        assert!(verify_reauth_github_user_id(None, 7).is_ok());
    }
}
