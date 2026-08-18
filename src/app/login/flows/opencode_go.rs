// SPDX-License-Identifier: MPL-2.0

use super::super::{LoginEventKind, LoginFlow, cancel_login};
use crate::app::{
    AccountSelectionStatus, AppModel, Config, Handle, Message, OpencodeGoLoginEvent,
    OpencodeGoLoginState, OpencodeGoLoginStatus, ProviderAccountRuntimeState, ProviderId, Task,
    opencode_go, refresh_provider_task_for_process,
};

pub(crate) struct OpencodeGoLoginFlow;

impl LoginFlow for OpencodeGoLoginFlow {
    type State = OpencodeGoLoginState;
    type Event = OpencodeGoLoginEvent;
    const PROVIDER: ProviderId = ProviderId::OpencodeGo;

    fn state(app: &AppModel) -> &Option<Self::State> {
        &app.opencode_go_login
    }
    fn state_mut(app: &mut AppModel) -> &mut Option<Self::State> {
        &mut app.opencode_go_login
    }
    fn handle_mut(app: &mut AppModel) -> &mut Option<Handle> {
        &mut app.opencode_go_login_handle
    }
    fn is_running(state: &Self::State) -> bool {
        matches!(
            state.status,
            OpencodeGoLoginStatus::Editing
                | OpencodeGoLoginStatus::Polling
                | OpencodeGoLoginStatus::SelectWorkspace
        )
    }
    fn log_id(state: &Self::State) -> &str {
        &state.account_id
    }
    fn status_debug(state: &Self::State) -> String {
        format!("{:?}", state.status)
    }
    fn account_exists(config: &Config, account_id: &str) -> bool {
        config
            .opencode_go_managed_accounts
            .iter()
            .any(|a| a.id == account_id)
    }
    fn failed_state(error: String) -> Self::State {
        OpencodeGoLoginState {
            account_id: "failed".to_string(),
            label: String::new(),
            status: OpencodeGoLoginStatus::Failed,
            workspace_id: String::new(),
            auth_cookie: String::new(),
            discovered_workspaces: Vec::new(),
            error: Some(error),
        }
    }
    fn prepare(config: Config) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        let _ = config;
        Ok((opencode_go::prepare_login(), cosmic::iced::Task::none()))
    }
    fn wrap_event(event: Self::Event) -> Message {
        Message::LoginEvent(
            ProviderId::OpencodeGo,
            Box::new(LoginEventKind::OpencodeGo(event)),
        )
    }
    fn on_event(app: &mut AppModel, event: Self::Event) -> Task<Message> {
        match event {
            OpencodeGoLoginEvent::Started
            | OpencodeGoLoginEvent::BrowserAuthStarted => Task::none(),
            OpencodeGoLoginEvent::BrowserAuthComplete {
                auth_cookie,
                workspaces,
            } => {
                if let Some(login) = app.opencode_go_login.as_mut() {
                    login.auth_cookie = auth_cookie;
                    login.error = None;
                    login.discovered_workspaces = workspaces.clone();
                    if workspaces.len() == 1 {
                        login.workspace_id = workspaces[0].id.clone();
                        login.status = OpencodeGoLoginStatus::Editing;
                    } else if workspaces.is_empty() {
                        login.status = OpencodeGoLoginStatus::Editing;
                    } else {
                        login.status = OpencodeGoLoginStatus::SelectWorkspace;
                    }
                }
                Task::none()
            }
            OpencodeGoLoginEvent::WorkspaceSelected(workspace_id) => {
                if let Some(login) = app.opencode_go_login.as_mut() {
                    login.update_workspace_id(workspace_id);
                }
                Task::none()
            }
            OpencodeGoLoginEvent::WorkspaceIdChanged(workspace_id) => {
                if let Some(login) = app.opencode_go_login.as_mut() {
                    login.update_workspace_id(workspace_id);
                }
                Task::none()
            }
            OpencodeGoLoginEvent::AuthCookieChanged(auth_cookie) => {
                if let Some(login) = app.opencode_go_login.as_mut() {
                    login.update_auth_cookie(auth_cookie);
                }
                Task::none()
            }
            OpencodeGoLoginEvent::LabelChanged(label) => {
                if let Some(login) = app.opencode_go_login.as_mut() {
                    login.update_label(label);
                }
                Task::none()
            }
            OpencodeGoLoginEvent::Saved => {
                let Some(login) = app.opencode_go_login.as_ref() else {
                    return Task::none();
                };
                match login.save(&mut app.config) {
                    Ok(managed_account) => {
                        app.opencode_go_login_handle = None;
                        let account_id = managed_account.id.clone();
                        let account_label = managed_account.label.clone();
                        app.write_config(|new_config| {
                            opencode_go::account::apply_login_account(new_config, managed_account);
                        });
                        let mut account = ProviderAccountRuntimeState::empty(
                            ProviderId::OpencodeGo,
                            account_id.clone(),
                            account_label,
                        );
                        account.auth_state = crate::model::AuthState::Ready;
                        account.error = None;
                        app.state.upsert_account(account);
                        if let Some(provider) = app.state.provider_mut(ProviderId::OpencodeGo) {
                            provider.account_status = AccountSelectionStatus::Ready;
                            provider.error = None;
                            if !provider.selected_account_ids.contains(&account_id) {
                                provider.selected_account_ids.push(account_id.clone());
                            }
                        }
                        app.persist_runtime_if_owner("opencode_go_account_saved");
                        app.opencode_go_login = None;
                        let process = app.refresh_task_process();
                        refresh_provider_task_for_process(
                            &app.config,
                            &mut app.state,
                            ProviderId::OpencodeGo,
                            Some(process),
                            false,
                        )
                    }
                    Err(error) => {
                        if let Some(login) = app.opencode_go_login.as_mut() {
                            login.error = Some(error);
                            login.status = OpencodeGoLoginStatus::Failed;
                        }
                        Task::none()
                    }
                }
            }
            OpencodeGoLoginEvent::Cancelled => {
                cancel_login::<OpencodeGoLoginFlow>(app);
                Task::none()
            }
            OpencodeGoLoginEvent::Failed(error) => {
                if let Some(login) = app.opencode_go_login.as_mut() {
                    login.error = Some(error);
                    login.status = OpencodeGoLoginStatus::Failed;
                }
                Task::none()
            }
        }
    }
}
