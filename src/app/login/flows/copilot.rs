use super::super::{LoginEventKind, LoginFlow, apply_login_success, log_login_failed};
use crate::app::{
    AppModel, Config, CopilotLoginEvent, CopilotLoginState, CopilotLoginStatus, Handle, Message,
    ProviderId, Task, copilot,
};

pub(crate) struct CopilotLoginFlow;

impl LoginFlow for CopilotLoginFlow {
    type State = CopilotLoginState;
    type Event = CopilotLoginEvent;
    const PROVIDER: ProviderId = ProviderId::Copilot;

    fn state(app: &AppModel) -> &Option<Self::State> {
        &app.copilot_login
    }
    fn state_mut(app: &mut AppModel) -> &mut Option<Self::State> {
        &mut app.copilot_login
    }
    fn handle_mut(app: &mut AppModel) -> &mut Option<Handle> {
        &mut app.copilot_login_handle
    }
    fn is_running(state: &Self::State) -> bool {
        state.status == CopilotLoginStatus::Running
    }
    fn log_id(state: &Self::State) -> &str {
        &state.flow_id
    }
    fn status_debug(state: &Self::State) -> String {
        format!("{:?}", state.status)
    }
    fn account_exists(config: &Config, account_id: &str) -> bool {
        config
            .copilot_managed_accounts
            .iter()
            .any(|a| a.id == account_id)
    }
    fn failed_state(error: String) -> Self::State {
        CopilotLoginState {
            flow_id: "failed".to_string(),
            status: CopilotLoginStatus::Failed,
            user_code: None,
            verification_uri: None,
            output: Vec::new(),
            error: Some(error),
            code_copied: false,
            expected_github_user_id: None,
        }
    }
    fn prepare(config: Config) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        copilot::prepare(config)
    }
    fn prepare_for_reauth(
        config: Config,
        account_id: &str,
    ) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        copilot::prepare_for_reauth(config, account_id)
    }
    fn wrap_event(event: Self::Event) -> Message {
        Message::LoginEvent(
            ProviderId::Copilot,
            Box::new(LoginEventKind::Copilot(event)),
        )
    }
    fn on_event(app: &mut AppModel, event: Self::Event) -> Task<Message> {
        match event {
            CopilotLoginEvent::Code {
                flow_id,
                user_code,
                verification_uri,
            } => {
                let Some(login) = app.copilot_login.as_mut() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                login.user_code = Some(user_code);
                login.verification_uri = Some(verification_uri);
                tracing::info!(
                    process_id = %app.process_info.id,
                    provider = ProviderId::Copilot.label(),
                    flow_id = %flow_id,
                    verification_uri_available = true,
                    user_code_available = true,
                    "login device code received"
                );
                Task::none()
            }
            CopilotLoginEvent::Output { flow_id, line } => {
                let Some(login) = app.copilot_login.as_mut() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                login.output.push(line);
                if login.output.len() > 8 {
                    login.output.remove(0);
                }
                Task::none()
            }
            CopilotLoginEvent::Finished { flow_id, result } => {
                let Some(login) = app.copilot_login.as_ref() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                app.copilot_login_handle = None;
                match *result {
                    Ok(success) => {
                        let account_id = success.account.id.clone();
                        app.copilot_login = None;
                        apply_login_success(
                            app,
                            ProviderId::Copilot,
                            &flow_id,
                            account_id,
                            move |cfg| copilot::apply_login_account(cfg, success.account),
                        )
                    }
                    Err(error) => {
                        if let Some(login) = app.copilot_login.as_mut() {
                            login.status = CopilotLoginStatus::Failed;
                            login.error = Some(error.clone());
                        }
                        log_login_failed(
                            &app.process_info.id,
                            ProviderId::Copilot,
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
