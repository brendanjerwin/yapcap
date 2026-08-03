use super::support::test_app;
use crate::app::login::{
    ClaudeLoginFlow, CodexLoginFlow, CopilotLoginFlow, GeminiLoginFlow, MinimaxLoginFlow,
    cancel_login, reauthenticate, start_login,
};
use crate::providers::claude::{ClaudeLoginState, ClaudeLoginStatus};
use crate::providers::codex::{CodexLoginState, CodexLoginStatus};
use crate::providers::copilot::{CopilotLoginState, CopilotLoginStatus};
use crate::providers::gemini::{GeminiLoginState, GeminiLoginStatus};
use crate::providers::minimax::MinimaxLoginState;

#[test]
fn start_login_is_noop_when_already_running_for_every_login_flow_provider() {
    let mut app = test_app();
    app.codex_login = Some(CodexLoginState {
        flow_id: "running".to_string(),
        status: CodexLoginStatus::Running,
        login_url: None,
        output: Vec::new(),
        error: None,
    });
    let _ = start_login::<CodexLoginFlow>(&mut app);
    assert_eq!(app.codex_login.as_ref().unwrap().flow_id, "running");

    app.claude_login = Some(ClaudeLoginState {
        flow_id: "running".to_string(),
        status: ClaudeLoginStatus::Running,
        login_url: None,
        code_input: String::new(),
        output: Vec::new(),
        error: None,
        redirect_uri: String::new(),
        code_verifier: String::new(),
        state_token: String::new(),
        target_account_id: None,
    });
    let _ = start_login::<ClaudeLoginFlow>(&mut app);
    assert_eq!(app.claude_login.as_ref().unwrap().flow_id, "running");

    app.gemini_login = Some(GeminiLoginState {
        flow_id: "running".to_string(),
        status: GeminiLoginStatus::Running,
        login_url: None,
        output: Vec::new(),
        error: None,
    });
    let _ = start_login::<GeminiLoginFlow>(&mut app);
    assert_eq!(app.gemini_login.as_ref().unwrap().flow_id, "running");

    app.copilot_login = Some(CopilotLoginState {
        flow_id: "running".to_string(),
        status: CopilotLoginStatus::Running,
        user_code: None,
        verification_uri: None,
        output: Vec::new(),
        error: None,
        code_copied: false,
        expected_github_user_id: None,
    });
    let _ = start_login::<CopilotLoginFlow>(&mut app);
    assert_eq!(app.copilot_login.as_ref().unwrap().flow_id, "running");

    app.minimax_login = Some(MinimaxLoginState::new("editing".to_string()));
    let _ = start_login::<MinimaxLoginFlow>(&mut app);
    assert_eq!(app.minimax_login.as_ref().unwrap().account_id, "editing");
}

#[test]
fn cancel_login_clears_state_for_every_login_flow_provider() {
    let mut app = test_app();
    app.codex_login = Some(CodexLoginState {
        flow_id: "flow".to_string(),
        status: CodexLoginStatus::Running,
        login_url: None,
        output: Vec::new(),
        error: None,
    });
    cancel_login::<CodexLoginFlow>(&mut app);
    assert!(app.codex_login.is_none());

    app.gemini_login = Some(GeminiLoginState {
        flow_id: "flow".to_string(),
        status: GeminiLoginStatus::Running,
        login_url: None,
        output: Vec::new(),
        error: None,
    });
    cancel_login::<GeminiLoginFlow>(&mut app);
    assert!(app.gemini_login.is_none());

    app.copilot_login = Some(CopilotLoginState {
        flow_id: "flow".to_string(),
        status: CopilotLoginStatus::Running,
        user_code: None,
        verification_uri: None,
        output: Vec::new(),
        error: None,
        code_copied: false,
        expected_github_user_id: None,
    });
    cancel_login::<CopilotLoginFlow>(&mut app);
    assert!(app.copilot_login.is_none());

    app.minimax_login = Some(MinimaxLoginState::new("flow".to_string()));
    cancel_login::<MinimaxLoginFlow>(&mut app);
    assert!(app.minimax_login.is_none());

    // Cancelling with no active login is a safe no-op, not a panic.
    cancel_login::<ClaudeLoginFlow>(&mut app);
    assert!(app.claude_login.is_none());
}

#[test]
fn reauthenticate_is_noop_when_account_is_unknown() {
    let mut app = test_app();
    let _ = reauthenticate::<CodexLoginFlow>(&mut app, "does-not-exist");
    assert!(app.codex_login.is_none());

    let _ = reauthenticate::<ClaudeLoginFlow>(&mut app, "does-not-exist");
    assert!(app.claude_login.is_none());
}
