use super::super::{LoginEventKind, LoginFlow, apply_login_success, log_login_failed};
use crate::app::{
    AntigravityLoginEvent, AntigravityLoginState, AntigravityLoginStatus, AppModel, Config, Handle,
    Message, ProviderId, Task, antigravity,
};

pub(crate) struct AntigravityLoginFlow;

impl LoginFlow for AntigravityLoginFlow {
    type State = AntigravityLoginState;
    type Event = AntigravityLoginEvent;
    const PROVIDER: ProviderId = ProviderId::Antigravity;

    fn state(app: &AppModel) -> &Option<Self::State> {
        &app.antigravity_login
    }
    fn state_mut(app: &mut AppModel) -> &mut Option<Self::State> {
        &mut app.antigravity_login
    }
    fn handle_mut(app: &mut AppModel) -> &mut Option<Handle> {
        &mut app.antigravity_login_handle
    }
    fn is_running(state: &Self::State) -> bool {
        state.status == AntigravityLoginStatus::Running
    }
    fn log_id(state: &Self::State) -> &str {
        &state.flow_id
    }
    fn status_debug(state: &Self::State) -> String {
        format!("{:?}", state.status)
    }
    fn account_exists(config: &Config, account_id: &str) -> bool {
        config
            .antigravity_managed_accounts
            .iter()
            .any(|a| a.id == account_id)
    }
    fn failed_state(error: String) -> Self::State {
        AntigravityLoginState {
            flow_id: "failed".to_string(),
            status: AntigravityLoginStatus::Failed,
            login_url: None,
            output: Vec::new(),
            error: Some(error),
        }
    }
    fn prepare(config: Config) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        antigravity::prepare(config)
    }
    fn prepare_for_reauth(
        config: Config,
        account_id: &str,
    ) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        antigravity::prepare_for_reauth(config, account_id)
    }
    fn wrap_event(event: Self::Event) -> Message {
        Message::LoginEvent(
            ProviderId::Antigravity,
            Box::new(LoginEventKind::Antigravity(event)),
        )
    }
    fn on_event(app: &mut AppModel, event: Self::Event) -> Task<Message> {
        match event {
            AntigravityLoginEvent::Output {
                flow_id,
                line,
                login_url,
            } => {
                let Some(login) = app.antigravity_login.as_mut() else {
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
            AntigravityLoginEvent::Finished { flow_id, result } => {
                let Some(login) = app.antigravity_login.as_ref() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                app.antigravity_login_handle = None;
                match *result {
                    Ok(success) => {
                        let account_id = success.account.id.clone();
                        app.antigravity_login = None;
                        apply_login_success(
                            app,
                            ProviderId::Antigravity,
                            &flow_id,
                            account_id,
                            move |cfg| antigravity::apply_login_account(cfg, success.account),
                        )
                    }
                    Err(error) => {
                        if let Some(login) = app.antigravity_login.as_mut() {
                            login.status = AntigravityLoginStatus::Failed;
                            login.error = Some(error.clone());
                        }
                        log_login_failed(
                            &app.process_info.id,
                            ProviderId::Antigravity,
                            &flow_id,
                            &error,
                        );
                        Task::none()
                    }
                }
            }
        }
    }
}
