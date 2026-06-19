// SPDX-License-Identifier: MPL-2.0

use crate::model::{AppState, ProviderId};
use chrono::{DateTime, Utc};
use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

pub const RUNTIME_DOCUMENT_VERSION: u16 = 1;
pub const CONTROL_DOCUMENT_VERSION: u16 = 1;

#[derive(Debug, Clone, CosmicConfigEntry, Serialize, Deserialize, PartialEq)]
#[version = 1]
pub struct SharedRuntimeState {
    #[serde(default = "runtime_document_version")]
    pub document_version: u16,
    #[serde(default)]
    pub generation: u64,
    #[serde(default = "Utc::now")]
    pub written_at: DateTime<Utc>,
    pub app_state: AppState,
}

#[derive(Debug, Clone, CosmicConfigEntry, Serialize, Deserialize, PartialEq)]
#[version = 1]
pub struct SharedControlState {
    #[serde(default = "control_document_version")]
    pub document_version: u16,
    #[serde(default)]
    pub generation: u64,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub requests: Vec<ProviderRefreshRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderRefreshRequest {
    pub provider: ProviderId,
    pub reason: RefreshRequestReason,
    pub requested_at: DateTime<Utc>,
    pub requesting_process_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefreshRequestReason {
    User,
    ProviderSelected,
    AccountAction,
}

impl Default for SharedRuntimeState {
    fn default() -> Self {
        Self::new(AppState::empty(), 0)
    }
}

impl SharedRuntimeState {
    #[must_use]
    pub fn new(app_state: AppState, generation: u64) -> Self {
        Self {
            document_version: RUNTIME_DOCUMENT_VERSION,
            generation,
            written_at: Utc::now(),
            app_state,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn from_json(raw: &str) -> Option<Self> {
        serde_json::from_str(raw).ok()
    }
}

impl Default for SharedControlState {
    fn default() -> Self {
        Self {
            document_version: CONTROL_DOCUMENT_VERSION,
            generation: 0,
            updated_at: Utc::now(),
            requests: Vec::new(),
        }
    }
}

impl SharedControlState {
    #[must_use]
    pub fn with_cleared_requests(&self) -> Self {
        let mut next = self.clone();
        if next.requests.is_empty() {
            return next;
        }
        next.requests.clear();
        next.generation += 1;
        next.updated_at = Utc::now();
        next
    }

    pub fn upsert_request(&mut self, request: ProviderRefreshRequest) {
        if let Some(existing) = self
            .requests
            .iter_mut()
            .find(|existing| existing.provider == request.provider)
        {
            *existing = request;
        } else {
            self.requests.push(request);
        }
        self.generation += 1;
        self.updated_at = Utc::now();
    }

    #[must_use]
    pub fn with_provider_requests_removed(&self, provider: ProviderId) -> Self {
        let mut next = self.clone();
        let before = next.requests.len();
        next.requests.retain(|request| request.provider != provider);
        if next.requests.len() == before {
            return next;
        }
        next.generation += 1;
        next.updated_at = Utc::now();
        next
    }

    #[cfg(test)]
    #[must_use]
    pub fn from_json(raw: &str) -> Option<Self> {
        serde_json::from_str(raw).ok()
    }
}

#[must_use]
pub fn load_runtime(app_id: &str) -> Option<SharedRuntimeState> {
    let ctx = match cosmic_config::Config::new(app_id, SharedRuntimeState::VERSION) {
        Ok(ctx) => ctx,
        Err(error) => {
            tracing::warn!(
                error = ?error,
                document_version = RUNTIME_DOCUMENT_VERSION,
                "shared runtime missing; using empty runtime fallback"
            );
            return None;
        }
    };
    match SharedRuntimeState::get_entry(&ctx) {
        Ok(shared) => {
            tracing::info!(
                generation = shared.generation,
                document_version = shared.document_version,
                provider_count = shared.app_state.providers.len(),
                account_count = shared.app_state.provider_accounts.len(),
                "shared runtime loaded"
            );
            Some(shared)
        }
        Err((errors, _fallback)) => {
            tracing::warn!(
                error_count = errors.len(),
                document_version = RUNTIME_DOCUMENT_VERSION,
                "shared runtime invalid; using empty runtime fallback"
            );
            None
        }
    }
}

#[must_use]
pub fn load_control(app_id: &str) -> SharedControlState {
    let ctx = match cosmic_config::Config::new(app_id, SharedControlState::VERSION) {
        Ok(ctx) => ctx,
        Err(error) => {
            tracing::warn!(
                error = ?error,
                document_version = CONTROL_DOCUMENT_VERSION,
                "shared control missing; using empty control fallback"
            );
            return SharedControlState::default();
        }
    };
    match SharedControlState::get_entry(&ctx) {
        Ok(shared) => {
            tracing::info!(
                generation = shared.generation,
                document_version = shared.document_version,
                request_count = shared.requests.len(),
                "shared control loaded"
            );
            shared
        }
        Err((errors, _fallback)) => {
            tracing::warn!(
                error_count = errors.len(),
                document_version = CONTROL_DOCUMENT_VERSION,
                "shared control invalid; using empty control fallback"
            );
            SharedControlState::default()
        }
    }
}

pub fn save_runtime(app_id: &str, state: &AppState) -> Result<(), cosmic_config::Error> {
    let ctx = cosmic_config::Config::new(app_id, SharedRuntimeState::VERSION)?;
    let generation = SharedRuntimeState::get_entry(&ctx)
        .ok()
        .map_or(1, |shared| shared.generation.saturating_add(1));
    let shared = SharedRuntimeState::new(state.clone(), generation);
    shared.write_entry(&ctx)?;
    tracing::info!(
        generation = shared.generation,
        document_version = shared.document_version,
        provider_count = shared.app_state.providers.len(),
        account_count = shared.app_state.provider_accounts.len(),
        "shared runtime written"
    );
    Ok(())
}

pub fn save_control(app_id: &str, state: &SharedControlState) -> Result<(), cosmic_config::Error> {
    let ctx = cosmic_config::Config::new(app_id, SharedControlState::VERSION)?;
    state.write_entry(&ctx)?;
    tracing::info!(
        generation = state.generation,
        document_version = state.document_version,
        request_count = state.requests.len(),
        "shared control written"
    );
    Ok(())
}

pub fn clear_control_requests(
    app_id: &str,
    state: &SharedControlState,
) -> Result<SharedControlState, cosmic_config::Error> {
    let cleared = state.with_cleared_requests();
    if cleared == *state {
        return Ok(cleared);
    }
    save_control(app_id, &cleared)?;
    Ok(cleared)
}

pub fn remove_control_requests_for_provider(
    app_id: &str,
    state: &SharedControlState,
    provider: ProviderId,
) -> Result<SharedControlState, cosmic_config::Error> {
    let next = state.with_provider_requests_removed(provider);
    if next == *state {
        return Ok(next);
    }
    save_control(app_id, &next)?;
    Ok(next)
}

fn runtime_document_version() -> u16 {
    RUNTIME_DOCUMENT_VERSION
}

fn control_document_version() -> u16 {
    CONTROL_DOCUMENT_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ProviderRuntimeState;

    #[test]
    fn invalid_shared_runtime_returns_none() {
        assert!(SharedRuntimeState::from_json("{").is_none());
    }

    #[test]
    fn shared_runtime_round_trips_app_state() {
        let mut app_state = AppState::empty();
        app_state.upsert_provider(ProviderRuntimeState::disabled(ProviderId::Claude));
        let shared = SharedRuntimeState::new(app_state.clone(), 7);
        let raw = serde_json::to_string(&shared).unwrap();
        let loaded = SharedRuntimeState::from_json(&raw).unwrap();

        assert_eq!(loaded.document_version, RUNTIME_DOCUMENT_VERSION);
        assert_eq!(loaded.generation, 7);
        assert_eq!(loaded.app_state.providers.len(), app_state.providers.len());
        assert!(
            !loaded
                .app_state
                .provider(ProviderId::Claude)
                .unwrap()
                .enabled
        );
    }

    #[test]
    fn shared_control_round_trips_and_updates_provider_request() {
        let mut control = SharedControlState::default();
        control.upsert_request(ProviderRefreshRequest {
            provider: ProviderId::Codex,
            reason: RefreshRequestReason::User,
            requested_at: Utc::now(),
            requesting_process_id: "first".to_string(),
        });
        control.upsert_request(ProviderRefreshRequest {
            provider: ProviderId::Codex,
            reason: RefreshRequestReason::ProviderSelected,
            requested_at: Utc::now(),
            requesting_process_id: "second".to_string(),
        });

        let raw = serde_json::to_string(&control).unwrap();
        let loaded = SharedControlState::from_json(&raw).unwrap();

        assert_eq!(loaded.document_version, CONTROL_DOCUMENT_VERSION);
        assert_eq!(loaded.generation, 2);
        assert_eq!(loaded.requests.len(), 1);
        assert_eq!(
            loaded.requests[0].reason,
            RefreshRequestReason::ProviderSelected
        );
        assert_eq!(loaded.requests[0].requesting_process_id, "second");
    }

    #[test]
    fn shared_control_clear_removes_requests_and_advances_generation() {
        let mut control = SharedControlState::default();
        control.upsert_request(ProviderRefreshRequest {
            provider: ProviderId::Codex,
            reason: RefreshRequestReason::User,
            requested_at: Utc::now(),
            requesting_process_id: "process".to_string(),
        });
        let generation = control.generation;

        let cleared = control.with_cleared_requests();

        assert!(cleared.requests.is_empty());
        assert_eq!(cleared.generation, generation + 1);
    }
}
