use super::support::{copilot_account, isolated_xdg, test_app};
use crate::app::login::{CopilotLoginFlow, LoginFlow};
use crate::providers::copilot::{CopilotLoginEvent, CopilotLoginState, CopilotLoginStatus};

#[test]
fn copilot_on_event_code_populates_device_code_fields() {
    let mut app = test_app();
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

    let _ = CopilotLoginFlow::on_event(
        &mut app,
        CopilotLoginEvent::Code {
            flow_id: "flow".to_string(),
            user_code: "ABCD-1234".to_string(),
            verification_uri: "https://github.com/login/device".to_string(),
        },
    );

    let login = app.copilot_login.as_ref().unwrap();
    assert_eq!(login.user_code.as_deref(), Some("ABCD-1234"));
    assert_eq!(
        login.verification_uri.as_deref(),
        Some("https://github.com/login/device")
    );
}

#[test]
fn copilot_on_event_finished_ok_applies_account_and_succeeds() {
    let (_env, _root) = isolated_xdg("copilot-finished-ok");
    let mut app = test_app();
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

    let _ = CopilotLoginFlow::on_event(
        &mut app,
        CopilotLoginEvent::Finished {
            flow_id: "flow".to_string(),
            result: Box::new(Ok(crate::providers::copilot::CopilotLoginSuccess {
                account: copilot_account("new-copilot", "octocat"),
            })),
        },
    );

    let login = app.copilot_login.as_ref().unwrap();
    assert_eq!(login.status, CopilotLoginStatus::Succeeded);
    assert!(
        app.config
            .copilot_managed_accounts
            .iter()
            .any(|account| account.id == "new-copilot")
    );
}
