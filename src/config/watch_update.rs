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
            "codex_enablement" => self.codex_enablement = update.codex_enablement,
            "claude_enablement" => self.claude_enablement = update.claude_enablement,
            "cursor_enablement" => self.cursor_enablement = update.cursor_enablement,
            "gemini_enablement" => self.gemini_enablement = update.gemini_enablement,
            "copilot_enablement" => self.copilot_enablement = update.copilot_enablement,
            "minimax_enablement" => self.minimax_enablement = update.minimax_enablement,
            "antigravity_enablement" => {
                self.antigravity_enablement = update.antigravity_enablement;
            }
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
            "selected_minimax_account_ids" => {
                self.selected_minimax_account_ids = update.selected_minimax_account_ids.clone();
            }
            "minimax_managed_accounts" => {
                self.minimax_managed_accounts = update.minimax_managed_accounts.clone();
            }
            "selected_antigravity_account_ids" => {
                self.selected_antigravity_account_ids =
                    update.selected_antigravity_account_ids.clone();
            }
            "antigravity_managed_accounts" => {
                self.antigravity_managed_accounts = update.antigravity_managed_accounts.clone();
            }
            _ => {}
        }
    }
}
