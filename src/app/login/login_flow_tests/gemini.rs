use super::support::{gemini_account, isolated_xdg, test_app};
use crate::app::login::{GeminiLoginFlow, LoginFlow};
use crate::providers::gemini::{GeminiLoginEvent, GeminiLoginState, GeminiLoginStatus};

#[test]
fn gemini_on_event_finished_ok_applies_account_and_succeeds() {
    let (_env, _root) = isolated_xdg("gemini-finished-ok");
    let mut app = test_app();
    app.gemini_login = Some(GeminiLoginState {
        flow_id: "flow".to_string(),
        status: GeminiLoginStatus::Running,
        login_url: None,
        output: Vec::new(),
        error: None,
    });

    let _ = GeminiLoginFlow::on_event(
        &mut app,
        GeminiLoginEvent::Finished {
            flow_id: "flow".to_string(),
            result: Box::new(Ok(crate::providers::gemini::GeminiLoginSuccess {
                account: gemini_account("new-gemini"),
            })),
        },
    );

    let login = app.gemini_login.as_ref().unwrap();
    assert_eq!(login.status, GeminiLoginStatus::Succeeded);
    assert!(
        app.config
            .gemini_managed_accounts
            .iter()
            .any(|account| account.id == "new-gemini")
    );
}
