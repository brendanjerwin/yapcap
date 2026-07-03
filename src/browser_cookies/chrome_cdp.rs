// SPDX-License-Identifier: MPL-2.0

use super::BrowserCookie;
use serde::Deserialize;
use std::time::Duration;

const CDP_DEFAULT_PORT: u16 = 9222;
const CDP_QUERY_TIMEOUT: Duration = Duration::from_secs(5);

/// CDP response from /json/version
#[derive(Debug, Deserialize)]
struct CdpVersion {
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: String,
}

/// CDP response from /json/list
#[derive(Debug, Deserialize)]
struct CdpPage {
    id: String,
    url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: String,
}

/// CDP Network.Cookie (subset of fields we need)
#[derive(Debug, Deserialize)]
struct CdpCookie {
    name: String,
    value: String,
    domain: String,
}

/// CDP response from Network.getAllCookies
#[derive(Debug, Deserialize)]
struct CdpGetAllCookiesResult {
    #[serde(rename = "cookies")]
    cookies: Vec<CdpCookie>,
}

/// CDP response wrapper
#[derive(Debug, Deserialize)]
struct CdpResponse {
    id: u64,
    result: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

/// Check if a Chromium browser is running with --remote-debugging-port
fn cdp_is_available(port: u16) -> bool {
    std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
}

/// Get the browser-level WebSocket URL from CDP /json/version
async fn get_browser_ws_url(port: u16) -> Option<String> {
    let url = format!("http://127.0.0.1:{port}/json/version");
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(CDP_QUERY_TIMEOUT)
        .send()
        .await
        .ok()?;
    let version: CdpVersion = resp.json().await.ok()?;
    Some(version.web_socket_debugger_url)
}

/// Find a cookie by name and domain via Chrome DevTools Protocol.
///
/// Connects to a Chromium browser running with --remote-debugging-port,
/// calls Network.getAllCookies, and filters for the target cookie.
///
/// Returns None if no browser with CDP is running, or if the cookie
/// isn't found.
pub async fn find_cookie(cookie_name: &str, domain: &str) -> Option<BrowserCookie> {
    if !cdp_is_available(CDP_DEFAULT_PORT) {
        return None;
    }

    let browser_ws_url = get_browser_ws_url(CDP_DEFAULT_PORT).await?;

    // Connect to the browser-level WebSocket and call Network.getAllCookies
    let (ws_stream, _) = tokio_tungstenite::connect_async(&browser_ws_url)
        .await
        .ok()?;

    use futures_util::StreamExt;
    use futures_util::SinkExt;

    let (mut write, mut read) = ws_stream.split();

    // Send Network.getAllCookies command
    let command = serde_json::json!({
        "id": 1,
        "method": "Network.getAllCookies"
    });
    write
        .send(tokio_tungstenite::tungstenite::Message::text(
            command.to_string(),
        ))
        .await
        .ok()?;

    // Read response
    let msg = tokio::time::timeout(CDP_QUERY_TIMEOUT, read.next())
        .await
        .ok()??
        .ok()?;

    let text = msg.into_text().ok()?.to_string();
    let resp: CdpResponse = serde_json::from_str(&text).ok()?;

    let result = resp.result?;
    let cookie_result: CdpGetAllCookiesResult =
        serde_json::from_value(result).ok()?;

    cookie_result
        .cookies
        .into_iter()
        .find(|c| {
            c.name == cookie_name && (c.domain == domain || c.domain == format!(".{domain}"))
        })
        .map(|c| BrowserCookie {
            value: c.value,
        })
}

/// Launch Chrome/Chromium with --remote-debugging-port so we can read cookies.
///
/// Uses the existing user profile so cookies are available. Returns true if
/// the browser was launched (or was already running with the port).
pub fn launch_chrome_with_debug_port() -> bool {
    // If CDP is already available, nothing to do
    if cdp_is_available(CDP_DEFAULT_PORT) {
        return true;
    }

    // Find Chrome/Chromium binary
    let browser = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]
        .iter()
        .find_map(|name| which::which(name).ok());

    let browser = match browser {
        Some(b) => b,
        None => return false,
    };

    // Launch with the user's default profile
    let user_data_dir = dirs::config_dir().map(|c| c.join("google-chrome/Default"));
    let user_data_dir = match user_data_dir {
        Some(d) => d,
        None => return false,
    };

    let parent = user_data_dir.parent().map(|p| p.to_path_buf());
    let parent = match parent {
        Some(p) => p,
        None => return false,
    };

    std::process::Command::new(browser)
        .arg(format!("--remote-debugging-port={CDP_DEFAULT_PORT}"))
        .arg(format!("--user-data-dir={}", parent.display()))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Discover OpenCode workspaces from Chrome CDP tabs.
///
/// Looks for open tabs with URLs matching /workspace/<wrk_...> and extracts
/// the workspace IDs. Names are fetched separately via fetch_workspace_name.
pub async fn discover_workspaces() -> Vec<super::WorkspaceInfo> {
    if !cdp_is_available(CDP_DEFAULT_PORT) {
        return Vec::new();
    }

    // Get list of open tabs
    let list_url = format!("http://127.0.0.1:{CDP_DEFAULT_PORT}/json/list");
    let resp = match reqwest::Client::new()
        .get(&list_url)
        .timeout(CDP_QUERY_TIMEOUT)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let pages: Vec<CdpPage> = match resp.json().await {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    let mut workspaces = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for page in pages {
        if let Some(id) = extract_workspace_id(&page.url) {
            if seen_ids.insert(id.clone()) {
                workspaces.push(super::WorkspaceInfo { id, name: None });
            }
        }
    }

    workspaces
}

/// Extract a workspace ID from a URL like https://opencode.ai/workspace/wrk_XXXXX/go
fn extract_workspace_id(url: &str) -> Option<String> {
    let marker = "/workspace/";
    let start = url.find(marker)? + marker.len();
    let rest = &url[start..];
    let end = rest.find('/').unwrap_or(rest.len());
    let id = &rest[..end];
    if id.starts_with("wrk_") && id.len() > 4 {
        Some(id.to_string())
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_workspace_id_simple() {
        let url = "https://opencode.ai/workspace/wrk_01KFTT8TJ78XXG19NX1NY1PF5R";
        assert_eq!(
            extract_workspace_id(url),
            Some("wrk_01KFTT8TJ78XXG19NX1NY1PF5R".to_string())
        );
    }

    #[test]
    fn extract_workspace_id_with_go_suffix() {
        let url = "https://opencode.ai/workspace/wrk_01KFTT8TJ78XXG19NX1NY1PF5R/go";
        assert_eq!(
            extract_workspace_id(url),
            Some("wrk_01KFTT8TJ78XXG19NX1NY1PF5R".to_string())
        );
    }

    #[test]
    fn extract_workspace_id_with_usage_suffix() {
        let url = "https://opencode.ai/workspace/wrk_abc/usage";
        assert_eq!(extract_workspace_id(url), Some("wrk_abc".to_string()));
    }

    #[test]
    fn extract_workspace_id_rejects_wrong_prefix() {
        let url = "https://opencode.ai/workspace/notwrk_123";
        assert_eq!(extract_workspace_id(url), None);
    }

    #[test]
    fn extract_workspace_id_no_marker() {
        let url = "https://opencode.ai/no-workspace-here";
        assert_eq!(extract_workspace_id(url), None);
    }

    #[test]
    fn extract_workspace_id_empty_after_marker() {
        let url = "https://opencode.ai/workspace/";
        assert_eq!(extract_workspace_id(url), None);
    }

    #[test]
    fn extract_workspace_id_too_short_after_prefix() {
        let url = "https://opencode.ai/workspace/wrk_";
        assert_eq!(extract_workspace_id(url), None);
    }

    #[test]
    fn extract_workspace_id_empty_url() {
        assert_eq!(extract_workspace_id(""), None);
    }

    #[test]
    fn extract_workspace_id_marker_at_start() {
        let url = "/workspace/wrk_something/extra";
        assert_eq!(extract_workspace_id(url), Some("wrk_something".to_string()));
    }

    #[test]
    fn extract_workspace_id_preserves_full_id() {
        let long = "wrk_01KFTT8TJ78XXG19NX1NY1PF5RABCDEFGHIJ";
        let url = format!("https://opencode.ai/workspace/{}", long);
        assert_eq!(extract_workspace_id(&url), Some(long.to_string()));
    }

    #[test]
    fn extract_workspace_id_ignores_query_string_as_part_of_id() {
        let url = "https://opencode.ai/workspace/wrk_abc?x=1";
        assert_eq!(extract_workspace_id(url), Some("wrk_abc?x=1".to_string()));
    }
}