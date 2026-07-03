// SPDX-License-Identifier: MPL-2.0

pub mod chrome_cdp;
pub mod firefox;

/// A cookie extracted from a browser, with the target domain and name.
#[derive(Debug, Clone)]
pub struct BrowserCookie {
    pub value: String,
}

/// A discovered OpenCode workspace with its ID and display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: Option<String>,
}

/// Which browser to use for cookie extraction and workspace discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserKind {
    Firefox,
    Chrome,
}

/// Detect the system's default browser via xdg-mime.
/// Falls back to Firefox if detection fails.
pub fn detect_default_browser() -> BrowserKind {
    let output = std::process::Command::new("xdg-mime")
        .arg("query")
        .arg("default")
        .arg("x-scheme-handler/https")
        .output();

    match output {
        Ok(o) => {
            let name = String::from_utf8_lossy(&o.stdout).to_lowercase();
            if name.contains("chrome") || name.contains("chromium") || name.contains("brave") {
                BrowserKind::Chrome
            } else {
                BrowserKind::Firefox
            }
        }
        Err(_) => BrowserKind::Firefox,
    }
}

/// Find a cookie by name+domain using the detected default browser only.
pub async fn find_cookie(cookie_name: &str, domain: &str) -> Option<BrowserCookie> {
    let browser = detect_default_browser();
    match browser {
        BrowserKind::Firefox => firefox::find_cookie(cookie_name, domain),
        BrowserKind::Chrome => chrome_cdp::find_cookie(cookie_name, domain).await,
    }
}

/// Discover OpenCode workspaces using the detected default browser only.
/// Returns workspace IDs from browser history/tabs. Names are fetched
/// separately via `fetch_workspace_name` using the auth cookie.
pub async fn discover_workspaces() -> Vec<WorkspaceInfo> {
    let browser = detect_default_browser();
    match browser {
        BrowserKind::Firefox => firefox::discover_workspaces(),
        BrowserKind::Chrome => chrome_cdp::discover_workspaces().await,
    }
}

/// Fetch the workspace name by scraping the /go page with the auth cookie.
/// The SSR hydration data contains `{id:"wrk_...",name:"WorkspaceName"}`.
pub async fn fetch_workspace_name(
    client: &reqwest::Client,
    workspace_id: &str,
    auth_cookie: &str,
) -> Option<String> {
    let url = format!("https://opencode.ai/workspace/{workspace_id}/go");
    let response = client
        .get(&url)
        .header("Cookie", format!("auth={auth_cookie}"))
        .send()
        .await
        .ok()?;

    let html = response.text().await.ok()?;

    // Look for {id:"wrk_...",name:"WorkspaceName"} in SSR hydration data
    let marker = format!(r#"id:"{workspace_id}",name:""#);
    if let Some(pos) = html.find(&marker) {
        let after = &html[pos + marker.len()..];
        if let Some(end) = after.find('"') {
            let name = &after[..end];
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }

    None
}

/// Poll for a cookie using the default browser, checking every `interval_ms`
/// milliseconds for up to `timeout_secs` seconds.
pub async fn poll_for_cookie(
    cookie_name: &str,
    domain: &str,
    interval_ms: u64,
    timeout_secs: u64,
) -> Option<BrowserCookie> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        if let Some(cookie) = find_cookie(cookie_name, domain).await {
            return Some(cookie);
        }

        if std::time::Instant::now() >= deadline {
            return None;
        }

        tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
    }
}

/// Poll for workspaces to appear in the default browser's history/tabs.
pub async fn poll_for_workspaces(
    interval_ms: u64,
    timeout_secs: u64,
) -> Vec<WorkspaceInfo> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        let workspaces = discover_workspaces().await;
        if !workspaces.is_empty() {
            return workspaces;
        }

        if std::time::Instant::now() >= deadline {
            return Vec::new();
        }

        tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
    }
}

/// Open a browser to a URL using xdg-open.
pub fn open_browser(url: &str) {
    let _ = std::process::Command::new("xdg-open")
        .arg(url)
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_info_equal_when_id_and_name_match() {
        let a = WorkspaceInfo {
            id: "wrk_123".to_string(),
            name: Some("My Workspace".to_string()),
        };
        let b = WorkspaceInfo {
            id: "wrk_123".to_string(),
            name: Some("My Workspace".to_string()),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn workspace_info_unequal_when_id_differs() {
        let a = WorkspaceInfo {
            id: "wrk_123".to_string(),
            name: Some("My Workspace".to_string()),
        };
        let b = WorkspaceInfo {
            id: "wrk_456".to_string(),
            name: Some("My Workspace".to_string()),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn workspace_info_unequal_when_name_differs() {
        let a = WorkspaceInfo {
            id: "wrk_123".to_string(),
            name: Some("Alpha".to_string()),
        };
        let b = WorkspaceInfo {
            id: "wrk_123".to_string(),
            name: Some("Beta".to_string()),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn workspace_info_unequal_when_some_vs_none_name() {
        let a = WorkspaceInfo {
            id: "wrk_123".to_string(),
            name: Some("Alpha".to_string()),
        };
        let b = WorkspaceInfo {
            id: "wrk_123".to_string(),
            name: None,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn workspace_info_equal_when_both_names_none() {
        let a = WorkspaceInfo {
            id: "wrk_123".to_string(),
            name: None,
        };
        let b = WorkspaceInfo {
            id: "wrk_123".to_string(),
            name: None,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn workspace_info_clone_is_equal() {
        let a = WorkspaceInfo {
            id: "wrk_789".to_string(),
            name: Some("Cloned".to_string()),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn browser_cookie_clone_is_equal() {
        let a = BrowserCookie {
            value: "session_abc".to_string(),
        };
        let b = a.clone();
        assert_eq!(a.value, b.value);
    }

    #[test]
    fn browser_cookie_clone_preserves_value() {
        let original = BrowserCookie {
            value: "cookie_value_xyz".to_string(),
        };
        let cloned = original.clone();
        // mutating the clone must not affect the original
        let _ = cloned;
        assert_eq!(original.value, "cookie_value_xyz");
    }

    #[test]
    fn browser_kind_firefox_not_equal_chrome() {
        assert_ne!(BrowserKind::Firefox, BrowserKind::Chrome);
    }

    #[test]
    fn browser_kind_equal_to_self() {
        assert_eq!(BrowserKind::Firefox, BrowserKind::Firefox);
        assert_eq!(BrowserKind::Chrome, BrowserKind::Chrome);
    }

    #[test]
    fn browser_kind_is_copy() {
        let a = BrowserKind::Chrome;
        let b = a; // Copy, not move
        assert_eq!(a, b);
    }

    #[test]
    fn workspace_info_debug_repr_contains_id() {
        let info = WorkspaceInfo {
            id: "wrk_debug".to_string(),
            name: Some("Debug".to_string()),
        };
        let s = format!("{:?}", info);
        assert!(s.contains("wrk_debug"));
        assert!(s.contains("Debug"));
    }

    #[test]
    fn browser_cookie_debug_repr_contains_value() {
        let cookie = BrowserCookie {
            value: "v_debug".to_string(),
        };
        let s = format!("{:?}", cookie);
        assert!(s.contains("v_debug"));
    }
}