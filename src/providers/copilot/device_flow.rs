// SPDX-License-Identifier: MPL-2.0

use super::headers::{
    DEVICE_CODE_URL, GITHUB_USER_URL, OAUTH_CLIENT_ID, OAUTH_SCOPE, OAUTH_TOKEN_URL,
    apply_copilot_headers,
};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
struct RawDeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotIdentity {
    pub id: u64,
    pub login: String,
}

#[derive(Debug, Deserialize)]
struct RawIdentity {
    id: u64,
    login: String,
}

#[derive(Debug, Deserialize)]
struct RawTokenSuccess {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct RawTokenError {
    error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    Token(String),
    Pending,
    SlowDown,
    Expired,
    AccessDenied,
    Other(String),
}

pub async fn request_device_code(
    client: &reqwest::Client,
    endpoint: &str,
) -> Result<DeviceCode, String> {
    let response = client
        .post(endpoint)
        .header("Accept", "application/json")
        .form(&[("client_id", OAUTH_CLIENT_ID), ("scope", OAUTH_SCOPE)])
        .send()
        .await
        .map_err(|error| format!("Copilot device code request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Copilot device code response read failed: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "Copilot device code request returned HTTP {status}: {body}"
        ));
    }
    parse_device_code(&body)
}

pub fn parse_device_code(body: &str) -> Result<DeviceCode, String> {
    let raw: RawDeviceCode = serde_json::from_str(body)
        .map_err(|error| format!("Copilot device code parse failed: {error}"))?;
    Ok(DeviceCode {
        device_code: raw.device_code,
        user_code: raw.user_code,
        verification_uri: raw.verification_uri,
        expires_in: raw.expires_in.unwrap_or(900),
        interval: raw.interval.unwrap_or(5),
    })
}

pub async fn poll_token(
    client: &reqwest::Client,
    endpoint: &str,
    device_code: &str,
) -> Result<PollOutcome, String> {
    let response = client
        .post(endpoint)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", OAUTH_CLIENT_ID),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
        .map_err(|error| format!("Copilot token poll request failed: {error}"))?;
    let body = response
        .text()
        .await
        .map_err(|error| format!("Copilot token poll response read failed: {error}"))?;
    Ok(parse_poll_outcome(&body))
}

pub fn parse_poll_outcome(body: &str) -> PollOutcome {
    if let Ok(success) = serde_json::from_str::<RawTokenSuccess>(body)
        && !success.access_token.is_empty()
    {
        return PollOutcome::Token(success.access_token);
    }
    match serde_json::from_str::<RawTokenError>(body) {
        Ok(err) => match err.error.as_str() {
            "authorization_pending" => PollOutcome::Pending,
            "slow_down" => PollOutcome::SlowDown,
            "expired_token" => PollOutcome::Expired,
            "access_denied" => PollOutcome::AccessDenied,
            other => PollOutcome::Other(other.to_string()),
        },
        Err(_) => PollOutcome::Other(body.to_string()),
    }
}

pub async fn fetch_identity(
    client: &reqwest::Client,
    endpoint: &str,
    access_token: &str,
) -> Result<CopilotIdentity, String> {
    let response = apply_copilot_headers(client.get(endpoint), access_token)
        .send()
        .await
        .map_err(|error| format!("Copilot identity request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Copilot identity read failed: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "Copilot identity request returned HTTP {status}: {body}"
        ));
    }
    parse_identity(&body)
}

pub fn parse_identity(body: &str) -> Result<CopilotIdentity, String> {
    let raw: RawIdentity = serde_json::from_str(body)
        .map_err(|error| format!("Copilot identity parse failed: {error}"))?;
    if raw.login.is_empty() {
        return Err("Copilot identity response had empty login".to_string());
    }
    Ok(CopilotIdentity {
        id: raw.id,
        login: raw.login,
    })
}

pub const DEFAULT_DEVICE_CODE_URL: &str = DEVICE_CODE_URL;
pub const DEFAULT_TOKEN_URL: &str = OAUTH_TOKEN_URL;
pub const DEFAULT_IDENTITY_URL: &str = GITHUB_USER_URL;

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn server(body: String) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0u8; 8192];
            let n = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..n]).to_string();
            let raw = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body,
            );
            stream.write_all(raw.as_bytes()).await.unwrap();
            request
        });
        (format!("http://{addr}/user"), handle)
    }

    #[test]
    fn parses_device_code_response() {
        let raw = include_str!("../../../fixtures/copilot/device_code_response.json");
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        let body = value.get("body_text").unwrap().as_str().unwrap();
        let parsed = parse_device_code(body).unwrap();
        assert_eq!(parsed.user_code, "XXXX-XXXX");
        assert_eq!(parsed.verification_uri, "https://github.com/login/device");
        assert_eq!(parsed.expires_in, 899);
        assert_eq!(parsed.interval, 5);
        assert!(!parsed.device_code.is_empty());
    }

    #[test]
    fn defaults_interval_and_expiry_when_missing() {
        let body = r#"{"device_code":"d","user_code":"u","verification_uri":"v"}"#;
        let parsed = parse_device_code(body).unwrap();
        assert_eq!(parsed.interval, 5);
        assert_eq!(parsed.expires_in, 900);
    }

    #[test]
    fn parses_token_success_response() {
        let raw = include_str!("../../../fixtures/copilot/oauth_token_response.json");
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        let body = value.get("body_text").unwrap().as_str().unwrap();
        match parse_poll_outcome(body) {
            PollOutcome::Token(token) => assert!(token.starts_with("ghu_")),
            other => panic!("expected token, got {other:?}"),
        }
    }

    #[test]
    fn parses_pending_error() {
        let outcome = parse_poll_outcome(r#"{"error":"authorization_pending"}"#);
        assert_eq!(outcome, PollOutcome::Pending);
    }

    #[test]
    fn parses_slow_down_error() {
        let outcome = parse_poll_outcome(r#"{"error":"slow_down"}"#);
        assert_eq!(outcome, PollOutcome::SlowDown);
    }

    #[test]
    fn parses_access_denied() {
        let outcome = parse_poll_outcome(r#"{"error":"access_denied"}"#);
        assert_eq!(outcome, PollOutcome::AccessDenied);
    }

    #[test]
    fn parses_expired_token() {
        let outcome = parse_poll_outcome(r#"{"error":"expired_token"}"#);
        assert_eq!(outcome, PollOutcome::Expired);
    }

    #[test]
    fn parses_identity_response() {
        let raw = include_str!("../../../fixtures/copilot/github_user_response.json");
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        let body = value.get("body_text").unwrap().as_str().unwrap();
        let identity = parse_identity(body).unwrap();
        assert_eq!(identity.id, 1);
        assert_eq!(identity.login, "exampleuser");
    }

    #[test]
    fn rejects_identity_with_empty_login() {
        let body = r#"{"id":42,"login":""}"#;
        assert!(parse_identity(body).is_err());
    }

    #[tokio::test]
    async fn fetch_identity_uses_supported_github_api_version() {
        let (endpoint, handle) = server(r#"{"id":42,"login":"octocat"}"#.to_string()).await;

        let identity = fetch_identity(&reqwest::Client::new(), &endpoint, "ghu_test")
            .await
            .unwrap();

        assert_eq!(identity.id, 42);
        let request = handle.await.unwrap();
        assert!(request.contains("authorization: token ghu_test\r\n"));
        assert!(request.contains("x-github-api-version: 2026-03-10\r\n"));
    }
}
