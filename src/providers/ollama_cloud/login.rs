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
    #[allow(dead_code)]
    Polling,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    BrowserAuthComplete { session_cookie: String },
    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn state_with(session_cookie: &str) -> OllamaCloudLoginState {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = format!("ollama-cloud-test-{}", COUNTER.fetch_add(1, Ordering::SeqCst));
        let mut state = OllamaCloudLoginState::new(id);
        state.session_cookie = session_cookie.to_string();
        state
    }

    #[test]
    fn new_creates_correct_initial_state() {
        let state = OllamaCloudLoginState::new("ollama-cloud-abc".to_string());
        assert_eq!(state.account_id, "ollama-cloud-abc");
        assert_eq!(state.label, "");
        assert_eq!(state.status, OllamaCloudLoginStatus::Editing);
        assert_eq!(state.session_cookie, "");
        assert!(state.error.is_none());
    }

    #[test]
    fn prepare_generates_account_id_prefix() {
        let state = prepare();
        assert!(
            state.account_id.starts_with("ollama-cloud-"),
            "account_id should start with 'ollama-cloud-', got: {}",
            state.account_id
        );
        assert!(
            state.account_id.len() > "ollama-cloud-".len(),
            "account_id should have a suffix after the prefix"
        );
    }

    #[test]
    fn update_session_cookie_works() {
        let mut state = OllamaCloudLoginState::new("id".to_string());
        assert_eq!(state.session_cookie, "");
        state.update_session_cookie("sess-123".to_string());
        assert_eq!(state.session_cookie, "sess-123");
    }

    #[test]
    fn update_label_works() {
        let mut state = OllamaCloudLoginState::new("id".to_string());
        assert_eq!(state.label, "");
        state.update_label("my account".to_string());
        assert_eq!(state.label, "my account");
    }

    #[test]
    fn save_succeeds_with_valid_session_cookie() {
        let state = state_with("session-1");
        let mut config = Config::default();
        let result = state.save(&mut config);

        let account = result.expect("save should succeed with a valid session cookie");
        assert!(account.id.starts_with("ollama-cloud-test-"));
        assert_eq!(account.label, "");
        assert_eq!(account.session_cookie_source, "stored");
        assert!(account.last_authenticated_at.is_some());
        assert_eq!(account.created_at, account.updated_at);
        assert_eq!(account.updated_at, account.last_authenticated_at.unwrap());
        let _ = std::fs::remove_dir_all(crate::config::managed_ollama_cloud_account_dir(&account.id));
    }

    #[test]
    fn save_fails_with_empty_session_cookie() {
        let state = state_with("");
        let mut config = Config::default();
        let err = state.save(&mut config).expect_err("should fail on empty session_cookie");
        assert_eq!(err, "Session cookie is required");
    }

    #[test]
    fn save_fails_with_whitespace_only_session_cookie() {
        let state = state_with("   \t\n ");
        let mut config = Config::default();
        let err = state
            .save(&mut config)
            .expect_err("should fail on whitespace-only session_cookie");
        assert_eq!(err, "Session cookie is required");
    }

    #[test]
    fn save_trims_whitespace_from_session_cookie() {
        let state = state_with("  session-1  ");
        let mut config = Config::default();
        let account = state.save(&mut config).expect("save should succeed");
        let account_dir = crate::config::managed_ollama_cloud_account_dir(&account.id);
        let stored = std::fs::read_to_string(account_dir.join("session_cookie.txt"))
            .expect("session cookie file should exist");
        assert_eq!(stored, "session-1");
        let _ = std::fs::remove_dir_all(&account_dir);
    }

    #[test]
    fn save_trims_label() {
        let mut state = state_with("session-1");
        state.label = "  my label  ".to_string();
        let mut config = Config::default();
        let account = state.save(&mut config).expect("save should succeed");
        assert_eq!(account.label, "my label");
        let _ = std::fs::remove_dir_all(crate::config::managed_ollama_cloud_account_dir(&account.id));
    }

    #[test]
    fn save_uses_account_id_from_state() {
        let state = state_with("session-1");
        let mut config = Config::default();
        let account = state.save(&mut config).expect("save should succeed");
        assert!(account.id.starts_with("ollama-cloud-test-"));
        let _ = std::fs::remove_dir_all(crate::config::managed_ollama_cloud_account_dir(&account.id));
    }

    #[test]
    fn save_returns_session_cookie_source_stored() {
        let state = state_with("session-1");
        let mut config = Config::default();
        let account = state.save(&mut config).expect("save should succeed");
        assert_eq!(account.session_cookie_source, "stored");
        let _ = std::fs::remove_dir_all(crate::config::managed_ollama_cloud_account_dir(&account.id));
    }
}