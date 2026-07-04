// SPDX-License-Identifier: MPL-2.0

pub mod chrome_cdp;
pub mod firefox;

use async_trait::async_trait;

/// A cookie extracted from a browser.
#[derive(Debug, Clone)]
pub struct BrowserCookie {
    pub value: String,
}

/// A discovered OpenCode workspace with its ID and optional display name.
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

/// Trait abstracting browser cookie extraction and workspace discovery.
///
/// Implementations include Firefox (SQLite) and Chrome (CDP), plus mock
/// implementations for testing. This allows the `fetch()` functions in
/// provider modules to accept an injectable cookie source rather than
/// binding directly to disk/CDP.
#[async_trait]
pub trait CookieSource: Send + Sync {
    /// Find a cookie by name and domain.
    async fn find_cookie(&self, cookie_name: &str, domain: &str) -> Option<BrowserCookie>;

    /// Discover OpenCode workspaces from browser history/tabs.
    async fn discover_workspaces(&self) -> Vec<WorkspaceInfo>;


}

/// Parse a browser name string to determine the browser kind.
/// "chrome", "chromium", "brave" → Chrome; anything else → Firefox.
pub fn detect_browser_kind_from_string(name: &str) -> BrowserKind {
    let lower = name.to_lowercase();
    if lower.contains("chrome") || lower.contains("chromium") || lower.contains("brave") {
        BrowserKind::Chrome
    } else {
        BrowserKind::Firefox
    }
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
        Ok(o) => detect_browser_kind_from_string(&String::from_utf8_lossy(&o.stdout)),
        Err(_) => BrowserKind::Firefox,
    }
}

/// Construct the default `CookieSource` for the system's detected browser.
pub fn default_cookie_source() -> Box<dyn CookieSource> {
    match detect_default_browser() {
        BrowserKind::Firefox => Box::new(firefox::FirefoxSource),
        BrowserKind::Chrome => Box::new(chrome_cdp::ChromeSource),
    }
}

/// Discover OpenCode workspaces using the detected default browser.
pub async fn discover_workspaces() -> Vec<WorkspaceInfo> {
    default_cookie_source().discover_workspaces().await
}

/// Extract a workspace name from SSR hydration data in HTML.
/// Looks for `{id:"wrk_...",name:"WorkspaceName"}`.
pub fn parse_workspace_name_from_html(html: &str, workspace_id: &str) -> Option<String> {
    let marker = format!(r#"id:"{workspace_id}",name:""#);
    let pos = html.find(&marker)?;
    let after = &html[pos + marker.len()..];
    let end = after.find('"')?;
    let name = &after[..end];
    if name.is_empty() { None } else { Some(name.to_string()) }
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
    parse_workspace_name_from_html(&html, workspace_id)
}

/// Poll for a cookie using the given source, checking every `interval_ms`
/// milliseconds for up to `timeout_secs` seconds.
pub async fn poll_for_cookie_with(
    source: &dyn CookieSource,
    cookie_name: &str,
    domain: &str,
    interval_ms: u64,
    timeout_secs: u64,
) -> Option<BrowserCookie> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        if let Some(cookie) = source.find_cookie(cookie_name, domain).await {
            return Some(cookie);
        }

        if std::time::Instant::now() >= deadline {
            return None;
        }

        tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
    }
}

/// Poll for a cookie using the default browser.
pub async fn poll_for_cookie(
    cookie_name: &str,
    domain: &str,
    interval_ms: u64,
    timeout_secs: u64,
) -> Option<BrowserCookie> {
    let source = default_cookie_source();
    poll_for_cookie_with(source.as_ref(), cookie_name, domain, interval_ms, timeout_secs).await
}

/// Poll for workspaces using the given source.
#[cfg(test)]
pub async fn poll_for_workspaces_with(
    source: &dyn CookieSource,
    interval_ms: u64,
    timeout_secs: u64,
) -> Vec<WorkspaceInfo> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        let workspaces = source.discover_workspaces().await;
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
    fn parse_workspace_name_from_html_extracts_name() {
        let html = r#"<script>{id:"wrk_test",name:"My Workspace"}</script>"#;
        assert_eq!(
            parse_workspace_name_from_html(html, "wrk_test"),
            Some("My Workspace".to_string())
        );
    }

    #[test]
    fn parse_workspace_name_from_html_empty_name_is_none() {
        let html = r#"<script>{id:"wrk_test",name:""}</script>"#;
        assert_eq!(parse_workspace_name_from_html(html, "wrk_test"), None);
    }

    #[test]
    fn parse_workspace_name_from_html_without_marker_is_none() {
        let html = r#"<script>{id:"wrk_other",name:"Not Ours"}</script>"#;
        assert_eq!(parse_workspace_name_from_html(html, "wrk_test"), None);
    }

    #[test]
    fn parse_workspace_name_from_html_extracts_from_larger_context() {
        let html = r#"<script>...$R[35]={id:"wrk_xyz",name:"Default",slug:null}...</script>"#;
        assert_eq!(
            parse_workspace_name_from_html(html, "wrk_xyz"),
            Some("Default".to_string())
        );
    }

    #[test]
    fn detect_browser_kind_google_chrome_is_chrome() {
        assert_eq!(detect_browser_kind_from_string("google-chrome.desktop"), BrowserKind::Chrome);
    }

    #[test]
    fn detect_browser_kind_chromium_browser_is_chrome() {
        assert_eq!(detect_browser_kind_from_string("chromium-browser.desktop"), BrowserKind::Chrome);
    }

    #[test]
    fn detect_browser_kind_brave_browser_is_chrome() {
        assert_eq!(detect_browser_kind_from_string("brave-browser.desktop"), BrowserKind::Chrome);
    }

    #[test]
    fn detect_browser_kind_firefox_is_firefox() {
        assert_eq!(detect_browser_kind_from_string("firefox.desktop"), BrowserKind::Firefox);
    }

    #[test]
    fn detect_browser_kind_userapp_firefox_is_firefox() {
        assert_eq!(detect_browser_kind_from_string("userapp-Firefox-XYZ.desktop"), BrowserKind::Firefox);
    }

    #[test]
    fn detect_browser_kind_empty_string_is_firefox() {
        assert_eq!(detect_browser_kind_from_string(""), BrowserKind::Firefox);
    }

    #[test]
    fn detect_browser_kind_uppercase_chrome_is_chrome() {
        assert_eq!(detect_browser_kind_from_string("Chrome"), BrowserKind::Chrome);
    }

    /// A mock cookie source for testing that returns canned data.
    pub struct MockCookieSource {
        pub cookie: Option<BrowserCookie>,
        pub workspaces: Vec<WorkspaceInfo>,
    }

    #[async_trait]
    impl CookieSource for MockCookieSource {
        async fn find_cookie(&self, _cookie_name: &str, _domain: &str) -> Option<BrowserCookie> {
            self.cookie.clone()
        }

        async fn discover_workspaces(&self) -> Vec<WorkspaceInfo> {
            self.workspaces.clone()
        }

    }

    #[test]
    fn mock_source_returns_canned_cookie() {
        let source = MockCookieSource {
            cookie: Some(BrowserCookie {
                value: "test-cookie-value".to_string(),
            }),
            workspaces: Vec::new(),
        };
        let future = source.find_cookie("auth", "opencode.ai");
        let result = tokio::runtime::Runtime::new().unwrap().block_on(future);
        assert_eq!(result.unwrap().value, "test-cookie-value");
    }

    #[test]
    fn mock_source_returns_canned_workspaces() {
        let source = MockCookieSource {
            cookie: None,
            workspaces: vec![
                WorkspaceInfo {
                    id: "wrk_test1".to_string(),
                    name: Some("Team A".to_string()),
                },
                WorkspaceInfo {
                    id: "wrk_test2".to_string(),
                    name: None,
                },
            ],
        };
        let future = source.discover_workspaces();
        let result = tokio::runtime::Runtime::new().unwrap().block_on(future);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "wrk_test1");
        assert_eq!(result[0].name.as_deref(), Some("Team A"));
    }

    #[test]
    fn mock_source_returns_none_when_no_cookie() {
        let source = MockCookieSource {
            cookie: None,
            workspaces: Vec::new(),
        };
        let future = source.find_cookie("auth", "opencode.ai");
        let result = tokio::runtime::Runtime::new().unwrap().block_on(future);
        assert!(result.is_none());
    }

    #[test]
    fn mock_source_returns_empty_workspaces_when_none() {
        let source = MockCookieSource {
            cookie: None,
            workspaces: Vec::new(),
        };
        let future = source.discover_workspaces();
        let result = tokio::runtime::Runtime::new().unwrap().block_on(future);
        assert!(result.is_empty());
    }

    #[test]
    fn browser_cookie_clone_preserves_value() {
        let cookie = BrowserCookie {
            value: "abc123".to_string(),
        };
        let cloned = cookie.clone();
        assert_eq!(cloned.value, "abc123");
    }

    #[test]
    fn browser_cookie_debug_repr_contains_value() {
        let cookie = BrowserCookie {
            value: "xyz789".to_string(),
        };
        let debug = format!("{cookie:?}");
        assert!(debug.contains("xyz789"));
    }

    #[test]
    fn workspace_info_equal_when_id_and_name_match() {
        let a = WorkspaceInfo {
            id: "wrk_1".to_string(),
            name: Some("Team".to_string()),
        };
        let b = WorkspaceInfo {
            id: "wrk_1".to_string(),
            name: Some("Team".to_string()),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn workspace_info_unequal_when_id_differs() {
        let a = WorkspaceInfo {
            id: "wrk_1".to_string(),
            name: None,
        };
        let b = WorkspaceInfo {
            id: "wrk_2".to_string(),
            name: None,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn workspace_info_unequal_when_name_differs() {
        let a = WorkspaceInfo {
            id: "wrk_1".to_string(),
            name: Some("A".to_string()),
        };
        let b = WorkspaceInfo {
            id: "wrk_1".to_string(),
            name: Some("B".to_string()),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn workspace_info_unequal_when_some_vs_none_name() {
        let a = WorkspaceInfo {
            id: "wrk_1".to_string(),
            name: Some("A".to_string()),
        };
        let b = WorkspaceInfo {
            id: "wrk_1".to_string(),
            name: None,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn workspace_info_equal_when_both_names_none() {
        let a = WorkspaceInfo {
            id: "wrk_1".to_string(),
            name: None,
        };
        let b = WorkspaceInfo {
            id: "wrk_1".to_string(),
            name: None,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn workspace_info_clone_is_equal() {
        let a = WorkspaceInfo {
            id: "wrk_1".to_string(),
            name: Some("Team".to_string()),
        };
        let b = a.clone();
        assert_eq!(a, b);
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
        let a = BrowserKind::Firefox;
        let b = a; // copy
        assert_eq!(a, b);
    }

    #[test]
    fn workspace_info_debug_repr_contains_id() {
        let ws = WorkspaceInfo {
            id: "wrk_abc".to_string(),
            name: None,
        };
        let debug = format!("{ws:?}");
        assert!(debug.contains("wrk_abc"));
    }

    #[test]
    fn poll_for_cookie_with_mock_returns_immediately_when_cookie_present() {
        let source = MockCookieSource {
            cookie: Some(BrowserCookie {
                value: "found".to_string(),
            }),
            workspaces: Vec::new(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(poll_for_cookie_with(
            &source,
            "auth",
            "opencode.ai",
            10,
            5,
        ));
        assert_eq!(result.unwrap().value, "found");
    }

    #[test]
    fn poll_for_cookie_with_mock_returns_none_on_timeout() {
        let source = MockCookieSource {
            cookie: None,
            workspaces: Vec::new(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(poll_for_cookie_with(
            &source,
            "auth",
            "opencode.ai",
            10,
            1,
        ));
        assert!(result.is_none());
    }

    #[test]
    fn poll_for_workspaces_with_mock_returns_immediately_when_present() {
        let source = MockCookieSource {
            cookie: None,
            workspaces: vec![WorkspaceInfo {
                id: "wrk_test".to_string(),
                name: None,
            }],
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(poll_for_workspaces_with(&source, 10, 5));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "wrk_test");
    }

    #[test]
    fn poll_for_workspaces_with_mock_returns_empty_on_timeout() {
        let source = MockCookieSource {
            cookie: None,
            workspaces: Vec::new(),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(poll_for_workspaces_with(&source, 10, 1));
        assert!(result.is_empty());
    }

    #[test]
    fn poll_for_cookie_retries_until_cookie_found() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        struct RetrySource {
            call_count: Arc<AtomicU32>,
            cookie: Option<BrowserCookie>,
        }

        #[async_trait]
        impl CookieSource for RetrySource {
            async fn find_cookie(&self, _name: &str, _domain: &str) -> Option<BrowserCookie> {
                let n = self.call_count.fetch_add(1, Ordering::SeqCst);
                if n < 2 { None } else { self.cookie.clone() }
            }
            async fn discover_workspaces(&self) -> Vec<WorkspaceInfo> { Vec::new() }
        }

        let calls = Arc::new(AtomicU32::new(0));
        let source = RetrySource {
            call_count: calls.clone(),
            cookie: Some(BrowserCookie { value: "found".into() }),
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(poll_for_cookie_with(&source, "auth", "opencode.ai", 10, 10));
        assert_eq!(result.unwrap().value, "found");
        // Must have retried at least 3 times (2 None + 1 Some)
        assert!(calls.load(Ordering::SeqCst) >= 3);
    }

    #[test]
    fn poll_for_workspaces_retries_until_found() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        struct RetrySource {
            call_count: Arc<AtomicU32>,
            workspaces: Vec<WorkspaceInfo>,
        }

        #[async_trait]
        impl CookieSource for RetrySource {
            async fn find_cookie(&self, _name: &str, _domain: &str) -> Option<BrowserCookie> { None }
            async fn discover_workspaces(&self) -> Vec<WorkspaceInfo> {
                let n = self.call_count.fetch_add(1, Ordering::SeqCst);
                if n < 2 { Vec::new() } else { self.workspaces.clone() }
            }
        }
        let calls = Arc::new(AtomicU32::new(0));
        let source = RetrySource {
            call_count: calls.clone(),
            workspaces: vec![WorkspaceInfo { id: "wrk_test".into(), name: None }],
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(poll_for_workspaces_with(&source, 10, 10));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "wrk_test");
        assert!(calls.load(Ordering::SeqCst) >= 3);
    }
}