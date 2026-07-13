use super::super::{LoginEventKind, LoginFlow, apply_login_success, log_login_failed};
use crate::app::{
    AppModel, Config, GeminiLoginEvent, GeminiLoginState, GeminiLoginStatus, Handle, Message,
    ProviderId, Task, gemini,
};

pub(crate) struct GeminiLoginFlow;

impl LoginFlow for GeminiLoginFlow {
    type State = GeminiLoginState;
    type Event = GeminiLoginEvent;
    const PROVIDER: ProviderId = ProviderId::Gemini;

    fn state(app: &AppModel) -> &Option<Self::State> {
        &app.gemini_login
    }
    fn state_mut(app: &mut AppModel) -> &mut Option<Self::State> {
        &mut app.gemini_login
    }
    fn handle_mut(app: &mut AppModel) -> &mut Option<Handle> {
        &mut app.gemini_login_handle
    }
    fn is_running(state: &Self::State) -> bool {
        state.status == GeminiLoginStatus::Running
    }
    fn log_id(state: &Self::State) -> &str {
        &state.flow_id
    }
    fn status_debug(state: &Self::State) -> String {
        format!("{:?}", state.status)
    }
    fn account_exists(config: &Config, account_id: &str) -> bool {
        config
            .gemini_managed_accounts
            .iter()
            .any(|a| a.id == account_id)
    }
    fn failed_state(error: String) -> Self::State {
        GeminiLoginState {
            flow_id: "failed".to_string(),
            status: GeminiLoginStatus::Failed,
            login_url: None,
            output: Vec::new(),
            error: Some(error),
        }
    }
    fn prepare(config: Config) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        gemini::prepare(config)
    }
    fn prepare_for_reauth(
        config: Config,
        account_id: &str,
    ) -> Result<(Self::State, cosmic::iced::Task<Self::Event>), String> {
        gemini::prepare_for_reauth(config, account_id)
    }
    fn wrap_event(event: Self::Event) -> Message {
        Message::LoginEvent(ProviderId::Gemini, Box::new(LoginEventKind::Gemini(event)))
    }
    fn on_event(app: &mut AppModel, event: Self::Event) -> Task<Message> {
        match event {
            GeminiLoginEvent::Output {
                flow_id,
                line,
                login_url,
            } => {
                let Some(login) = app.gemini_login.as_mut() else {
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
            GeminiLoginEvent::Finished { flow_id, result } => {
                let Some(login) = app.gemini_login.as_ref() else {
                    return Task::none();
                };
                if login.flow_id != flow_id {
                    return Task::none();
                }
                app.gemini_login_handle = None;
                match *result {
                    Ok(success) => {
                        let account_id = success.account.id.clone();
                        if let Some(login) = app.gemini_login.as_mut() {
                            login.status = GeminiLoginStatus::Succeeded;
                            login.error = None;
                        }
                        apply_login_success(
                            app,
                            ProviderId::Gemini,
                            &flow_id,
                            account_id,
                            move |cfg| gemini::apply_login_account(cfg, success.account),
                        )
                    }
                    Err(error) => {
                        if let Some(login) = app.gemini_login.as_mut() {
                            login.status = GeminiLoginStatus::Failed;
                            login.error = Some(error.clone());
                        }
                        log_login_failed(
                            &app.process_info.id,
                            ProviderId::Gemini,
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
