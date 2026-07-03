// SPDX-License-Identifier: MPL-2.0

use crate::config::{Config, ManagedOpencodeGoAccountConfig};
use crate::providers::opencode_go::storage::{create_private_dir, write_auth_cookie, write_workspace_id};
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct OpencodeGoLoginState {
    pub account_id: String,
    pub label: String,
    pub status: OpencodeGoLoginStatus,
    pub workspace_id: String,
    pub auth_cookie: String,
    pub discovered_workspaces: Vec<crate::browser_cookies::WorkspaceInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpencodeGoLoginStatus {
    Editing,
    Polling,
    SelectWorkspace,
    Saved,
    Failed,
}

impl OpencodeGoLoginState {
    pub fn new(account_id: String) -> Self {
        Self {
            account_id,
            label: String::new(),
            status: OpencodeGoLoginStatus::Editing,
            workspace_id: String::new(),
            auth_cookie: String::new(),
            discovered_workspaces: Vec::new(),
            error: None,
        }
    }

    pub fn update_label(&mut self, label: String) {
        self.label = label;
    }

    pub fn update_workspace_id(&mut self, workspace_id: String) {
        self.workspace_id = workspace_id;
    }

    pub fn update_auth_cookie(&mut self, auth_cookie: String) {
        self.auth_cookie = auth_cookie;
    }

    pub fn save(&self, _config: &mut Config) -> Result<ManagedOpencodeGoAccountConfig, String> {
        let workspace_id = self.workspace_id.trim().to_string();
        let auth_cookie = self.auth_cookie.trim().to_string();

        if workspace_id.is_empty() {
            return Err("Workspace ID is required".to_string());
        }
        if auth_cookie.is_empty() {
            return Err("Auth cookie is required".to_string());
        }

        let account_id = self.account_id.clone();
        let label = self.label.trim().to_string();
        let auth_cookie_source = "stored".to_string();
        let now = Utc::now();

        let managed_account = ManagedOpencodeGoAccountConfig {
            id: account_id.clone(),
            label,
            workspace_id: workspace_id.clone(),
            auth_cookie_source,
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        };

        let account_dir = crate::config::managed_opencode_go_account_dir(&account_id);
        create_private_dir(&account_dir)?;
        write_workspace_id(&account_dir, &workspace_id)?;
        write_auth_cookie(&account_dir, &auth_cookie)?;

        Ok(managed_account)
    }
}

#[derive(Debug, Clone)]
pub enum OpencodeGoLoginEvent {
    #[allow(dead_code)]
    BrowserAuthStarted,
    BrowserAuthComplete {
        auth_cookie: String,
        workspaces: Vec<crate::browser_cookies::WorkspaceInfo>,
    },
    WorkspaceSelected(String),
    Started,
    WorkspaceIdChanged(String),
    AuthCookieChanged(String),
    LabelChanged(String),
    Saved,
    #[allow(dead_code)]
    Cancelled,
    #[allow(dead_code)]
    Failed(String),
}

pub fn prepare() -> OpencodeGoLoginState {
    let account_id = format!(
        "opencode-go-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    OpencodeGoLoginState::new(account_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn state_with(workspace_id: &str, auth_cookie: &str) -> OpencodeGoLoginState {
        let mut state = OpencodeGoLoginState::new("opencode-go-test".to_string());
        state.workspace_id = workspace_id.to_string();
        state.auth_cookie = auth_cookie.to_string();
        state
    }

    #[test]
    fn new_creates_correct_initial_state() {
        let state = OpencodeGoLoginState::new("opencode-go-abc".to_string());
        assert_eq!(state.account_id, "opencode-go-abc");
        assert_eq!(state.label, "");
        assert_eq!(state.status, OpencodeGoLoginStatus::Editing);
        assert_eq!(state.workspace_id, "");
        assert_eq!(state.auth_cookie, "");
        assert!(state.discovered_workspaces.is_empty());
        assert!(state.error.is_none());
    }

    #[test]
    fn prepare_generates_account_id_prefix() {
        let state = prepare();
        assert!(
            state.account_id.starts_with("opencode-go-"),
            "account_id should start with 'opencode-go-', got: {}",
            state.account_id
        );
        assert!(
            state.account_id.len() > "opencode-go-".len(),
            "account_id should have a suffix after the prefix"
        );
    }

    #[test]
    fn update_workspace_id_works() {
        let mut state = OpencodeGoLoginState::new("id".to_string());
        assert_eq!(state.workspace_id, "");
        state.update_workspace_id("ws-123".to_string());
        assert_eq!(state.workspace_id, "ws-123");
    }

    #[test]
    fn update_auth_cookie_works() {
        let mut state = OpencodeGoLoginState::new("id".to_string());
        assert_eq!(state.auth_cookie, "");
        state.update_auth_cookie("cookie-val".to_string());
        assert_eq!(state.auth_cookie, "cookie-val");
    }

    #[test]
    fn update_label_works() {
        let mut state = OpencodeGoLoginState::new("id".to_string());
        assert_eq!(state.label, "");
        state.update_label("my account".to_string());
        assert_eq!(state.label, "my account");
    }

    #[test]
    fn save_succeeds_with_valid_fields() {
        let state = state_with("workspace-1", "auth-cookie-1");
        let mut config = Config::default();
        let result = state.save(&mut config);

        let account = result.expect("save should succeed with valid inputs");
        assert_eq!(account.id, "opencode-go-test");
        assert_eq!(account.workspace_id, "workspace-1");
        assert_eq!(account.label, "");
        assert_eq!(account.auth_cookie_source, "stored");
        assert!(account.last_authenticated_at.is_some());
        assert_eq!(account.created_at, account.updated_at);
        assert_eq!(account.updated_at, account.last_authenticated_at.unwrap());
    }

    #[test]
    fn save_fails_with_empty_workspace_id() {
        let state = state_with("", "auth-cookie-1");
        let mut config = Config::default();
        let err = state.save(&mut config).expect_err("should fail on empty workspace_id");
        assert_eq!(err, "Workspace ID is required");
    }

    #[test]
    fn save_fails_with_whitespace_only_workspace_id() {
        let state = state_with("   \t ", "auth-cookie-1");
        let mut config = Config::default();
        let err = state.save(&mut config).expect_err("should fail on whitespace-only workspace_id");
        assert_eq!(err, "Workspace ID is required");
    }

    #[test]
    fn save_fails_with_empty_auth_cookie() {
        let state = state_with("workspace-1", "");
        let mut config = Config::default();
        let err = state.save(&mut config).expect_err("should fail on empty auth_cookie");
        assert_eq!(err, "Auth cookie is required");
    }

    #[test]
    fn save_fails_with_whitespace_only_auth_cookie() {
        let state = state_with("workspace-1", " \n ");
        let mut config = Config::default();
        let err = state.save(&mut config).expect_err("should fail on whitespace-only auth_cookie");
        assert_eq!(err, "Auth cookie is required");
    }

    #[test]
    fn save_trims_workspace_id_and_auth_cookie() {
        let state = state_with("  workspace-1  ", "  auth-cookie-1  ");
        let mut config = Config::default();
        let account = state.save(&mut config).expect("save should succeed");
        assert_eq!(account.workspace_id, "workspace-1");
        // auth_cookie is not stored on the config struct, only written to disk;
        // verify the trimmed value was persisted by reading it back.
        let account_dir = crate::config::managed_opencode_go_account_dir(&account.id);
        let stored = std::fs::read_to_string(account_dir.join("auth_cookie.txt"))
            .expect("auth cookie file should exist");
        assert_eq!(stored, "auth-cookie-1");
    }

    #[test]
    fn save_trims_label() {
        let mut state = state_with("workspace-1", "auth-cookie-1");
        state.label = "  my label  ".to_string();
        let mut config = Config::default();
        let account = state.save(&mut config).expect("save should succeed");
        assert_eq!(account.label, "my label");
    }

    #[test]
    fn save_uses_account_id_from_state() {
        let state = state_with("workspace-1", "auth-cookie-1");
        let mut config = Config::default();
        let account = state.save(&mut config).expect("save should succeed");
        assert_eq!(account.id, "opencode-go-test");
    }

    #[test]
    fn save_returns_auth_cookie_source_stored() {
        let state = state_with("workspace-1", "auth-cookie-1");
        let mut config = Config::default();
        let account = state.save(&mut config).expect("save should succeed");
        assert_eq!(account.auth_cookie_source, "stored");
    }
}