use super::super::{LoginEventKind, LoginFlow, cancel_login};
use crate::app::{
    AccountSelectionStatus, AppModel, Config, Handle, Message, MinimaxLoginEvent,
    MinimaxLoginState, MinimaxLoginStatus, ProviderAccountRuntimeState, ProviderId, Task, minimax,
    refresh_provider_task_for_process,
};

pub(crate) struct MinimaxLoginFlow;

impl LoginFlow for MinimaxLoginFlow {
    type State = MinimaxLoginState;
    type Event = MinimaxLoginEvent;
    const PROVIDER: ProviderId = ProviderId::Minimax;

    fn state(app: &AppModel) -> &Option<Self::State> {
        &app.minimax_login
    }
    fn state_mut(app: &mut AppModel) -> &mut Option<Self::State> {
        &mut app.minimax_login
    }
    fn handle_mut(app: &mut AppModel) -> &mut Option<Handle> {
        &mut app.minimax_login_handle
    }
    fn is_running(state: &Self::State) -> bool {
        state.status == MinimaxLoginStatus::Editing
    }
    fn log_id(state: &Self::State) -> &str {
        &state.account_id
    }
    fn status_debug(state: &Self::State) -> String {
        format!("{:?}", state.status)
    }
    fn account_exists(config: &Config, account_id: &str) -> bool {
        config
            .minimax_managed_accounts
            .iter()
            .any(|a| a.id == account_id)
    }
    fn failed_state(error: String) -> Self::State {
        MinimaxLoginState {
            account_id: "failed".to_string(),
            label: String::new(),
            status: MinimaxLoginStatus::Failed,
            api_key: String::new(),
            error: Some(error),
        }
    }
    fn prepare(config: Config) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        let _ = config;
        Ok((minimax::prepare_login(), cosmic::iced::Task::none()))
    }
    fn wrap_event(event: Self::Event) -> Message {
        Message::LoginEvent(
            ProviderId::Minimax,
            Box::new(LoginEventKind::Minimax(event)),
        )
    }
    fn on_event(app: &mut AppModel, event: Self::Event) -> Task<Message> {
        match event {
            MinimaxLoginEvent::Started => Task::none(),
            MinimaxLoginEvent::ApiKeyChanged(api_key) => {
                if let Some(login) = app.minimax_login.as_mut() {
                    login.update_api_key(api_key);
                }
                Task::none()
            }
            MinimaxLoginEvent::LabelChanged(label) => {
                if let Some(login) = app.minimax_login.as_mut() {
                    login.update_label(label);
                }
                Task::none()
            }
            MinimaxLoginEvent::Saved => {
                let Some(login) = app.minimax_login.as_ref() else {
                    return Task::none();
                };
                match login.save(&mut app.config) {
                    Ok(managed_account) => {
                        app.minimax_login_handle = None;
                        let account_id = managed_account.id.clone();
                        let account_label = managed_account.label.clone();
                        app.write_config(|new_config| {
                            minimax::account::apply_login_account(new_config, managed_account);
                        });
                        let mut account = ProviderAccountRuntimeState::empty(
                            ProviderId::Minimax,
                            account_id.clone(),
                            account_label,
                        );
                        account.auth_state = crate::model::AuthState::Ready;
                        account.error = None;
                        app.state.upsert_account(account);
                        if let Some(provider) = app.state.provider_mut(ProviderId::Minimax) {
                            provider.account_status = AccountSelectionStatus::Ready;
                            provider.error = None;
                            if !provider.selected_account_ids.contains(&account_id) {
                                provider.selected_account_ids.push(account_id.clone());
                            }
                        }
                        app.persist_runtime_if_owner("minimax_account_saved");
                        let login = app.minimax_login.as_mut().unwrap();
                        login.status = MinimaxLoginStatus::Saved;
                        let process = app.refresh_task_process();
                        refresh_provider_task_for_process(
                            &app.config,
                            &mut app.state,
                            ProviderId::Minimax,
                            Some(process),
                            false,
                        )
                    }
                    Err(error) => {
                        if let Some(login) = app.minimax_login.as_mut() {
                            login.error = Some(error);
                            login.status = MinimaxLoginStatus::Failed;
                        }
                        Task::none()
                    }
                }
            }
            MinimaxLoginEvent::Cancelled => {
                cancel_login::<MinimaxLoginFlow>(app);
                Task::none()
            }
            MinimaxLoginEvent::Failed(error) => {
                if let Some(login) = app.minimax_login.as_mut() {
                    login.error = Some(error);
                    login.status = MinimaxLoginStatus::Failed;
                }
                Task::none()
            }
        }
    }
}
