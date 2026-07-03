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