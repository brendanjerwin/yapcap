use super::super::LoginFlow;
use super::super::flows::ClaudeLoginFlow;
use crate::app::{AppModel, ClaudeLoginStatus, Message, ProviderId, Task, claude};

impl AppModel {
    pub(in crate::app) fn update_claude_login_code(&mut self, code: String) {
        if let Some(login) = self.claude_login.as_mut()
            && login.status == ClaudeLoginStatus::Running
        {
            login.code_input = code;
        }
    }

    pub(in crate::app) fn submit_claude_login_code(&mut self) -> Task<Message> {
        let Some(login) = self.claude_login.as_mut() else {
            return Task::none();
        };
        if login.status != ClaudeLoginStatus::Running || login.code_input.trim().is_empty() {
            return Task::none();
        }
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Claude.label(),
            flow_id = %login.flow_id,
            "login code submitted"
        );
        login.error = None;
        login.output.push("Completing Claude sign-in".to_string());
        if login.output.len() > 8 {
            login.output.remove(0);
        }
        let task = claude::submit_code(login, self.config.clone());
        let task = task.map(|event| cosmic::Action::App(ClaudeLoginFlow::wrap_event(event)));
        let (task, handle) = task.abortable();
        self.claude_login_handle = Some(handle);
        task
    }

    pub(in crate::app) fn copy_copilot_login_code(&mut self, code: String) -> Task<Message> {
        let Some(login) = self.copilot_login.as_mut() else {
            return Task::none();
        };
        login.code_copied = true;
        tracing::info!(
            process_id = %self.process_info.id,
            provider = ProviderId::Copilot.label(),
            flow_id = %login.flow_id,
            "copilot login code copied"
        );
        let flow_id = login.flow_id.clone();
        let write: Task<Message> = cosmic::iced::clipboard::write(code);
        let clear: Task<Message> = Task::perform(
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                flow_id
            },
            |flow_id| cosmic::Action::App(Message::ClearCopilotLoginCodeCopied(flow_id)),
        );
        write.chain(clear)
    }

    pub(in crate::app) fn clear_copilot_login_code_copied(&mut self, flow_id: &str) {
        if let Some(login) = self.copilot_login.as_mut()
            && login.flow_id == flow_id
        {
            login.code_copied = false;
        }
    }
}
