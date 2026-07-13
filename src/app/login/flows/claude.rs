use super::super::{LoginEventKind, LoginFlow, apply_login_success, log_login_failed};
use crate::app::{
    AppModel, ClaudeLoginEvent, ClaudeLoginState, ClaudeLoginStatus, Config, Handle, Message,
    ProviderId, Task, claude,
};

pub(crate) struct ClaudeLoginFlow;

impl LoginFlow for ClaudeLoginFlow {
    type State = ClaudeLoginState;
    type Event = ClaudeLoginEvent;
    const PROVIDER: ProviderId = ProviderId::Claude;

    fn state(app: &AppModel) -> &Option<Self::State> {
        &app.claude_login
    }
    fn state_mut(app: &mut AppModel) -> &mut Option<Self::State> {
        &mut app.claude_login
    }
    fn handle_mut(app: &mut AppModel) -> &mut Option<Handle> {
        &mut app.claude_login_handle
    }
    fn is_running(state: &Self::State) -> bool {
        state.status == ClaudeLoginStatus::Running
    }
    fn log_id(state: &Self::State) -> &str {
        &state.flow_id
    }
    fn status_debug(state: &Self::State) -> String {
        format!("{:?}", state.status)
    }
    fn account_exists(config: &Config, account_id: &str) -> bool {
        config
            .claude_managed_accounts
            .iter()
            .any(|a| a.id == account_id)
    }
    fn failed_state(error: String) -> Self::State {
        ClaudeLoginState {
            flow_id: "failed".to_string(),
            status: ClaudeLoginStatus::Failed,
            login_url: None,
            code_input: String::new(),
            output: Vec::new(),
            error: Some(error),
            redirect_uri: String::new(),
            code_verifier: String::new(),
            state_token: String::new(),
            target_account_id: None,
        }
    }
    fn prepare(config: Config) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        let _ = config;
        Ok((claude::prepare(), cosmic::iced::Task::none()))
    }
    fn prepare_for_reauth(
        config: Config,
        account_id: &str,
    ) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        let _ = config;
        Ok((
            claude::prepare_targeted(account_id.to_string()),
            cosmic::iced::Task::none(),
        ))
    }
    fn wrap_event(event: Self::Event) -> Message {
        Message::LoginEvent(ProviderId::Claude, Box::new(LoginEventKind::Claude(event)))
    }
    fn on_event(app: &mut AppModel, event: Self::Event) -> Task<Message> {
        match event {
            ClaudeLoginEvent::Finished { flow_id, result } => {
                let Some(login) = app.claude_login.as_ref() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                app.claude_login_handle = None;
                match *result {
                    Ok(success) => {
                        let account_id = success.account.id.clone();
                        if let Some(login) = app.claude_login.as_mut() {
                            login.status = ClaudeLoginStatus::Succeeded;
                            login.error = None;
                        }
                        apply_login_success(
                            app,
                            ProviderId::Claude,
                            &flow_id,
                            account_id,
                            move |cfg| claude::apply_login_account(cfg, success.account),
                        )
                    }
                    Err(error) => {
                        if let Some(login) = app.claude_login.as_mut() {
                            login.status = ClaudeLoginStatus::Failed;
                            login.error = Some(error.clone());
                        }
                        log_login_failed(
                            &app.process_info.id,
                            ProviderId::Claude,
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
