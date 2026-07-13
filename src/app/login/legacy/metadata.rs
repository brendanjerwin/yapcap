use crate::app::{
    AppModel, ManagedClaudeAccountConfig, ManagedCodexAccountConfig, ProviderAccountRuntimeState,
    ProviderHealth, ProviderId, runtime,
};

impl AppModel {
    pub(in crate::app) fn update_codex_metadata_from_state(&mut self) {
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

    pub(in crate::app) fn clear_codex_legacy_snapshot_after_success(&mut self) {
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

    pub(in crate::app) fn update_claude_metadata_from_state(&mut self) {
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

    pub(in crate::app) fn clear_claude_legacy_snapshot_after_success(&mut self) {
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
