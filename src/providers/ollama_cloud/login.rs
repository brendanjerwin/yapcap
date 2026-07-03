// SPDX-License-Identifier: MPL-2.0

use crate::config::{Config, ManagedOllamaCloudAccountConfig};
use crate::providers::ollama_cloud::storage::{create_private_dir, write_session_cookie};
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct OllamaCloudLoginState {
    pub account_id: String,
    pub label: String,
    pub status: OllamaCloudLoginStatus,
    pub session_cookie: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OllamaCloudLoginStatus {
    Editing,
    Polling,
    Saved,
    Failed,
}

impl OllamaCloudLoginState {
    pub fn new(account_id: String) -> Self {
        Self {
            account_id,
            label: String::new(),
            status: OllamaCloudLoginStatus::Editing,
            session_cookie: String::new(),
            error: None,
        }
    }

    pub fn update_label(&mut self, label: String) {
        self.label = label;
    }

    pub fn update_session_cookie(&mut self, session_cookie: String) {
        self.session_cookie = session_cookie;
    }

    pub fn save(&self, _config: &mut Config) -> Result<ManagedOllamaCloudAccountConfig, String> {
        let session_cookie = self.session_cookie.trim().to_string();

        if session_cookie.is_empty() {
            return Err("Session cookie is required".to_string());
        }

        let account_id = self.account_id.clone();
        let label = self.label.trim().to_string();
        let session_cookie_source = "stored".to_string();
        let now = Utc::now();

        let managed_account = ManagedOllamaCloudAccountConfig {
            id: account_id.clone(),
            label,
            session_cookie_source,
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        };

        let account_dir = crate::config::managed_ollama_cloud_account_dir(&account_id);
        create_private_dir(&account_dir)?;
        write_session_cookie(&account_dir, &session_cookie)?;

        Ok(managed_account)
    }
}

#[derive(Debug, Clone)]
pub enum OllamaCloudLoginEvent {
    #[allow(dead_code)]
    BrowserAuthStarted,
    BrowserAuthComplete { session_cookie: String },
    Started,
    SessionCookieChanged(String),
    LabelChanged(String),
    Saved,
    #[allow(dead_code)]
    Cancelled,
    #[allow(dead_code)]
    Failed(String),
}

pub fn prepare() -> OllamaCloudLoginState {
    let account_id = format!(
        "ollama-cloud-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    OllamaCloudLoginState::new(account_id)
}