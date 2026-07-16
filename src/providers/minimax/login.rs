// SPDX-License-Identifier: MPL-2.0

use crate::config::{Config, ManagedMinimaxAccountConfig};
use crate::providers::minimax::storage::{create_private_dir, write_api_key};
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct MinimaxLoginState {
    pub account_id: String,
    pub label: String,
    pub status: MinimaxLoginStatus,
    pub api_key: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimaxLoginStatus {
    Editing,
    Failed,
}

impl MinimaxLoginState {
    pub fn new(account_id: String) -> Self {
        Self {
            account_id,
            label: String::new(),
            status: MinimaxLoginStatus::Editing,
            api_key: String::new(),
            error: None,
        }
    }

    pub fn update_label(&mut self, label: String) {
        self.label = label;
    }

    pub fn update_api_key(&mut self, api_key: String) {
        self.api_key = api_key;
    }

    pub fn save(&self, _config: &mut Config) -> Result<ManagedMinimaxAccountConfig, String> {
        if self.api_key.is_empty() {
            return Err("API key is required".to_string());
        }

        let account_id = self.account_id.clone();
        let label = self.label.clone();
        let api_key_source = "stored".to_string();
        let now = Utc::now();

        let managed_account = ManagedMinimaxAccountConfig {
            id: account_id.clone(),
            label: label.clone(),
            api_key_source,
            created_at: now,
            updated_at: now,
            last_authenticated_at: Some(now),
        };

        let account_dir = crate::config::managed_minimax_account_dir(&account_id);
        create_private_dir(&account_dir)?;
        write_api_key(&account_dir, &self.api_key)?;

        Ok(managed_account)
    }
}

#[derive(Debug, Clone)]
pub enum MinimaxLoginEvent {
    // TODO: These variants are used in pattern matching (app/login.rs) but never constructed.
    // This suggests incomplete implementation. They should either be constructed where appropriate
    // or removed and the pattern matching updated.
    #[allow(dead_code)]
    Started,
    ApiKeyChanged(String),
    LabelChanged(String),
    Saved,
    #[allow(dead_code)]
    Cancelled,
    #[allow(dead_code)]
    Failed(String),
}

pub fn prepare() -> MinimaxLoginState {
    let account_id = format!(
        "minimax-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    );
    MinimaxLoginState::new(account_id)
}
