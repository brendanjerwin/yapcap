use super::support::{antigravity_account, isolated_xdg, test_app};
use crate::app::login::{AntigravityLoginFlow, LoginFlow};
use crate::providers::antigravity::{
    AntigravityLoginEvent, AntigravityLoginState, AntigravityLoginStatus,
};

fn running_state(flow_id: &str) -> AntigravityLoginState {
    AntigravityLoginState {
        flow_id: flow_id.to_string(),
        status: AntigravityLoginStatus::Running,
        login_url: None,
        output: Vec::new(),
        error: None,
    }
}

#[test]
fn on_event_finished_ok_applies_account_and_succeeds() {
    let (_env, _root) = isolated_xdg("antigravity-finished-ok");
    let mut app = test_app();
    app.antigravity_login = Some(running_state("flow"));

    let _ = AntigravityLoginFlow::on_event(
        &mut app,
        AntigravityLoginEvent::Finished {
            flow_id: "flow".to_string(),
            result: Box::new(Ok(crate::providers::antigravity::AntigravityLoginSuccess {
                account: antigravity_account("new-antigravity"),
            })),
        },
    );

    let login = app.antigravity_login.as_ref().unwrap();
    assert_eq!(login.status, AntigravityLoginStatus::Succeeded);
    assert!(
        app.config
            .antigravity_managed_accounts
            .iter()
            .any(|account| account.id == "new-antigravity")
    );
    assert_eq!(
        app.config.selected_antigravity_account_ids,
        vec!["new-antigravity".to_string()]
    );
}

#[test]
fn on_event_finished_error_marks_failed_and_commits_nothing() {
    let (_env, _root) = isolated_xdg("antigravity-finished-err");
    let mut app = test_app();
    app.antigravity_login = Some(running_state("flow"));

    let _ = AntigravityLoginFlow::on_event(
        &mut app,
        AntigravityLoginEvent::Finished {
            flow_id: "flow".to_string(),
            result: Box::new(Err("boom".to_string())),
        },
    );

    let login = app.antigravity_login.as_ref().unwrap();
    assert_eq!(login.status, AntigravityLoginStatus::Failed);
    assert_eq!(login.error.as_deref(), Some("boom"));
    assert!(app.config.antigravity_managed_accounts.is_empty());
}

#[test]
fn on_event_ignores_mismatched_flow_id() {
    let (_env, _root) = isolated_xdg("antigravity-mismatch");
    let mut app = test_app();
    app.antigravity_login = Some(running_state("current"));

    let _ = AntigravityLoginFlow::on_event(
        &mut app,
        AntigravityLoginEvent::Finished {
            flow_id: "stale".to_string(),
            result: Box::new(Ok(crate::providers::antigravity::AntigravityLoginSuccess {
                account: antigravity_account("stale-account"),
            })),
        },
    );

    assert_eq!(
        app.antigravity_login.as_ref().unwrap().status,
        AntigravityLoginStatus::Running
    );
    assert!(app.config.antigravity_managed_accounts.is_empty());
}
