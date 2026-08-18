// SPDX-License-Identifier: MPL-2.0

use super::super::{LoginEventKind, LoginFlow, cancel_login};
use crate::app::{
    AccountSelectionStatus, AppModel, Config, Handle, Message, OllamaCloudLoginEvent,
    OllamaCloudLoginState, OllamaCloudLoginStatus, ProviderAccountRuntimeState, ProviderId, Task,
    ollama_cloud, refresh_provider_task_for_process,
};

pub(crate) struct OllamaCloudLoginFlow;

impl LoginFlow for OllamaCloudLoginFlow {
    type State = OllamaCloudLoginState;
    type Event = OllamaCloudLoginEvent;
    const PROVIDER: ProviderId = ProviderId::OllamaCloud;

    fn state(app: &AppModel) -> &Option<Self::State> {
        &app.ollama_cloud_login
    }
    fn state_mut(app: &mut AppModel) -> &mut Option<Self::State> {
        &mut app.ollama_cloud_login
    }
    fn handle_mut(app: &mut AppModel) -> &mut Option<Handle> {
        &mut app.ollama_cloud_login_handle
    }
    fn is_running(state: &Self::State) -> bool {
        matches!(state.status, OllamaCloudLoginStatus::Editing | OllamaCloudLoginStatus::Polling)
    }
    fn log_id(state: &Self::State) -> &str {
        &state.account_id
    }
    fn status_debug(state: &Self::State) -> String {
        format!("{:?}", state.status)
    }
    fn account_exists(config: &Config, account_id: &str) -> bool {
        config
            .ollama_cloud_managed_accounts
            .iter()
            .any(|a| a.id == account_id)
    }
    fn failed_state(error: String) -> Self::State {
        OllamaCloudLoginState {
            account_id: "failed".to_string(),
            label: String::new(),
            status: OllamaCloudLoginStatus::Failed,
            session_cookie: String::new(),
            error: Some(error),
        }
    }
    fn prepare(config: Config) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        let _ = config;
        Ok((ollama_cloud::prepare_login(), cosmic::iced::Task::none()))
    }
    fn wrap_event(event: Self::Event) -> Message {
        Message::LoginEvent(
            ProviderId::OllamaCloud,
            Box::new(LoginEventKind::OllamaCloud(event)),
        )
    }
    fn on_event(app: &mut AppModel, event: Self::Event) -> Task<Message> {
        match event {
            OllamaCloudLoginEvent::Started
            | OllamaCloudLoginEvent::BrowserAuthStarted => Task::none(),
            OllamaCloudLoginEvent::BrowserAuthComplete { session_cookie } => {
                if let Some(login) = app.ollama_cloud_login.as_mut() {
                    login.session_cookie = session_cookie;
                    login.error = None;
                    login.status = OllamaCloudLoginStatus::Editing;
                }
                Task::none()
            }
            OllamaCloudLoginEvent::SessionCookieChanged(session_cookie) => {
                if let Some(login) = app.ollama_cloud_login.as_mut() {
                    login.update_session_cookie(session_cookie);
                }
                Task::none()
            }
            OllamaCloudLoginEvent::LabelChanged(label) => {
                if let Some(login) = app.ollama_cloud_login.as_mut() {
                    login.update_label(label);
                }
                Task::none()
            }
            OllamaCloudLoginEvent::Saved => {
                let Some(login) = app.ollama_cloud_login.as_ref() else {
                    return Task::none();
                };
                match login.save(&mut app.config) {
                    Ok(managed_account) => {
                        app.ollama_cloud_login_handle = None;
                        let account_id = managed_account.id.clone();
                        let account_label = managed_account.label.clone();
                        app.write_config(|new_config| {
                            ollama_cloud::account::apply_login_account(new_config, managed_account);
                        });
                        let mut account = ProviderAccountRuntimeState::empty(
                            ProviderId::OllamaCloud,
                            account_id.clone(),
                            account_label,
                        );
                        account.auth_state = crate::model::AuthState::Ready;
                        account.error = None;
                        app.state.upsert_account(account);
                        if let Some(provider) = app.state.provider_mut(ProviderId::OllamaCloud) {
                            provider.account_status = AccountSelectionStatus::Ready;
                            provider.error = None;
                            if !provider.selected_account_ids.contains(&account_id) {
                                provider.selected_account_ids.push(account_id.clone());
                            }
                        }
                        app.persist_runtime_if_owner("ollama_cloud_account_saved");
                        app.ollama_cloud_login = None;
                        let process = app.refresh_task_process();
                        refresh_provider_task_for_process(
                            &app.config,
                            &mut app.state,
                            ProviderId::OllamaCloud,
                            Some(process),
                            false,
                        )
                    }
                    Err(error) => {
                        if let Some(login) = app.ollama_cloud_login.as_mut() {
                            login.error = Some(error);
                            login.status = OllamaCloudLoginStatus::Failed;
                        }
                        Task::none()
                    }
                }
            }
            OllamaCloudLoginEvent::Cancelled => {
                cancel_login::<OllamaCloudLoginFlow>(app);
                Task::none()
            }
            OllamaCloudLoginEvent::Failed(error) => {
                if let Some(login) = app.ollama_cloud_login.as_mut() {
                    login.error = Some(error);
                    login.status = OllamaCloudLoginStatus::Failed;
                }
                Task::none()
            }
        }
    }
}
