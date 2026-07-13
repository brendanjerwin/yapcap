use super::support::{claude_account, isolated_xdg, test_app};
use crate::app::login::{ClaudeLoginFlow, LoginFlow};
use crate::providers::claude::{ClaudeLoginEvent, ClaudeLoginState, ClaudeLoginStatus};

#[test]
fn claude_on_event_finished_ok_applies_account_and_succeeds() {
    let (_env, _root) = isolated_xdg("claude-finished-ok");
    let mut app = test_app();
    app.claude_login = Some(ClaudeLoginState {
        flow_id: "flow".to_string(),
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

    let _ = ClaudeLoginFlow::on_event(
        &mut app,
        ClaudeLoginEvent::Finished {
            flow_id: "flow".to_string(),
            result: Box::new(Ok(crate::providers::claude::ClaudeLoginSuccess {
                account: claude_account("new-claude"),
            })),
        },
    );

    let login = app.claude_login.as_ref().unwrap();
    assert_eq!(login.status, ClaudeLoginStatus::Succeeded);
    assert!(
        app.config
            .claude_managed_accounts
            .iter()
            .any(|account| account.id == "new-claude")
    );
}

#[test]
fn claude_on_event_finished_err_marks_login_failed() {
    let mut app = test_app();
    app.claude_login = Some(ClaudeLoginState {
        flow_id: "flow".to_string(),
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

    let _ = ClaudeLoginFlow::on_event(
        &mut app,
        ClaudeLoginEvent::Finished {
            flow_id: "flow".to_string(),
            result: Box::new(Err("invalid-code".to_string())),
        },
    );

    let login = app.claude_login.as_ref().unwrap();
    assert_eq!(login.status, ClaudeLoginStatus::Failed);
    assert_eq!(login.error.as_deref(), Some("invalid-code"));
}
