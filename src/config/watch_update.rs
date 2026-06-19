// SPDX-License-Identifier: MPL-2.0

use super::Config;

impl Config {
    pub fn apply_watcher_update(&mut self, update: Self, keys: &[&str]) {
        if keys.is_empty() {
            *self = update;
            return;
        }
        for key in keys {
            if !self.apply_general_watcher_key(&update, key) {
                self.apply_account_watcher_key(&update, key);
            }
        }
    }

    fn apply_general_watcher_key(&mut self, update: &Self, key: &str) -> bool {
        match key {
            "refresh_interval_seconds" => {
                self.refresh_interval_seconds = update.refresh_interval_seconds;
            }
            "reset_time_format" => self.reset_time_format = update.reset_time_format,
            "usage_amount_format" => self.usage_amount_format = update.usage_amount_format,
            "panel_icon_style" => self.panel_icon_style = update.panel_icon_style,
            "selected_provider" => self.selected_provider = update.selected_provider,
            "provider_visibility_mode" => {
                self.provider_visibility_mode = update.provider_visibility_mode;
            }
            "codex_enabled" => self.codex_enabled = update.codex_enabled,
            "claude_enabled" => self.claude_enabled = update.claude_enabled,
            "cursor_enabled" => self.cursor_enabled = update.cursor_enabled,
            "gemini_enabled" => self.gemini_enabled = update.gemini_enabled,
            "copilot_enabled" => self.copilot_enabled = update.copilot_enabled,
            "show_all_accounts" => self.show_all_accounts = update.show_all_accounts.clone(),
            "log_level" => self.log_level.clone_from(&update.log_level),
            _ => return false,
        }
        true
    }

    fn apply_account_watcher_key(&mut self, update: &Self, key: &str) {
        match key {
            "selected_codex_account_ids" => {
                self.selected_codex_account_ids = update.selected_codex_account_ids.clone();
            }
            "codex_managed_accounts" => {
                self.codex_managed_accounts = update.codex_managed_accounts.clone();
            }
            "selected_claude_account_ids" => {
                self.selected_claude_account_ids = update.selected_claude_account_ids.clone();
            }
            "claude_managed_accounts" => {
                self.claude_managed_accounts = update.claude_managed_accounts.clone();
            }
            "selected_cursor_account_ids" => {
                self.selected_cursor_account_ids = update.selected_cursor_account_ids.clone();
            }
            "cursor_managed_accounts" => {
                self.cursor_managed_accounts = update.cursor_managed_accounts.clone();
            }
            "selected_gemini_account_ids" => {
                self.selected_gemini_account_ids = update.selected_gemini_account_ids.clone();
            }
            "gemini_managed_accounts" => {
                self.gemini_managed_accounts = update.gemini_managed_accounts.clone();
            }
            "selected_copilot_account_ids" => {
                self.selected_copilot_account_ids = update.selected_copilot_account_ids.clone();
            }
            "copilot_managed_accounts" => {
                self.copilot_managed_accounts = update.copilot_managed_accounts.clone();
            }
            _ => {}
        }
    }
}
