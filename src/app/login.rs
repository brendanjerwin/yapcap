use super::{
    AccountSelectionStatus, AppModel, ClaudeLoginEvent, ClaudeLoginStatus, CodexLoginEvent,
    CodexLoginState, CodexLoginStatus, CopilotLoginEvent, CopilotLoginState, CopilotLoginStatus,
    CursorScanResult, CursorScanState, GeminiLoginEvent, GeminiLoginState, GeminiLoginStatus,
    ManagedClaudeAccountConfig, ManagedCodexAccountConfig, ManagedCursorAccountConfig, Message,
    MinimaxLoginEvent, MinimaxLoginStatus, ProviderAccountRuntimeState, ProviderHealth, ProviderId,
    Task, claude, codex, copilot, cursor, gemini, minimax, refresh_provider_task_for_process,
    runtime,
};
use crate::shared_state::RefreshRequestReason;

impl AppModel {
    pub(super) fn reauthenticate_codex_account(&mut self, account_id: &str) -> Task<Message> {
        if self
            .config
            .codex_managed_accounts
            .iter()
            .all(|a| a.id != account_id)
        {
            log_reauth_ignored(&self.process_info.id, ProviderId::Codex, account_id);
            return Task::none();
        }
        log_reauth_requested(&self.process_info.id, ProviderId::Codex, account_id);
        self.start_codex_login()
    }

    pub(super) fn start_codex_login(&mut self) -> Task<Message> {
        if self
            .codex_login
            .as_ref()
            .is_some_and(|login| login.status == CodexLoginStatus::Running)
        {
            log_login_already_running(&self.process_info.id, ProviderId::Codex);
            return Task::none();
        }
        log_login_requested(&self.process_info.id, ProviderId::Codex);
        self.codex_login = None;
        let (state, task) = match codex::prepare(self.config.clone()) {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::info!(process_id = %self.process_info.id, provider = ProviderId::Codex.label(), error = %error, "login preparation failed");
                self.codex_login = Some(CodexLoginState {
                    flow_id: "failed".to_string(),
                    status: CodexLoginStatus::Failed,
                    login_url: None,
                    output: Vec::new(),
                    error: Some(error),
                });
                return Task::none();
            }
        };
        self.start_codex_login_task(state, task)
    }

    pub(super) fn start_codex_login_task(
        &mut self,
        state: CodexLoginState,
        task: cosmic::iced::Task<CodexLoginEvent>,
    ) -> Task<Message> {
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Codex.label(),
            flow_id = %state.flow_id,
            "login flow started"
        );
        self.codex_login = Some(state);
        let task = task.map(|event| cosmic::Action::App(Message::CodexLoginEvent(Box::new(event))));
        let (task, handle) = task.abortable();
        self.codex_login_handle = Some(handle);
        task
    }

    pub(super) fn cancel_codex_login(&mut self) {
        match self.codex_login.as_ref() {
            Some(login) => log_login_state_cleared(
                &self.process_info.id,
                ProviderId::Codex,
                &login.flow_id,
                login.status,
            ),
            None => log_login_state_clear_ignored(&self.process_info.id, ProviderId::Codex),
        }
        if let Some(handle) = self.codex_login_handle.take() {
            handle.abort();
        }
        self.codex_login = None;
    }

    pub(super) fn handle_codex_login_event(&mut self, event: CodexLoginEvent) -> Task<Message> {
        match event {
            CodexLoginEvent::Output {
                flow_id,
                line,
                login_url,
            } => {
                let Some(login) = self.codex_login.as_mut() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                if let Some(url) = login_url {
                    login.login_url = Some(url);
                }
                login.output.push(line);
                if login.output.len() > 8 {
                    login.output.remove(0);
                }
                Task::none()
            }
            CodexLoginEvent::Finished { flow_id, result } => {
                let Some(login) = self.codex_login.as_mut() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                self.codex_login_handle = None;
                match *result {
                    Ok(success) => {
                        login.status = CodexLoginStatus::Succeeded;
                        login.error = None;
                        tracing::info!(
                            process_id = %self.process_info.id,
                            provider = ProviderId::Codex.label(),
                            flow_id = %flow_id,
                            account_id = %success.account.id,
                            "login flow succeeded"
                        );
                        self.write_config(|new_config| {
                            codex::apply_login_account(new_config, success.account.clone());
                        });
                        runtime::reconcile_provider(
                            &self.config,
                            &mut self.state,
                            ProviderId::Codex,
                        );
                        self.sync_panel_suggested_bounds();
                        self.request_provider_refresh(
                            ProviderId::Codex,
                            RefreshRequestReason::AccountAction,
                        )
                    }
                    Err(error) => {
                        login.status = CodexLoginStatus::Failed;
                        tracing::info!(
                            process_id = %self.process_info.id,
                            provider = ProviderId::Codex.label(),
                            flow_id = %flow_id,
                            error = %error,
                            "login flow failed"
                        );
                        login.error = Some(error);
                        Task::none()
                    }
                }
            }
        }
    }

    pub(super) fn update_codex_metadata_from_state(&mut self) {
        let updates = self
            .state
            .accounts_for(ProviderId::Codex)
            .into_iter()
            .filter_map(codex_managed_metadata_update)
            .collect::<Vec<_>>();
        if updates.is_empty() {
            return;
        }

        self.write_config(|new_config| {
            for update in &updates {
                if let Some(account) = new_config
                    .codex_managed_accounts
                    .iter_mut()
                    .find(|account| account.id == update.id)
                {
                    apply_codex_metadata_update(account, update);
                }
            }
        });
        runtime::reconcile_provider(&self.config, &mut self.state, ProviderId::Codex);
    }

    pub(super) fn clear_codex_legacy_snapshot_after_success(&mut self) {
        let active_ok = self
            .state
            .active_account(ProviderId::Codex)
            .is_some_and(|account| {
                account.health == ProviderHealth::Ok && account.snapshot.is_some()
            });
        if !active_ok {
            return;
        }
        if let Some(provider) = self.state.provider_mut(ProviderId::Codex) {
            provider.legacy_display_snapshot = None;
        }
    }

    pub(super) fn update_claude_metadata_from_state(&mut self) {
        let updates = self
            .state
            .accounts_for(ProviderId::Claude)
            .into_iter()
            .filter_map(claude_managed_metadata_update)
            .collect::<Vec<_>>();
        if updates.is_empty() {
            return;
        }

        self.write_config(|new_config| {
            for update in &updates {
                if let Some(account) = new_config
                    .claude_managed_accounts
                    .iter_mut()
                    .find(|account| account.id == update.id)
                {
                    apply_claude_metadata_update(account, update);
                }
            }
        });
        runtime::reconcile_provider(&self.config, &mut self.state, ProviderId::Claude);
    }

    pub(super) fn clear_claude_legacy_snapshot_after_success(&mut self) {
        let active_ok = self
            .state
            .active_account(ProviderId::Claude)
            .is_some_and(|account| {
                account.health == ProviderHealth::Ok && account.snapshot.is_some()
            });
        if !active_ok {
            return;
        }
        if let Some(provider) = self.state.provider_mut(ProviderId::Claude) {
            provider.legacy_display_snapshot = None;
        }
    }

    pub(super) fn update_cursor_metadata_from_state(&mut self) {
        let updates = self
            .state
            .accounts_for(ProviderId::Cursor)
            .into_iter()
            .filter_map(cursor_managed_metadata_update)
            .collect::<Vec<_>>();
        if updates.is_empty() {
            return;
        }
        self.write_config(|new_config| {
            for update in &updates {
                if let Some(account) = new_config.cursor_managed_accounts.iter_mut().find(|a| {
                    (!a.id.is_empty() && a.id == update.config_id) || a.email == update.config_id
                }) {
                    apply_cursor_metadata_update(account, update);
                }
            }
        });
        runtime::reconcile_provider(&self.config, &mut self.state, ProviderId::Cursor);
    }

    pub(super) fn update_cursor_active_account(&mut self) {
        if let Some(provider_state) = self.state.provider_mut(ProviderId::Cursor) {
            provider_state.active_account_id = provider_state.selected_account_ids.first().cloned();
        }
    }

    pub(super) fn start_claude_login(&mut self) -> Task<Message> {
        if self
            .claude_login
            .as_ref()
            .is_some_and(|login| login.status == ClaudeLoginStatus::Running)
        {
            log_login_already_running(&self.process_info.id, ProviderId::Claude);
            return Task::none();
        }
        log_login_requested(&self.process_info.id, ProviderId::Claude);
        self.claude_login = None;
        let state = claude::prepare();
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Claude.label(),
            flow_id = %state.flow_id,
            "login flow started"
        );
        self.claude_login = Some(state);
        Task::none()
    }

    pub(super) fn reauthenticate_claude_account(&mut self, account_id: &str) -> Task<Message> {
        if self
            .config
            .claude_managed_accounts
            .iter()
            .all(|a| a.id != account_id)
        {
            log_reauth_ignored(&self.process_info.id, ProviderId::Claude, account_id);
            return Task::none();
        }
        if self
            .claude_login
            .as_ref()
            .is_some_and(|login| login.status == ClaudeLoginStatus::Running)
        {
            log_login_already_running(&self.process_info.id, ProviderId::Claude);
            return Task::none();
        }
        log_reauth_requested(&self.process_info.id, ProviderId::Claude, account_id);
        self.claude_login = None;
        let state = claude::prepare_targeted(account_id.to_string());
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Claude.label(),
            flow_id = %state.flow_id,
            account_id,
            "reauthentication flow started"
        );
        self.claude_login = Some(state);
        Task::none()
    }

    pub(super) fn update_claude_login_code(&mut self, code: String) {
        if let Some(login) = self.claude_login.as_mut()
            && login.status == ClaudeLoginStatus::Running
        {
            login.code_input = code;
        }
    }

    pub(super) fn submit_claude_login_code(&mut self) -> Task<Message> {
        let Some(login) = self.claude_login.as_mut() else {
            return Task::none();
        };
        if login.status != ClaudeLoginStatus::Running || login.code_input.trim().is_empty() {
            return Task::none();
        }
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Claude.label(),
            flow_id = %login.flow_id,
            "login code submitted"
        );
        login.error = None;
        login.output.push("Completing Claude sign-in".to_string());
        if login.output.len() > 8 {
            login.output.remove(0);
        }
        let task = claude::submit_code(login, self.config.clone());
        let task =
            task.map(|event| cosmic::Action::App(Message::ClaudeLoginEvent(Box::new(event))));
        let (task, handle) = task.abortable();
        self.claude_login_handle = Some(handle);
        task
    }

    pub(super) fn cancel_claude_login(&mut self) {
        match self.claude_login.as_ref() {
            Some(login) => log_login_state_cleared(
                &self.process_info.id,
                ProviderId::Claude,
                &login.flow_id,
                login.status,
            ),
            None => log_login_state_clear_ignored(&self.process_info.id, ProviderId::Claude),
        }
        if let Some(handle) = self.claude_login_handle.take() {
            handle.abort();
        }
        self.claude_login = None;
    }

    pub(super) fn handle_claude_login_event(&mut self, event: ClaudeLoginEvent) -> Task<Message> {
        match event {
            ClaudeLoginEvent::Finished { flow_id, result } => {
                let Some(login) = self.claude_login.as_mut() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                self.claude_login_handle = None;
                match *result {
                    Ok(success) => {
                        login.status = ClaudeLoginStatus::Succeeded;
                        login.error = None;
                        tracing::info!(
                            process_id = %self.process_info.id,
                            provider = ProviderId::Claude.label(),
                            flow_id = %flow_id,
                            account_id = %success.account.id,
                            "login flow succeeded"
                        );
                        self.write_config(|new_config| {
                            claude::apply_login_account(new_config, success.account.clone());
                        });
                        runtime::reconcile_provider(
                            &self.config,
                            &mut self.state,
                            ProviderId::Claude,
                        );
                        self.sync_panel_suggested_bounds();
                        self.request_provider_refresh(
                            ProviderId::Claude,
                            RefreshRequestReason::AccountAction,
                        )
                    }
                    Err(error) => {
                        login.status = ClaudeLoginStatus::Failed;
                        tracing::info!(
                            process_id = %self.process_info.id,
                            provider = ProviderId::Claude.label(),
                            flow_id = %flow_id,
                            error = %error,
                            "login flow failed"
                        );
                        login.error = Some(error);
                        Task::none()
                    }
                }
            }
        }
    }

    pub(super) fn reauthenticate_cursor_account(&mut self, account_id: &str) -> Task<Message> {
        if cursor::find_managed_account(&self.config.cursor_managed_accounts, account_id).is_none()
        {
            log_reauth_ignored(&self.process_info.id, ProviderId::Cursor, account_id);
            return Task::none();
        }
        log_reauth_requested(&self.process_info.id, ProviderId::Cursor, account_id);
        self.start_cursor_scan()
    }

    pub(super) fn start_cursor_scan(&mut self) -> Task<Message> {
        if matches!(self.cursor_scan, CursorScanState::Scanning) {
            tracing::info!(
                process_id = %self.process_info.id,
                provider = ProviderId::Cursor.label(),
                "cursor scan already running"
            );
            return Task::none();
        }
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Cursor.label(),
            "cursor account scan started"
        );
        self.cursor_scan = CursorScanState::Scanning;
        self.cursor_scan_result = None;
        let existing = self.config.cursor_managed_accounts.clone();
        Task::perform(
            async move {
                let client = runtime::http_client();
                cursor::scan(&client, &existing).await
            },
            |(state, result)| cosmic::Action::App(Message::CursorScanComplete(state, result)),
        )
    }

    pub(super) fn handle_cursor_scan_complete(
        &mut self,
        state: CursorScanState,
        result: Option<CursorScanResult>,
    ) {
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Cursor.label(),
            state = cursor_scan_state_label(&state),
            result_available = result.is_some(),
            "cursor account scan completed"
        );
        self.cursor_scan = state;
        self.cursor_scan_result = result;
    }

    pub(super) fn confirm_cursor_scan(&mut self) -> Task<Message> {
        let Some(result) = self.cursor_scan_result.take() else {
            self.cursor_scan = CursorScanState::Idle;
            return Task::none();
        };
        match cursor::confirm_scan(&self.config.cursor_managed_accounts, &result) {
            Ok(new_account) => {
                let mut applied_account = new_account.clone();
                self.write_config(|new_config| {
                    applied_account = cursor::upsert_managed_account(new_config, new_account);
                });
                runtime::reconcile_provider(&self.config, &mut self.state, ProviderId::Cursor);
                self.cursor_scan = CursorScanState::Idle;
                self.sync_panel_suggested_bounds();
                tracing::info!(
                    process_id = %self.process_info.id,
                    provider = ProviderId::Cursor.label(),
                    account_id = %applied_account.id,
                    "cursor account scan confirmed"
                );
                self.request_provider_refresh(
                    ProviderId::Cursor,
                    RefreshRequestReason::AccountAction,
                )
            }
            Err(error) => {
                tracing::info!(
                    process_id = %self.process_info.id,
                    provider = ProviderId::Cursor.label(),
                    error = %error,
                    "cursor account scan confirmation failed"
                );
                self.cursor_scan = CursorScanState::Error(error);
                Task::none()
            }
        }
    }

    pub(super) fn dismiss_cursor_scan(&mut self) {
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Cursor.label(),
            "cursor account scan dismissed"
        );
        self.cursor_scan = CursorScanState::Idle;
        self.cursor_scan_result = None;
    }

    pub(super) fn reauthenticate_gemini_account(&mut self, account_id: &str) -> Task<Message> {
        if self
            .config
            .gemini_managed_accounts
            .iter()
            .all(|a| a.id != account_id)
        {
            log_reauth_ignored(&self.process_info.id, ProviderId::Gemini, account_id);
            return Task::none();
        }
        if self
            .gemini_login
            .as_ref()
            .is_some_and(|login| login.status == GeminiLoginStatus::Running)
        {
            log_login_already_running(&self.process_info.id, ProviderId::Gemini);
            return Task::none();
        }
        log_reauth_requested(&self.process_info.id, ProviderId::Gemini, account_id);
        self.gemini_login = None;
        let (state, task) = match gemini::prepare_for_reauth(self.config.clone(), account_id) {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::info!(process_id = %self.process_info.id, provider = ProviderId::Gemini.label(), account_id, error = %error, "reauthentication preparation failed");
                self.gemini_login = Some(GeminiLoginState {
                    flow_id: "failed".to_string(),
                    status: GeminiLoginStatus::Failed,
                    login_url: None,
                    output: Vec::new(),
                    error: Some(error),
                });
                return Task::none();
            }
        };
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Gemini.label(),
            flow_id = %state.flow_id,
            account_id,
            "reauthentication flow started"
        );
        self.gemini_login = Some(state);
        let task =
            task.map(|event| cosmic::Action::App(Message::GeminiLoginEvent(Box::new(event))));
        let (task, handle) = task.abortable();
        self.gemini_login_handle = Some(handle);
        task
    }

    pub(super) fn start_gemini_login(&mut self) -> Task<Message> {
        if self
            .gemini_login
            .as_ref()
            .is_some_and(|login| login.status == GeminiLoginStatus::Running)
        {
            log_login_already_running(&self.process_info.id, ProviderId::Gemini);
            return Task::none();
        }
        log_login_requested(&self.process_info.id, ProviderId::Gemini);
        self.gemini_login = None;
        let (state, task) = match gemini::prepare(self.config.clone()) {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::info!(process_id = %self.process_info.id, provider = ProviderId::Gemini.label(), error = %error, "login preparation failed");
                self.gemini_login = Some(GeminiLoginState {
                    flow_id: "failed".to_string(),
                    status: GeminiLoginStatus::Failed,
                    login_url: None,
                    output: Vec::new(),
                    error: Some(error),
                });
                return Task::none();
            }
        };
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Gemini.label(),
            flow_id = %state.flow_id,
            "login flow started"
        );
        self.gemini_login = Some(state);
        let task =
            task.map(|event| cosmic::Action::App(Message::GeminiLoginEvent(Box::new(event))));
        let (task, handle) = task.abortable();
        self.gemini_login_handle = Some(handle);
        task
    }

    pub(super) fn cancel_gemini_login(&mut self) {
        match self.gemini_login.as_ref() {
            Some(login) => log_login_state_cleared(
                &self.process_info.id,
                ProviderId::Gemini,
                &login.flow_id,
                login.status,
            ),
            None => log_login_state_clear_ignored(&self.process_info.id, ProviderId::Gemini),
        }
        if let Some(handle) = self.gemini_login_handle.take() {
            handle.abort();
        }
        self.gemini_login = None;
    }

    pub(super) fn handle_gemini_login_event(&mut self, event: GeminiLoginEvent) -> Task<Message> {
        match event {
            GeminiLoginEvent::Output {
                flow_id,
                line,
                login_url,
            } => {
                let Some(login) = self.gemini_login.as_mut() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                if let Some(url) = login_url {
                    login.login_url = Some(url);
                }
                login.output.push(line);
                if login.output.len() > 8 {
                    login.output.remove(0);
                }
                Task::none()
            }
            GeminiLoginEvent::Finished { flow_id, result } => {
                let Some(login) = self.gemini_login.as_mut() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                self.gemini_login_handle = None;
                match *result {
                    Ok(success) => {
                        login.status = GeminiLoginStatus::Succeeded;
                        login.error = None;
                        tracing::info!(
                            process_id = %self.process_info.id,
                            provider = ProviderId::Gemini.label(),
                            flow_id = %flow_id,
                            account_id = %success.account.id,
                            "login flow succeeded"
                        );
                        self.write_config(|new_config| {
                            gemini::apply_login_account(new_config, success.account.clone());
                        });
                        runtime::reconcile_provider(
                            &self.config,
                            &mut self.state,
                            ProviderId::Gemini,
                        );
                        self.sync_panel_suggested_bounds();
                        self.request_provider_refresh(
                            ProviderId::Gemini,
                            RefreshRequestReason::AccountAction,
                        )
                    }
                    Err(error) => {
                        login.status = GeminiLoginStatus::Failed;
                        tracing::info!(
                            process_id = %self.process_info.id,
                            provider = ProviderId::Gemini.label(),
                            flow_id = %flow_id,
                            error = %error,
                            "login flow failed"
                        );
                        login.error = Some(error);
                        Task::none()
                    }
                }
            }
        }
    }

    pub(super) fn reauthenticate_copilot_account(&mut self, account_id: &str) -> Task<Message> {
        if self
            .config
            .copilot_managed_accounts
            .iter()
            .all(|a| a.id != account_id)
        {
            log_reauth_ignored(&self.process_info.id, ProviderId::Copilot, account_id);
            return Task::none();
        }
        if self
            .copilot_login
            .as_ref()
            .is_some_and(|login| login.status == CopilotLoginStatus::Running)
        {
            log_login_already_running(&self.process_info.id, ProviderId::Copilot);
            return Task::none();
        }
        log_reauth_requested(&self.process_info.id, ProviderId::Copilot, account_id);
        self.copilot_login = None;
        let (state, task) = match copilot::prepare_for_reauth(self.config.clone(), account_id) {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::info!(process_id = %self.process_info.id, provider = ProviderId::Copilot.label(), account_id, error = %error, "reauthentication preparation failed");
                self.copilot_login = Some(CopilotLoginState {
                    flow_id: "failed".to_string(),
                    status: CopilotLoginStatus::Failed,
                    user_code: None,
                    verification_uri: None,
                    output: Vec::new(),
                    error: Some(error),
                    code_copied: false,
                    expected_github_user_id: None,
                });
                return Task::none();
            }
        };
        self.start_copilot_login_task(state, task)
    }

    pub(super) fn start_copilot_login(&mut self) -> Task<Message> {
        if self
            .copilot_login
            .as_ref()
            .is_some_and(|login| login.status == CopilotLoginStatus::Running)
        {
            log_login_already_running(&self.process_info.id, ProviderId::Copilot);
            return Task::none();
        }
        log_login_requested(&self.process_info.id, ProviderId::Copilot);
        self.copilot_login = None;
        let (state, task) = match copilot::prepare(self.config.clone()) {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::info!(process_id = %self.process_info.id, provider = ProviderId::Copilot.label(), error = %error, "login preparation failed");
                self.copilot_login = Some(CopilotLoginState {
                    flow_id: "failed".to_string(),
                    status: CopilotLoginStatus::Failed,
                    user_code: None,
                    verification_uri: None,
                    output: Vec::new(),
                    error: Some(error),
                    code_copied: false,
                    expected_github_user_id: None,
                });
                return Task::none();
            }
        };
        self.start_copilot_login_task(state, task)
    }

    fn start_copilot_login_task(
        &mut self,
        state: CopilotLoginState,
        task: cosmic::iced::Task<CopilotLoginEvent>,
    ) -> Task<Message> {
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Copilot.label(),
            flow_id = %state.flow_id,
            "login flow started"
        );
        self.copilot_login = Some(state);
        let task =
            task.map(|event| cosmic::Action::App(Message::CopilotLoginEvent(Box::new(event))));
        let (task, handle) = task.abortable();
        self.copilot_login_handle = Some(handle);
        task
    }

    pub(super) fn cancel_copilot_login(&mut self) {
        match self.copilot_login.as_ref() {
            Some(login) => log_login_state_cleared(
                &self.process_info.id,
                ProviderId::Copilot,
                &login.flow_id,
                login.status,
            ),
            None => log_login_state_clear_ignored(&self.process_info.id, ProviderId::Copilot),
        }
        if let Some(handle) = self.copilot_login_handle.take() {
            handle.abort();
        }
        self.copilot_login = None;
    }

    pub(super) fn copy_copilot_login_code(&mut self, code: String) -> Task<Message> {
        let Some(login) = self.copilot_login.as_mut() else {
            return Task::none();
        };
        login.code_copied = true;
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Copilot.label(),
            flow_id = %login.flow_id,
            "copilot login code copied"
        );
        let flow_id = login.flow_id.clone();
        let write: Task<Message> = cosmic::iced::clipboard::write(code);
        let clear: Task<Message> = Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                flow_id
            },
            |flow_id| cosmic::Action::App(Message::ClearCopilotLoginCodeCopied(flow_id)),
        );
        write.chain(clear)
    }

    pub(super) fn clear_copilot_login_code_copied(&mut self, flow_id: &str) {
        if let Some(login) = self.copilot_login.as_mut()
            && login.flow_id == flow_id
        {
            login.code_copied = false;
        }
    }

    pub(super) fn handle_copilot_login_event(&mut self, event: CopilotLoginEvent) -> Task<Message> {
        match event {
            CopilotLoginEvent::Code {
                flow_id,
                user_code,
                verification_uri,
            } => {
                let Some(login) = self.copilot_login.as_mut() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                login.user_code = Some(user_code);
                login.verification_uri = Some(verification_uri);
                tracing::info!(
                    process_id = %self.process_info.id,
                    provider = ProviderId::Copilot.label(),
                    flow_id = %flow_id,
                    verification_uri_available = true,
                    user_code_available = true,
                    "login device code received"
                );
                Task::none()
            }
            CopilotLoginEvent::Output { flow_id, line } => {
                let Some(login) = self.copilot_login.as_mut() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                login.output.push(line);
                if login.output.len() > 8 {
                    login.output.remove(0);
                }
                Task::none()
            }
            CopilotLoginEvent::Finished { flow_id, result } => {
                let Some(login) = self.copilot_login.as_mut() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                self.copilot_login_handle = None;
                match *result {
                    Ok(success) => {
                        login.status = CopilotLoginStatus::Succeeded;
                        login.error = None;
                        tracing::info!(
                            process_id = %self.process_info.id,
                            provider = ProviderId::Copilot.label(),
                            flow_id = %flow_id,
                            account_id = %success.account.id,
                            "login flow succeeded"
                        );
                        self.write_config(|new_config| {
                            copilot::apply_login_account(new_config, success.account.clone());
                        });
                        runtime::reconcile_provider(
                            &self.config,
                            &mut self.state,
                            ProviderId::Copilot,
                        );
                        self.sync_panel_suggested_bounds();
                        self.request_provider_refresh(
                            ProviderId::Copilot,
                            RefreshRequestReason::AccountAction,
                        )
                    }
                    Err(error) => {
                        login.status = CopilotLoginStatus::Failed;
                        tracing::info!(
                            process_id = %self.process_info.id,
                            provider = ProviderId::Copilot.label(),
                            flow_id = %flow_id,
                            error = %error,
                            "login flow failed"
                        );
                        login.error = Some(error);
                        Task::none()
                    }
                }
            }
        }
    }

    pub(super) fn reauthenticate_minimax_account(&mut self, account_id: &str) -> Task<Message> {
        if self
            .config
            .minimax_managed_accounts
            .iter()
            .all(|a| a.id != account_id)
        {
            return Task::none();
        }
        self.start_minimax_login()
    }

    pub(super) fn start_minimax_login(&mut self) -> Task<Message> {
        if self
            .minimax_login
            .as_ref()
            .is_some_and(|login| login.status == MinimaxLoginStatus::Editing)
        {
            return Task::none();
        }
        self.minimax_login = None;
        let state = minimax::prepare_login();
        self.minimax_login = Some(state);
        Task::none()
    }

    pub(super) fn cancel_minimax_login(&mut self) {
        if let Some(handle) = self.minimax_login_handle.take() {
            handle.abort();
        }
        self.minimax_login = None;
    }

    pub(super) fn handle_minimax_login_event(&mut self, event: MinimaxLoginEvent) -> Task<Message> {
        match event {
            MinimaxLoginEvent::Started => Task::none(),
            MinimaxLoginEvent::ApiKeyChanged(api_key) => {
                if let Some(login) = self.minimax_login.as_mut() {
                    login.update_api_key(api_key);
                }
                Task::none()
            }
            MinimaxLoginEvent::LabelChanged(label) => {
                if let Some(login) = self.minimax_login.as_mut() {
                    login.update_label(label);
                }
                Task::none()
            }
            MinimaxLoginEvent::Saved => {
                let Some(login) = self.minimax_login.as_ref() else {
                    return Task::none();
                };
                match login.save(&mut self.config) {
                    Ok(managed_account) => {
                        self.minimax_login_handle = None;
                        let account_id = managed_account.id.clone();
                        let account_label = managed_account.label.clone();
                        self.write_config(|new_config| {
                            minimax::account::apply_login_account(new_config, managed_account);
                        });
                        let mut account = ProviderAccountRuntimeState::empty(
                            ProviderId::Minimax,
                            account_id.clone(),
                            account_label,
                        );
                        account.auth_state = crate::model::AuthState::Ready;
                        account.error = None;
                        self.state.upsert_account(account);
                        if let Some(provider) = self.state.provider_mut(ProviderId::Minimax) {
                            provider.account_status = AccountSelectionStatus::Ready;
                            provider.error = None;
                            // Add the new account to selected ids if not already there
                            if !provider.selected_account_ids.contains(&account_id) {
                                provider.selected_account_ids.push(account_id.clone());
                            }
                        }
                        self.persist_runtime_if_owner("minimax_account_saved");
                        let login = self.minimax_login.as_mut().unwrap();
                        login.status = MinimaxLoginStatus::Saved;
                        let process = self.refresh_task_process();
                        refresh_provider_task_for_process(
                            &self.config,
                            &mut self.state,
                            ProviderId::Minimax,
                            Some(process),
                        )
                    }
                    Err(error) => {
                        if let Some(login) = self.minimax_login.as_mut() {
                            login.error = Some(error);
                            login.status = MinimaxLoginStatus::Failed;
                        }
                        Task::none()
                    }
                }
            }
            MinimaxLoginEvent::Cancelled => {
                self.cancel_minimax_login();
                Task::none()
            }
            MinimaxLoginEvent::Failed(error) => {
                if let Some(login) = self.minimax_login.as_mut() {
                    login.error = Some(error);
                    login.status = MinimaxLoginStatus::Failed;
                }
                Task::none()
            }
        }
    }
}

fn log_login_requested(process_id: &str, provider: ProviderId) {
    tracing::info!(
        process_id,
        provider = provider.label(),
        "login flow requested"
    );
}

fn log_login_already_running(process_id: &str, provider: ProviderId) {
    tracing::info!(
        process_id,
        provider = provider.label(),
        "login flow ignored because one is already running"
    );
}

fn log_reauth_requested(process_id: &str, provider: ProviderId, account_id: &str) {
    tracing::info!(
        process_id,
        provider = provider.label(),
        account_id,
        "reauthentication requested"
    );
}

fn log_reauth_ignored(process_id: &str, provider: ProviderId, account_id: &str) {
    tracing::info!(
        process_id,
        provider = provider.label(),
        account_id,
        "reauthentication ignored because account was not configured"
    );
}

fn cursor_scan_state_label(state: &CursorScanState) -> &'static str {
    match state {
        CursorScanState::Idle => "idle",
        CursorScanState::Scanning => "scanning",
        CursorScanState::Found { .. } => "found",
        CursorScanState::AlreadyConnected { .. } => "already_connected",
        CursorScanState::Error(_) => "error",
    }
}

fn log_login_state_cleared(
    process_id: &str,
    provider: ProviderId,
    flow_id: &str,
    status_before: impl std::fmt::Debug,
) {
    tracing::info!(
        process_id,
        provider = provider.label(),
        flow_id,
        status_before = ?status_before,
        "login state cleared"
    );
}

fn log_login_state_clear_ignored(process_id: &str, provider: ProviderId) {
    tracing::info!(
        process_id,
        provider = provider.label(),
        reason = "no_login_state",
        "login state clear ignored"
    );
}

#[derive(Clone)]
struct CodexMetadataUpdate {
    id: String,
    label: Option<String>,
    email: Option<String>,
    provider_account_id: Option<String>,
}

fn codex_managed_metadata_update(
    account: &ProviderAccountRuntimeState,
) -> Option<CodexMetadataUpdate> {
    let snapshot = account.snapshot.as_ref()?;
    Some(CodexMetadataUpdate {
        id: account.account_id.clone(),
        label: snapshot.identity.email.clone(),
        email: snapshot.identity.email.clone(),
        provider_account_id: snapshot.identity.account_id.clone(),
    })
}

fn apply_codex_metadata_update(
    account: &mut ManagedCodexAccountConfig,
    update: &CodexMetadataUpdate,
) {
    if let Some(label) = &update.label
        && account.label == "Codex account"
    {
        account.label.clone_from(label);
    }
    if update.email.is_some() {
        account.email.clone_from(&update.email);
    }
    if update.provider_account_id.is_some() {
        account
            .provider_account_id
            .clone_from(&update.provider_account_id);
    }
    account.updated_at = chrono::Utc::now();
}

#[derive(Clone)]
struct ClaudeMetadataUpdate {
    id: String,
    label: Option<String>,
    email: Option<String>,
    subscription_type: Option<String>,
}

fn claude_managed_metadata_update(
    account: &ProviderAccountRuntimeState,
) -> Option<ClaudeMetadataUpdate> {
    let snapshot = account.snapshot.as_ref()?;
    Some(ClaudeMetadataUpdate {
        id: account.account_id.clone(),
        label: snapshot.identity.email.clone(),
        email: snapshot.identity.email.clone(),
        subscription_type: snapshot.identity.plan.clone(),
    })
}

fn apply_claude_metadata_update(
    account: &mut ManagedClaudeAccountConfig,
    update: &ClaudeMetadataUpdate,
) {
    if let Some(label) = &update.label {
        account.label.clone_from(label);
    }
    if update.email.is_some() {
        account.email.clone_from(&update.email);
    }
    if update.subscription_type.is_some() {
        account
            .subscription_type
            .clone_from(&update.subscription_type);
    }
    account.updated_at = chrono::Utc::now();
}

#[derive(Clone)]
struct CursorMetadataUpdate {
    config_id: String,
    email: String,
    display_name: Option<String>,
    plan: Option<String>,
}

fn cursor_managed_metadata_update(
    account: &ProviderAccountRuntimeState,
) -> Option<CursorMetadataUpdate> {
    let config_id = cursor::managed_config_id(&account.account_id)?;
    let snapshot = account.snapshot.as_ref()?;
    Some(CursorMetadataUpdate {
        config_id: config_id.to_string(),
        email: snapshot
            .identity
            .email
            .as_deref()
            .map_or_else(|| config_id.to_string(), cursor::normalized_email),
        display_name: snapshot.identity.display_name.clone(),
        plan: snapshot.identity.plan.clone(),
    })
}

fn apply_cursor_metadata_update(
    account: &mut ManagedCursorAccountConfig,
    update: &CursorMetadataUpdate,
) {
    account.label.clone_from(&update.email);
    account.email.clone_from(&update.email);
    if update.display_name.is_some() {
        account.display_name.clone_from(&update.display_name);
    }
    if update.plan.is_some() {
        account.plan.clone_from(&update.plan);
    }
    account.updated_at = chrono::Utc::now();
}
