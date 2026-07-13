use super::support::{isolated_xdg, test_app};
use crate::app::login::{LoginFlow, MinimaxLoginFlow};
use crate::model::ProviderId;
use crate::providers::minimax::{MinimaxLoginEvent, MinimaxLoginState, MinimaxLoginStatus};

#[test]
fn minimax_on_event_api_key_and_label_changes_update_state() {
    let mut app = test_app();
    app.minimax_login = Some(MinimaxLoginState::new("draft".to_string()));

    let _ = MinimaxLoginFlow::on_event(
        &mut app,
        MinimaxLoginEvent::ApiKeyChanged("sk-test".to_string()),
    );
    let _ = MinimaxLoginFlow::on_event(
        &mut app,
        MinimaxLoginEvent::LabelChanged("Work".to_string()),
    );

    let login = app.minimax_login.as_ref().unwrap();
    assert_eq!(login.api_key, "sk-test");
    assert_eq!(login.label, "Work");
}

#[test]
fn minimax_on_event_saved_persists_account_and_seeds_runtime_state() {
    let (_env, _root) = isolated_xdg("minimax-saved");
    let mut app = test_app();
    app.minimax_login = Some(MinimaxLoginState::new("minimax-1".to_string()));
    let _ = MinimaxLoginFlow::on_event(
        &mut app,
        MinimaxLoginEvent::ApiKeyChanged("sk-test".to_string()),
    );

    let _ = MinimaxLoginFlow::on_event(&mut app, MinimaxLoginEvent::Saved);

    let login = app.minimax_login.as_ref().unwrap();
    assert_eq!(login.status, MinimaxLoginStatus::Saved);
    assert!(
        app.config
            .minimax_managed_accounts
            .iter()
            .any(|account| account.id == "minimax-1")
    );
    assert!(
        app.state
            .accounts_for(ProviderId::Minimax)
            .into_iter()
            .any(|account| account.account_id == "minimax-1")
    );
}

#[test]
fn minimax_on_event_saved_without_api_key_fails() {
    let mut app = test_app();
    app.minimax_login = Some(MinimaxLoginState::new("minimax-2".to_string()));

    let _ = MinimaxLoginFlow::on_event(&mut app, MinimaxLoginEvent::Saved);

    let login = app.minimax_login.as_ref().unwrap();
    assert_eq!(login.status, MinimaxLoginStatus::Failed);
    assert!(login.error.is_some());
}
