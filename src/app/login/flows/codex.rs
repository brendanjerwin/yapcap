use super::super::{LoginEventKind, LoginFlow, apply_login_success, log_login_failed};
use crate::app::{
    AppModel, CodexLoginEvent, CodexLoginState, CodexLoginStatus, Config, Handle, Message,
    ProviderId, Task, codex,
};

pub(crate) struct CodexLoginFlow;

impl LoginFlow for CodexLoginFlow {
    type State = CodexLoginState;
    type Event = CodexLoginEvent;
    const PROVIDER: ProviderId = ProviderId::Codex;

    fn state(app: &AppModel) -> &Option<Self::State> {
        &app.codex_login
    }
    fn state_mut(app: &mut AppModel) -> &mut Option<Self::State> {
        &mut app.codex_login
    }
    fn handle_mut(app: &mut AppModel) -> &mut Option<Handle> {
        &mut app.codex_login_handle
    }
    fn is_running(state: &Self::State) -> bool {
        state.status == CodexLoginStatus::Running
    }
    fn log_id(state: &Self::State) -> &str {
        &state.flow_id
    }
    fn status_debug(state: &Self::State) -> String {
        format!("{:?}", state.status)
    }
    fn account_exists(config: &Config, account_id: &str) -> bool {
        config
            .codex_managed_accounts
            .iter()
            .any(|a| a.id == account_id)
    }
    fn failed_state(error: String) -> Self::State {
        CodexLoginState {
            flow_id: "failed".to_string(),
            status: CodexLoginStatus::Failed,
            login_url: None,
            output: Vec::new(),
            error: Some(error),
        }
    }
    fn prepare(config: Config) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        codex::prepare(config)
    }
    fn wrap_event(event: Self::Event) -> Message {
        Message::LoginEvent(ProviderId::Codex, Box::new(LoginEventKind::Codex(event)))
    }
    fn on_event(app: &mut AppModel, event: Self::Event) -> Task<Message> {
        match event {
            CodexLoginEvent::Output {
                flow_id,
                line,
                login_url,
            } => {
                let Some(login) = app.codex_login.as_mut() else {
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
                let Some(login) = app.codex_login.as_ref() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                app.codex_login_handle = None;
                match *result {
                    Ok(success) => {
                        let account_id = success.account.id.clone();
                        app.codex_login = None;
                        apply_login_success(
                            app,
                            ProviderId::Codex,
                            &flow_id,
                            account_id,
                            move |cfg| codex::apply_login_account(cfg, success.account),
                        )
                    }
                    Err(error) => {
                        if let Some(login) = app.codex_login.as_mut() {
                            login.status = CodexLoginStatus::Failed;
                            login.error = Some(error.clone());
                        }
                        log_login_failed(&app.process_info.id, ProviderId::Codex, &flow_id, &error);
                        Task::none()
                    }
                }
            }
        }
    }
}
