// SPDX-License-Identifier: MPL-2.0

use super::reconcile_provider_account_descriptors;
use crate::config::{Config, managed_ollama_cloud_account_dir};
use crate::error::AppError;
use crate::model::{AppState, ProviderId, UsageSnapshot};
use crate::providers::interface::{
    BoxFuture, ProviderAccountDescriptor, ProviderAccountHandle, ProviderAdapter,
    ProviderCapabilities,
};
use crate::providers::ollama_cloud;

pub(super) struct OllamaCloudAdapter;

impl ProviderAdapter for OllamaCloudAdapter {
    fn id(&self) -> ProviderId {
        ProviderId::OllamaCloud
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_delete: true,
            supports_reauthentication: false,
            supports_background_status_refresh: false,
            requires_auth_prompt_on_auth_failure: false,
        }
    }

    fn discover_accounts(&self, config: &Config) -> Vec<ProviderAccountDescriptor> {
        let capabilities = self.capabilities();
        ollama_cloud::discover_accounts(config)
            .into_iter()
            .filter_map(|account| {
                config
                    .ollama_cloud_managed_accounts
                    .iter()
                    .find(|managed| managed.id == account.id)
                    .cloned()
                    .map(|managed| ProviderAccountDescriptor {
                        provider: self.id(),
                        account_id: account.id,
                        label: account.label,
                        capabilities,
                        handle: ProviderAccountHandle::OllamaCloud(managed),
                    })
            })
            .collect()
    }

    fn delete_account(&self, account_id: &str, config: &mut Config) -> bool {
        if !config
            .ollama_cloud_managed_accounts
            .iter()
            .any(|a| a.id == account_id)
        {
            return false;
        }
        ollama_cloud::remove_managed_config_dir(&managed_ollama_cloud_account_dir(account_id));
        config
            .ollama_cloud_managed_accounts
            .retain(|a| a.id != account_id);
        config
            .selected_ollama_cloud_account_ids
            .retain(|id| id != account_id);
        true
    }

    fn reconcile_provider_accounts(&self, config: &Config, state: &mut AppState) {
        let accounts = self.discover_accounts(config);
        reconcile_provider_account_descriptors(self.id(), config, state, &accounts);
    }

    fn fetch_account<'a>(
        &self,
        handle: &'a ProviderAccountHandle,
        client: &'a reqwest::Client,
    ) -> BoxFuture<'a, crate::error::Result<UsageSnapshot, AppError>> {
        Box::pin(async move {
            match handle {
                ProviderAccountHandle::OllamaCloud(account) => {
                    ollama_cloud::fetch(client, account).await.map_err(AppError::from)
                }
                _ => unreachable!(),
            }
        })
    }
}