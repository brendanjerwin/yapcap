use super::support::{codex_account, isolated_xdg, test_app};
use crate::app::login::{CodexLoginFlow, LoginFlow};
use crate::providers::codex::{CodexLoginEvent, CodexLoginState, CodexLoginStatus};

#[test]
fn codex_on_event_output_buffers_lines_and_captures_login_url() {
    let mut app = test_app();
    app.codex_login = Some(CodexLoginState {
        flow_id: "flow".to_string(),
        status: CodexLoginStatus::Running,
        login_url: None,
        output: (0..8).map(|i| format!("line-{i}")).collect(),
        error: None,
    });

    let _ = CodexLoginFlow::on_event(
        &mut app,
        CodexLoginEvent::Output {
            flow_id: "flow".to_string(),
            line: "line-8".to_string(),
            login_url: Some("https://example.com/device".to_string()),
        },
    );

    let login = app.codex_login.as_ref().unwrap();
    assert_eq!(login.output.len(), 8);
    assert_eq!(login.output.last().unwrap(), "line-8");
    assert_eq!(login.output.first().unwrap(), "line-1");
    assert_eq!(
        login.login_url.as_deref(),
        Some("https://example.com/device")
    );
}

#[test]
fn codex_on_event_finished_err_marks_login_failed() {
    let mut app = test_app();
    app.codex_login = Some(CodexLoginState {
        flow_id: "flow".to_string(),
        status: CodexLoginStatus::Running,
        login_url: None,
        output: Vec::new(),
        error: None,
    });

    let _ = CodexLoginFlow::on_event(
        &mut app,
        CodexLoginEvent::Finished {
            flow_id: "flow".to_string(),
            result: Box::new(Err("network unreachable".to_string())),
        },
    );

    let login = app.codex_login.as_ref().unwrap();
    assert_eq!(login.status, CodexLoginStatus::Failed);
    assert_eq!(login.error.as_deref(), Some("network unreachable"));
}

#[test]
fn codex_on_event_finished_ok_applies_account_and_succeeds() {
    let (_env, _root) = isolated_xdg("codex-finished-ok");
    let mut app = test_app();
    app.codex_login = Some(CodexLoginState {
        flow_id: "flow".to_string(),
        status: CodexLoginStatus::Running,
        login_url: None,
        output: Vec::new(),
        error: None,
    });

    let _ = CodexLoginFlow::on_event(
        &mut app,
        CodexLoginEvent::Finished {
            flow_id: "flow".to_string(),
            result: Box::new(Ok(crate::providers::codex::CodexLoginSuccess {
                account: codex_account("new-codex"),
            })),
        },
    );

    assert!(app.codex_login.is_none());
    assert!(
        app.config
            .codex_managed_accounts
            .iter()
            .any(|account| account.id == "new-codex")
    );
}
