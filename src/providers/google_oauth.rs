// SPDX-License-Identifier: MPL-2.0

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::Read as _;

pub const AUTHORIZE_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

pub const REFRESH_BEFORE_EXPIRY: Duration = Duration::minutes(5);

#[derive(Debug, Clone)]
pub struct GoogleOAuthConfig {
    pub client_id: Cow<'static, str>,
    pub client_secret: Cow<'static, str>,
    pub scope: Cow<'static, str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkceCodes {
    pub code_verifier: String,
    pub code_challenge: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleOAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub expires_at: DateTime<Utc>,
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleRefreshedTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub scope: Vec<String>,
}

#[derive(Debug)]
pub enum GoogleRefreshError {
    Request(reqwest::Error),
    RateLimited { retry_after_secs: Option<u64> },
    Http { status: u16 },
    Decode(reqwest::Error),
    Parse(String),
}

#[derive(Debug, Deserialize)]
struct RawTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

pub fn parse_token_response(raw: &str) -> Result<GoogleOAuthTokens, String> {
    let parsed: RawTokenResponse = serde_json::from_str(raw)
        .map_err(|error| format!("failed to decode Google OAuth token response: {error}"))?;
    let access_token = parsed.access_token;
    let refresh_token = parsed
        .refresh_token
        .ok_or_else(|| "Google OAuth response missing refresh_token".to_string())?;
    let id_token = parsed
        .id_token
        .ok_or_else(|| "Google OAuth response missing id_token".to_string())?;
    let now = Utc::now();
    let expires_at = parsed
        .expires_in
        .map(|seconds| now + Duration::seconds(seconds))
        .unwrap_or(now + Duration::hours(1));
    Ok(GoogleOAuthTokens {
        access_token,
        refresh_token,
        id_token,
        expires_at,
        scope: split_scope(parsed.scope.as_deref()),
    })
}

pub fn authorization_url_with_hint(
    config: &GoogleOAuthConfig,
    redirect_uri: &str,
    pkce: &PkceCodes,
    state: &str,
    login_hint: Option<&str>,
) -> String {
    let mut params: Vec<(&str, &str)> = vec![
        ("response_type", "code"),
        ("client_id", config.client_id.as_ref()),
        ("redirect_uri", redirect_uri),
        ("scope", config.scope.as_ref()),
        ("code_challenge", pkce.code_challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("access_type", "offline"),
        ("prompt", "consent"),
    ];
    if let Some(hint) = login_hint.filter(|hint| !hint.is_empty()) {
        params.push(("login_hint", hint));
    }
    let query = params
        .into_iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{AUTHORIZE_ENDPOINT}?{query}")
}

pub async fn exchange_code(
    config: &GoogleOAuthConfig,
    client: &reqwest::Client,
    token_endpoint: &str,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
) -> Result<GoogleOAuthTokens, String> {
    let response = client
        .post(token_endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", code_verifier),
            ("client_id", config.client_id.as_ref()),
            ("client_secret", config.client_secret.as_ref()),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|error| format!("Google OAuth token exchange failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("failed to read Google OAuth token response: {error}"))?;
    if !status.is_success() {
        let snippet = body.trim().chars().take(256).collect::<String>();
        return Err(format!(
            "Google OAuth token exchange returned {status} (body: {snippet})"
        ));
    }
    parse_token_response(&body)
}

#[must_use]
pub fn needs_refresh(expires_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    expires_at <= now + REFRESH_BEFORE_EXPIRY
}

pub async fn refresh_access_token_at(
    config: &GoogleOAuthConfig,
    client: &reqwest::Client,
    endpoint: &str,
    refresh_token: &str,
    now: DateTime<Utc>,
) -> Result<GoogleRefreshedTokens, GoogleRefreshError> {
    let response = client
        .post(endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", config.client_id.as_ref()),
            ("client_secret", config.client_secret.as_ref()),
        ])
        .send()
        .await
        .map_err(GoogleRefreshError::Request)?;
    let status = response.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(GoogleRefreshError::RateLimited {
            retry_after_secs: retry_after_secs(response.headers()),
        });
    }
    if !status.is_success() {
        return Err(GoogleRefreshError::Http {
            status: status.as_u16(),
        });
    }
    let body = response.text().await.map_err(GoogleRefreshError::Decode)?;
    parse_refresh_response(&body, refresh_token, now).map_err(GoogleRefreshError::Parse)
}

pub fn parse_refresh_response(
    raw: &str,
    original_refresh_token: &str,
    now: DateTime<Utc>,
) -> Result<GoogleRefreshedTokens, String> {
    #[derive(Debug, Deserialize)]
    struct Raw {
        access_token: Option<String>,
        #[serde(default)]
        refresh_token: Option<String>,
        #[serde(default)]
        expires_in: Option<i64>,
        #[serde(default)]
        scope: Option<String>,
    }
    let parsed: Raw = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    let access_token = parsed
        .access_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "missing access_token".to_string())?;
    let expires_in = parsed.expires_in.unwrap_or(3600);
    if expires_in <= 0 {
        return Err("invalid expires_in".to_string());
    }
    let refresh_token = parsed
        .refresh_token
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| original_refresh_token.to_string());
    Ok(GoogleRefreshedTokens {
        access_token,
        refresh_token,
        expires_at: now + Duration::seconds(expires_in),
        scope: split_scope(parsed.scope.as_deref()),
    })
}

fn retry_after_secs(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn split_scope(scope: Option<&str>) -> Vec<String> {
    scope
        .map(|scope| {
            scope
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub fn new_pkce() -> PkceCodes {
    let bytes = random_bytes();
    let code_verifier = URL_SAFE_NO_PAD.encode(bytes);
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    PkceCodes {
        code_verifier,
        code_challenge,
    }
}

pub fn new_state() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes())
}

fn random_bytes() -> [u8; 64] {
    let mut bytes = [0; 64];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom")
        && file.read_exact(&mut bytes).is_ok()
    {
        return bytes;
    }
    let fallback = format!(
        "{}:{}:{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id(),
        std::thread::current().name().unwrap_or("thread")
    );
    let digest = Sha256::digest(fallback.as_bytes());
    bytes[..32].copy_from_slice(&digest);
    let second = Sha256::digest(&bytes[..32]);
    bytes[32..].copy_from_slice(&second);
    bytes
}

pub fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            write!(out, "%{byte:02X}").expect("writing to a string cannot fail");
        }
    }
    out
}

pub fn percent_decode(value: &str) -> String {
    let mut out = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(hex) = u8::from_str_radix(&value[index + 1..index + 3], 16)
        {
            out.push(hex);
            index += 3;
        } else if bytes[index] == b'+' {
            out.push(b' ');
            index += 1;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 14, 12, 0, 0).unwrap()
    }

    #[test]
    fn pkce_challenge_is_sha256_of_verifier() {
        let pkce = new_pkce();
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce.code_verifier.as_bytes()));
        assert_eq!(pkce.code_challenge, expected);
        assert!(pkce.code_verifier.len() >= 43);
    }

    #[test]
    fn state_is_random() {
        assert_ne!(new_state(), new_state());
    }

    #[test]
    fn refresh_parse_preserves_refresh_token_when_absent() {
        let raw = r#"{"access_token":"a","expires_in":3599,"scope":"openid"}"#;
        let parsed = parse_refresh_response(raw, "original", fixed_now()).unwrap();
        assert_eq!(parsed.refresh_token, "original");
        assert_eq!(parsed.expires_at, fixed_now() + Duration::seconds(3599));
    }

    #[test]
    fn refresh_parse_rejects_invalid_expires_in() {
        assert!(
            parse_refresh_response(r#"{"access_token":"a","expires_in":0}"#, "r", fixed_now())
                .is_err()
        );
    }
}
