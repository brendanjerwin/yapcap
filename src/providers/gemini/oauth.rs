// SPDX-License-Identifier: MPL-2.0

use crate::error::GeminiError;
use crate::providers::google_oauth::{self, GoogleOAuthConfig, GoogleRefreshError};
use chrono::{DateTime, Utc};
use std::borrow::Cow;

pub use google_oauth::{
    GoogleOAuthTokens as GeminiOAuthTokens, GoogleRefreshedTokens as GeminiRefreshedTokens,
    PkceCodes, TOKEN_ENDPOINT, needs_refresh, new_pkce, new_state, percent_decode,
};

pub const OAUTH_CLIENT_ID: &str =
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
pub const OAUTH_CLIENT_SECRET: &str = "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";

pub const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform openid https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/userinfo.email";

pub fn config() -> GoogleOAuthConfig {
    GoogleOAuthConfig {
        client_id: Cow::Borrowed(OAUTH_CLIENT_ID),
        client_secret: Cow::Borrowed(OAUTH_CLIENT_SECRET),
        scope: Cow::Borrowed(SCOPE),
    }
}

pub fn authorization_url_with_hint(
    redirect_uri: &str,
    pkce: &PkceCodes,
    state: &str,
    login_hint: Option<&str>,
) -> String {
    google_oauth::authorization_url_with_hint(&config(), redirect_uri, pkce, state, login_hint)
}

pub async fn exchange_code(
    client: &reqwest::Client,
    token_endpoint: &str,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
) -> Result<GeminiOAuthTokens, String> {
    google_oauth::exchange_code(
        &config(),
        client,
        token_endpoint,
        redirect_uri,
        code_verifier,
        code,
    )
    .await
}

pub async fn refresh_access_token_at(
    client: &reqwest::Client,
    endpoint: &str,
    refresh_token: &str,
    now: DateTime<Utc>,
) -> Result<GeminiRefreshedTokens, GeminiError> {
    google_oauth::refresh_access_token_at(&config(), client, endpoint, refresh_token, now)
        .await
        .map_err(map_refresh_error)
}

fn map_refresh_error(error: GoogleRefreshError) -> GeminiError {
    match error {
        GoogleRefreshError::Request(source) => GeminiError::TokenRefreshRequest(source),
        GoogleRefreshError::RateLimited { retry_after_secs } => {
            GeminiError::RateLimited { retry_after_secs }
        }
        GoogleRefreshError::Http { status } => GeminiError::TokenRefreshHttp { status },
        GoogleRefreshError::Decode(source) => GeminiError::TokenRefreshDecode(source),
        GoogleRefreshError::Parse(detail) => GeminiError::TokenRefreshParse(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::google_oauth::parse_token_response;
    use chrono::{Duration, TimeZone};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct MockResponse {
        status: u16,
        body: String,
        extra_headers: Vec<(&'static str, String)>,
    }

    async fn mock_token_server(
        responses: Vec<MockResponse>,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let mut requests = Vec::new();
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = vec![0; 8192];
                let bytes = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..bytes]).to_string();
                let mut header_lines = String::new();
                for (key, value) in &response.extra_headers {
                    header_lines.push_str(&format!("{key}: {value}\r\n"));
                }
                let raw = format!(
                    "HTTP/1.1 {} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n{header_lines}connection: close\r\n\r\n{}",
                    response.status,
                    response.body.len(),
                    response.body,
                );
                stream.write_all(raw.as_bytes()).await.unwrap();
                requests.push(request);
            }
            requests
        });
        (format!("http://{addr}/token"), handle)
    }

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 14, 12, 0, 0).unwrap()
    }

    #[test]
    fn authorization_url_includes_pkce_and_scopes() {
        let url = authorization_url_with_hint(
            "http://localhost:12345/oauth/callback",
            &PkceCodes {
                code_verifier: "verifier".to_string(),
                code_challenge: "challenge".to_string(),
            },
            "nonce",
            None,
        );
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("client_id=681255809395-"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A12345%2Foauth%2Fcallback"));
        assert!(url.contains("code_challenge=challenge"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=nonce"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcloud-platform"));
        assert!(url.contains("openid"));
        assert!(url.contains("userinfo.email"));
        assert!(!url.contains("verifier"));
        assert!(!url.contains("login_hint"));
    }

    #[test]
    fn authorization_url_appends_login_hint_when_provided() {
        let url = authorization_url_with_hint(
            "http://localhost:1/oauth/callback",
            &PkceCodes {
                code_verifier: "v".to_string(),
                code_challenge: "c".to_string(),
            },
            "n",
            Some("user@example.com"),
        );
        assert!(url.contains("login_hint=user%40example.com"));
    }

    #[test]
    fn parses_authorization_code_token_response() {
        let raw = r#"{
            "access_token": "ya29.access",
            "refresh_token": "1//refresh",
            "id_token": "header.payload.sig",
            "expires_in": 3599,
            "scope": "openid https://www.googleapis.com/auth/userinfo.email",
            "token_type": "Bearer"
        }"#;
        let parsed = parse_token_response(raw).expect("parsed");
        assert_eq!(parsed.access_token, "ya29.access");
        assert_eq!(parsed.refresh_token, "1//refresh");
        assert_eq!(parsed.id_token, "header.payload.sig");
        assert!(parsed.scope.iter().any(|s| s == "openid"));
        assert!(parsed.expires_at > Utc::now());
    }

    #[test]
    fn missing_refresh_token_is_an_error() {
        let raw = r#"{"access_token":"a","id_token":"h.p.s","expires_in":60}"#;
        assert!(parse_token_response(raw).is_err());
    }

    #[test]
    fn missing_id_token_is_an_error() {
        let raw = r#"{"access_token":"a","refresh_token":"r","expires_in":60}"#;
        assert!(parse_token_response(raw).is_err());
    }

    #[test]
    fn needs_refresh_when_expires_at_within_five_minutes() {
        let now = fixed_now();
        assert!(needs_refresh(now + Duration::minutes(2), now));
        assert!(needs_refresh(now + Duration::minutes(5), now));
        assert!(!needs_refresh(now + Duration::minutes(6), now));
    }

    #[tokio::test]
    async fn refresh_success_preserves_refresh_token_across_http_call() {
        let (endpoint, handle) = mock_token_server(vec![MockResponse {
            status: 200,
            body: r#"{"access_token":"ya29.new","expires_in":3599,"scope":"openid"}"#.to_string(),
            extra_headers: Vec::new(),
        }])
        .await;
        let now = fixed_now();
        let tokens =
            refresh_access_token_at(&reqwest::Client::new(), &endpoint, "original-refresh", now)
                .await
                .unwrap();
        assert_eq!(tokens.access_token, "ya29.new");
        assert_eq!(tokens.refresh_token, "original-refresh");
        assert_eq!(tokens.expires_at, now + Duration::seconds(3599));
        let requests = handle.await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("grant_type=refresh_token"));
        assert!(requests[0].contains("refresh_token=original-refresh"));
        assert!(requests[0].contains(&format!("client_id={OAUTH_CLIENT_ID}")));
        assert!(requests[0].contains("client_secret="));
    }

    #[tokio::test]
    async fn refresh_4xx_is_classified_permanent_action_required() {
        for status in [400_u16, 401, 403] {
            let (endpoint, handle) = mock_token_server(vec![MockResponse {
                status,
                body: r#"{"error":"invalid_grant"}"#.to_string(),
                extra_headers: Vec::new(),
            }])
            .await;
            let error =
                refresh_access_token_at(&reqwest::Client::new(), &endpoint, "refresh", fixed_now())
                    .await
                    .unwrap_err();
            assert!(matches!(error, GeminiError::TokenRefreshHttp { status: s } if s == status));
            assert!(error.requires_user_action());
            assert!(!error.is_transient());
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn refresh_429_with_retry_after_parses_seconds() {
        let (endpoint, handle) = mock_token_server(vec![MockResponse {
            status: 429,
            body: "{}".to_string(),
            extra_headers: vec![("Retry-After", "120".to_string())],
        }])
        .await;
        let error =
            refresh_access_token_at(&reqwest::Client::new(), &endpoint, "refresh", fixed_now())
                .await
                .unwrap_err();
        assert!(matches!(
            error,
            GeminiError::RateLimited {
                retry_after_secs: Some(120)
            }
        ));
        assert!(error.is_transient());
        assert!(!error.requires_user_action());
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn refresh_5xx_is_transient_token_refresh_http() {
        let (endpoint, handle) = mock_token_server(vec![MockResponse {
            status: 503,
            body: "{}".to_string(),
            extra_headers: Vec::new(),
        }])
        .await;
        let error =
            refresh_access_token_at(&reqwest::Client::new(), &endpoint, "refresh", fixed_now())
                .await
                .unwrap_err();
        assert!(matches!(
            error,
            GeminiError::TokenRefreshHttp { status: 503 }
        ));
        assert!(error.is_transient());
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn refresh_network_error_is_transient_and_offline() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let endpoint = format!("http://{addr}/token");
        let error = refresh_access_token_at(
            &reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(2))
                .build()
                .unwrap(),
            &endpoint,
            "refresh",
            fixed_now(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, GeminiError::TokenRefreshRequest(_)));
        assert!(error.is_network_unavailable());
        assert!(error.is_transient());
    }
}
