use super::super::{log_reauth_ignored, log_reauth_requested};
use crate::app::{
    AppModel, CursorScanResult, CursorScanState, ManagedCursorAccountConfig, Message,
    ProviderAccountRuntimeState, ProviderId, Task, cursor, runtime,
};
use crate::shared_state::RefreshRequestReason;

impl AppModel {
    pub(in crate::app) fn reauthenticate_cursor_account(
        &mut self,
        account_id: &str,
    ) -> Task<Message> {
        if cursor::find_managed_account(&self.config.cursor_managed_accounts, account_id).is_none()
        {
            log_reauth_ignored(&self.process_info.id, ProviderId::Cursor, account_id);
            return Task::none();
        }
        log_reauth_requested(&self.process_info.id, ProviderId::Cursor, account_id);
        self.start_cursor_scan()
    }

    pub(in crate::app) fn start_cursor_scan(&mut self) -> Task<Message> {
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

    pub(in crate::app) fn handle_cursor_scan_complete(
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

    pub(in crate::app) fn confirm_cursor_scan(&mut self) -> Task<Message> {
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

    pub(in crate::app) fn dismiss_cursor_scan(&mut self) {
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Cursor.label(),
            "cursor account scan dismissed"
        );
        self.cursor_scan = CursorScanState::Idle;
        self.cursor_scan_result = None;
    }

    pub(in crate::app) fn update_cursor_metadata_from_state(&mut self) {
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

    pub(in crate::app) fn update_cursor_active_account(&mut self) {
        if let Some(provider_state) = self.state.provider_mut(ProviderId::Cursor) {
            provider_state.active_account_id = provider_state.selected_account_ids.first().cloned();
        }
    }
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
