use super::applet::{
    AppletBarLayout, applet_bar_layout, applet_bar_width, applet_button_size,
    applet_fallback_button_size, applet_percent_cell_alignment, applet_percent_cell_width,
    applet_percent_text, panel_fallback_active, select_provider, selected_provider_all_bar_layouts,
};
use super::popup_view::{POPUP_COLUMN_WIDTH, popup_session_size, popup_settings_size};
use super::refresh::should_refresh_account_statuses;
use super::{
    APPLET_ACCOUNT_GAP, APPLET_ICON_GAP, APPLET_PERCENT_ACCOUNT_GAP, AppModel, AppState, Config,
    LaunchMode, Message, PanelIconStyle, PopupBodyMeasurements, PopupRoute, ProviderId, Size,
    UsageAmountFormat, automatic_refresh_poll_interval, format_retry_delay,
    popup_size_limits_with_max_width, popup_size_tuple, update_retry_delay,
};
use crate::account_storage::{NewProviderAccount, ProviderAccountStorage, ProviderAccountTokens};
use crate::config::{
    ManagedClaudeAccountConfig, ManagedCodexAccountConfig, ManagedCopilotAccountConfig,
    ManagedCursorAccountConfig, ManagedGeminiAccountConfig, ManagedMinimaxAccountConfig,
};
use crate::model::{
    AccountSelectionStatus, ExtraUsageState, ProviderAccountRuntimeState, ProviderCost,
    ProviderIdentity, ProviderRuntimeState, UsageHeadline, UsageSnapshot, UsageWindow,
};
use crate::providers::cursor::CursorScanState;
use crate::refresh_owner::{ProcessInfo, RefreshOwner, RefreshOwnerAttempt};
use crate::shared_state::{
    ProviderRefreshRequest, RefreshRequestReason, SharedControlState, SharedRuntimeState,
};
use crate::updates::UpdateStatus;
use chrono::Utc;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn popup_limits_allow_wider_max() {
    let limits = popup_size_limits_with_max_width(Size::new(420.0, 640.0), 840.0);

    assert_eq!(limits.min().width, 1.0);
    assert_eq!(limits.max().width, 840.0);
    assert_eq!(limits.min().height, 640.0);
    assert_eq!(limits.max().height, 640.0);
}

#[test]
fn popup_size_tuple_rounds_logical_size() {
    assert_eq!(popup_size_tuple(Size::new(419.6, 640.2)), (420, 640));
}

#[test]
fn update_retry_delay_backs_off_to_cap() {
    assert_eq!(update_retry_delay(1), Duration::from_secs(15));
    assert_eq!(update_retry_delay(2), Duration::from_secs(30));
    assert_eq!(update_retry_delay(7), Duration::from_secs(15 * 60));
    assert_eq!(update_retry_delay(20), Duration::from_secs(15 * 60));
}

#[test]
fn retry_delay_format_is_compact() {
    assert_eq!(format_retry_delay(Duration::from_secs(15)), "15s");
    assert_eq!(format_retry_delay(Duration::from_secs(60)), "1m");
    assert_eq!(format_retry_delay(Duration::from_secs(75)), "1m 15s");
}

#[test]
fn automatic_refresh_poll_checks_more_often_than_default_refresh_interval() {
    assert_eq!(automatic_refresh_poll_interval(), Duration::from_secs(10));
}

#[test]
fn owner_tick_runs_automatic_refresh() {
    let owner = refresh_owner("owner-tick");
    let mut app = test_app(Some(owner));
    ready_selected_provider(&mut app.state, ProviderId::Codex);

    let _task = app.handle_message(Message::Tick);

    assert!(app.state.provider(ProviderId::Codex).unwrap().is_refreshing);
}

#[test]
fn non_owner_tick_does_not_run_automatic_refresh() {
    let mut app = test_app(None);
    ready_selected_provider(&mut app.state, ProviderId::Codex);

    let _task = app.handle_message(Message::Tick);

    assert!(!app.state.provider(ProviderId::Codex).unwrap().is_refreshing);
}

#[test]
fn owner_tick_skips_disabled_provider() {
    let owner = refresh_owner("owner-disabled");
    let mut app = test_app(Some(owner));
    app.config.cursor_enablement = crate::config::ProviderEnablement::Disabled;
    app.state.provider_mut(ProviderId::Cursor).unwrap().enabled = false;
    ready_selected_provider(&mut app.state, ProviderId::Cursor);

    let _task = app.handle_message(Message::Tick);

    let cursor = app.state.provider(ProviderId::Cursor).unwrap();
    assert!(!cursor.is_refreshing);
}

#[test]
fn non_owner_refresh_now_writes_shared_control_requests_without_refreshing() {
    let mut app = test_app(None);
    ready_selected_provider(&mut app.state, ProviderId::Codex);

    let task = app.handle_message(Message::RefreshNow);

    assert_eq!(task.units(), 0);
    assert_eq!(app.shared_control.requests.len(), ProviderId::ALL.len());
    assert!(
        app.shared_control
            .requests
            .iter()
            .all(|request| request.reason == RefreshRequestReason::User)
    );
    assert!(!app.state.provider(ProviderId::Codex).unwrap().is_refreshing);
}

#[test]
fn refresh_now_excludes_disabled_providers_from_requests() {
    let mut app = test_app(None);
    app.config.claude_enablement = crate::config::ProviderEnablement::Disabled;
    app.state.provider_mut(ProviderId::Claude).unwrap().enabled = false;

    let _task = app.handle_message(Message::RefreshNow);

    assert!(
        !app.shared_control
            .requests
            .iter()
            .any(|request| request.provider == ProviderId::Claude)
    );
}

#[test]
fn owner_observing_shared_control_runs_requested_refresh() {
    let owner = refresh_owner("owner-request");
    let mut app = test_app(Some(owner));
    ready_selected_provider(&mut app.state, ProviderId::Codex);

    let task = app.handle_message(Message::UpdateSharedControl(
        Box::new(control_request(ProviderId::Codex)),
        vec!["requests"],
    ));

    assert!(task.units() > 0);
    assert!(app.state.provider(ProviderId::Codex).unwrap().is_refreshing);
}

#[test]
fn owner_ignores_duplicate_request_for_refreshing_provider() {
    let owner = refresh_owner("owner-duplicate");
    let mut app = test_app(Some(owner));
    ready_selected_provider(&mut app.state, ProviderId::Codex);
    app.state
        .provider_mut(ProviderId::Codex)
        .unwrap()
        .is_refreshing = true;

    let task = app.handle_message(Message::UpdateSharedControl(
        Box::new(control_request(ProviderId::Codex)),
        vec!["requests"],
    ));

    assert_eq!(task.units(), 0);
    assert!(app.state.provider(ProviderId::Codex).unwrap().is_refreshing);
}

#[test]
fn owner_consumes_request_for_not_ready_provider() {
    let _env = crate::test_support::test_env();
    let owner = refresh_owner("owner-not-ready-request");
    let mut app = test_app(Some(owner));

    let task = app.handle_message(Message::UpdateSharedControl(
        Box::new(control_request(ProviderId::Codex)),
        vec!["requests"],
    ));

    assert_eq!(task.units(), 0);
    assert!(app.shared_control.requests.is_empty());
    assert!(!app.state.provider(ProviderId::Codex).unwrap().is_refreshing);
}

#[test]
fn owner_ignores_stale_shared_control_update() {
    let owner = refresh_owner("owner-stale-control");
    let mut app = test_app(Some(owner));
    ready_selected_provider(&mut app.state, ProviderId::Cursor);
    app.shared_control.generation = 2;

    let stale = control_request(ProviderId::Cursor);
    let task = app.handle_message(Message::UpdateSharedControl(
        Box::new(stale),
        vec!["requests"],
    ));

    assert_eq!(task.units(), 0);
    assert_eq!(app.shared_control.generation, 2);
    assert!(app.shared_control.requests.is_empty());
    assert!(
        !app.state
            .provider(ProviderId::Cursor)
            .unwrap()
            .is_refreshing
    );
}

#[test]
fn owner_ignores_same_generation_conflicting_shared_control_update() {
    let owner = refresh_owner("owner-same-generation-control");
    let mut app = test_app(Some(owner));
    ready_selected_provider(&mut app.state, ProviderId::Cursor);
    app.shared_control.generation = 1;

    let same_generation = control_request(ProviderId::Cursor);
    let task = app.handle_message(Message::UpdateSharedControl(
        Box::new(same_generation),
        vec!["requests"],
    ));

    assert_eq!(task.units(), 0);
    assert_eq!(app.shared_control.generation, 1);
    assert!(app.shared_control.requests.is_empty());
    assert!(
        !app.state
            .provider(ProviderId::Cursor)
            .unwrap()
            .is_refreshing
    );
}

#[test]
fn shared_control_metadata_notification_does_not_apply_partial_document() {
    let owner = refresh_owner("owner-partial-control");
    let mut app = test_app(Some(owner));
    let partial = control_request(ProviderId::Codex);

    let task = app.handle_message(Message::UpdateSharedControl(
        Box::new(partial),
        vec!["generation"],
    ));

    assert_eq!(task.units(), 0);
    assert_eq!(app.shared_control.generation, 0);
    assert!(app.shared_control.requests.is_empty());
}

#[test]
fn shared_refresh_evaluation_lists_unique_requesters() {
    let mut control = SharedControlState::default();
    control.upsert_request(refresh_request(ProviderId::Codex, "process-a"));
    control.upsert_request(refresh_request(ProviderId::Claude, "process-b"));
    control.upsert_request(refresh_request(ProviderId::Cursor, "process-a"));

    let evaluation = super::SharedRefreshEvaluationLog::from_requests(&control);

    assert_eq!(evaluation.requesters(), "process-a,process-b");
}

#[test]
fn shared_runtime_metadata_notification_does_not_apply_partial_document() {
    let mut app = test_app(None);
    let initial_state = app.state.clone();
    let mut partial_state = initial_state.clone();
    partial_state.provider_mut(ProviderId::Codex).unwrap().error = Some("partial".to_string());

    let task = app.handle_message(Message::UpdateSharedRuntime(
        Box::new(SharedRuntimeState::new(partial_state, 1)),
        vec!["generation"],
    ));

    assert_eq!(task.units(), 0);
    assert_eq!(app.state, initial_state);
}

#[test]
fn shared_runtime_update_preserves_refreshing_provider() {
    let mut app = test_app(None);
    app.config.codex_enablement = crate::config::ProviderEnablement::Enabled;
    let mut shared_state = app.state.clone();
    shared_state
        .provider_mut(ProviderId::Codex)
        .unwrap()
        .is_refreshing = true;

    let task = app.handle_message(Message::UpdateSharedRuntime(
        Box::new(SharedRuntimeState::new(shared_state, 1)),
        Vec::new(),
    ));

    assert_eq!(task.units(), 0);
    assert!(app.state.provider(ProviderId::Codex).unwrap().is_refreshing);
}

#[test]
fn owner_refresh_now_processes_new_shared_control_snapshot() {
    let owner = refresh_owner("owner-refresh-now-new-control");
    let mut app = test_app(Some(owner));
    ready_selected_provider(&mut app.state, ProviderId::Cursor);

    let task = app.handle_message(Message::RefreshNow);

    assert!(task.units() > 0);
    assert!(
        app.state
            .provider(ProviderId::Cursor)
            .unwrap()
            .is_refreshing
    );
}

#[test]
fn provider_refresh_completion_consumes_shared_control_request() {
    let owner = refresh_owner("owner-consume-request");
    let mut app = test_app(Some(owner));
    app.shared_control = control_request(ProviderId::Codex);
    let provider = ProviderRuntimeState {
        provider: ProviderId::Codex,
        enabled: true,
        selected_account_ids: vec!["default".to_string()],
        active_account_id: Some("default".to_string()),
        system_active_account_id: None,
        account_status: AccountSelectionStatus::Ready,
        is_refreshing: false,
        refresh_started_at: None,
        legacy_display_snapshot: None,
        error: None,
    };

    let _task = app.handle_message(Message::ProviderRefreshed(Box::new(
        crate::runtime::ProviderRefreshResult {
            provider,
            accounts: Vec::new(),
        },
    )));

    assert!(app.shared_control.requests.is_empty());
}

#[test]
fn config_update_applies_selected_provider_without_changing_popup_route() {
    let mut app = test_app(None);
    app.popup = Some(cosmic::iced::window::Id::unique());
    app.popup_route = PopupRoute::Settings(super::SettingsRoute::General);
    let mut config = app.config.clone();
    config.selected_provider = ProviderId::Claude;
    config.claude_enablement = crate::config::ProviderEnablement::Enabled;

    let _task = app.handle_message(Message::UpdateConfig(
        Box::new(config),
        vec!["selected_provider", "claude_enablement"],
    ));

    assert_eq!(app.selected_provider, ProviderId::Claude);
    assert_eq!(
        app.popup_route,
        PopupRoute::Settings(super::SettingsRoute::General)
    );
    assert!(app.popup.is_some());
}

#[test]
fn partial_config_update_preserves_locally_written_account() {
    let mut app = test_app(None);
    app.config.codex_managed_accounts = vec![codex_account("codex-1")];
    app.config.selected_codex_account_ids = vec!["codex-1".to_string()];
    let partial_watcher_config = Config {
        selected_codex_account_ids: vec!["codex-1".to_string()],
        ..Config::default()
    };

    app.on_config_update(partial_watcher_config, &["selected_codex_account_ids"]);

    assert_eq!(app.config.codex_managed_accounts.len(), 1);
}

#[test]
fn quit_requests_runtime_exit() {
    let mut app = test_app(None);

    let task = app.handle_message(Message::Quit);

    assert_eq!(task.units(), 1);
}

#[test]
fn selecting_stale_enabled_provider_writes_provider_selected_request() {
    let _env = crate::test_support::test_env();
    let mut app = test_app(None);
    ready_selected_provider(&mut app.state, ProviderId::Claude);
    selected_account_without_usage(&mut app.state, ProviderId::Claude);

    let task = app.handle_message(Message::SelectProvider(ProviderId::Claude));

    assert_eq!(task.units(), 0);
    assert_eq!(app.config.selected_provider, ProviderId::Claude);
    assert_eq!(app.selected_provider, ProviderId::Claude);
    assert_eq!(app.shared_control.requests.len(), 1);
    let request = &app.shared_control.requests[0];
    assert_eq!(request.provider, ProviderId::Claude);
    assert_eq!(request.reason, RefreshRequestReason::ProviderSelected);
}

#[test]
fn selecting_disabled_provider_does_not_request_refresh() {
    let mut app = test_app(None);
    app.config.cursor_enablement = crate::config::ProviderEnablement::Disabled;
    if let Some(cursor) = app.state.provider_mut(ProviderId::Cursor) {
        cursor.enabled = false;
    }

    let _task = app.handle_message(Message::SelectProvider(ProviderId::Cursor));

    assert!(app.shared_control.requests.is_empty());
}

#[test]
fn selecting_provider_preserves_local_popup_state() {
    let mut app = test_app(None);
    let popup = cosmic::iced::window::Id::unique();
    app.popup = Some(popup);
    app.popup_route = PopupRoute::Settings(super::SettingsRoute::Provider(ProviderId::Codex));
    ready_selected_provider(&mut app.state, ProviderId::Gemini);
    selected_account_without_usage(&mut app.state, ProviderId::Gemini);

    let _task = app.handle_message(Message::SelectProvider(ProviderId::Gemini));

    assert_eq!(app.popup, Some(popup));
    assert_eq!(
        app.popup_route,
        PopupRoute::Settings(super::SettingsRoute::Provider(ProviderId::Codex))
    );
}

#[test]
fn non_owner_account_selection_requests_owner_refresh_without_running_it() {
    let _env = crate::test_support::test_env();
    let mut app = test_app(None);
    app.config.copilot_managed_accounts = vec![copilot_account("copilot-1", "octocat")];
    runtime_reconcile_provider(&app.config, &mut app.state, ProviderId::Copilot);

    let task = app.handle_message(Message::ToggleAccountSelection(
        ProviderId::Copilot,
        "copilot-1".to_string(),
    ));

    assert_eq!(task.units(), 0);
    assert_eq!(
        app.config.selected_account_ids(ProviderId::Copilot),
        ["copilot-1"]
    );
    assert_eq!(app.shared_control.requests.len(), 1);
    assert_eq!(
        app.shared_control.requests[0].reason,
        RefreshRequestReason::AccountAction
    );
    assert!(
        !app.state
            .provider(ProviderId::Copilot)
            .unwrap()
            .is_refreshing
    );
}

#[test]
fn owner_account_selection_runs_requested_refresh() {
    let owner = refresh_owner("owner-account-selection");
    let mut app = test_app(Some(owner));
    app.config.copilot_managed_accounts = vec![copilot_account("copilot-1", "octocat")];
    runtime_reconcile_provider(&app.config, &mut app.state, ProviderId::Copilot);

    let task = app.handle_message(Message::ToggleAccountSelection(
        ProviderId::Copilot,
        "copilot-1".to_string(),
    ));

    assert!(task.units() > 0);
    assert!(
        app.state
            .provider(ProviderId::Copilot)
            .unwrap()
            .is_refreshing
    );
}

#[test]
fn non_owner_provider_disable_reconciles_locally_without_publishing_runtime() {
    let _env = crate::test_support::test_env();
    let mut app = test_app(None);
    app.config.copilot_managed_accounts = vec![copilot_account("copilot-1", "octocat")];
    app.config.selected_copilot_account_ids = vec!["copilot-1".to_string()];
    runtime_reconcile_provider(&app.config, &mut app.state, ProviderId::Copilot);

    let task = app.handle_message(Message::SetProviderEnabled(ProviderId::Copilot, false));

    assert_eq!(task.units(), 0);
    assert_eq!(
        app.config.provider_enablement(ProviderId::Copilot),
        crate::config::ProviderEnablement::Disabled
    );
    assert!(!app.state.provider(ProviderId::Copilot).unwrap().enabled);
    assert!(app.shared_control.requests.is_empty());
}

#[test]
fn select_provider_keeps_current_when_enabled() {
    let mut state = AppState::empty();
    for p in &mut state.providers {
        p.enabled = true;
    }
    assert_eq!(
        select_provider(ProviderId::Claude, &state),
        ProviderId::Claude
    );
}

#[test]
fn select_provider_falls_back_when_current_disabled() {
    let mut state = AppState::empty();
    for p in &mut state.providers {
        p.enabled = p.provider != ProviderId::Codex;
    }
    let selected = select_provider(ProviderId::Codex, &state);
    assert_ne!(selected, ProviderId::Codex);
}

#[test]
fn applet_button_size_uses_panel_icon_style() {
    let core = cosmic::Core::default();
    let (suggested_w, suggested_h) = core.applet.suggested_size(false);
    let (major_padding, minor_padding) = core.applet.suggested_padding(false);
    let horizontal_padding = if core.applet.is_horizontal() {
        major_padding
    } else {
        minor_padding
    };
    let compact_px = suggested_w.min(suggested_h);
    let logo_width = f32::from(compact_px.saturating_sub(8).max(11));
    let bar_width = applet_bar_width(suggested_w, suggested_h);
    let padding_width = f32::from(2 * horizontal_padding);
    let (logo_bars_width, height) = applet_button_size(&core, PanelIconStyle::LogoAndBars, 1);
    let (bars_only_width, bars_only_height) =
        applet_button_size(&core, PanelIconStyle::BarsOnly, 1);
    let (percent_width, percent_height) =
        applet_button_size(&core, PanelIconStyle::LogoAndPercent, 1);
    let (percent_only_width, percent_only_height) =
        applet_button_size(&core, PanelIconStyle::PercentOnly, 1);

    assert_eq!(bars_only_width, bar_width + padding_width);
    let cell_100 = applet_percent_cell_width();
    assert_eq!(percent_only_width, cell_100 + padding_width);
    assert_eq!(
        logo_bars_width,
        logo_width + APPLET_ICON_GAP + bar_width + padding_width
    );
    assert_eq!(
        percent_width,
        logo_width + APPLET_ICON_GAP + cell_100 + padding_width
    );
    assert_eq!(height, bars_only_height);
    assert_eq!(height, percent_height);
    assert_eq!(height, percent_only_height);
}

#[test]
fn applet_button_size_ignores_percent_primaries_for_bar_styles() {
    let core = cosmic::Core::default();
    let (a, _) = applet_button_size(&core, PanelIconStyle::BarsOnly, 2);
    let (b, _) = applet_button_size(&core, PanelIconStyle::BarsOnly, 2);
    assert_eq!(a, b);
    let (c, _) = applet_button_size(&core, PanelIconStyle::LogoAndBars, 2);
    let (d, _) = applet_button_size(&core, PanelIconStyle::LogoAndBars, 2);
    assert_eq!(c, d);
}

#[test]
fn applet_button_size_scales_with_account_count() {
    let core = cosmic::Core::default();
    let (w1, _) = applet_button_size(&core, PanelIconStyle::BarsOnly, 1);
    let (w2, _) = applet_button_size(&core, PanelIconStyle::BarsOnly, 2);
    let (w3, _) = applet_button_size(&core, PanelIconStyle::BarsOnly, 3);
    let (suggested_w, suggested_h) = core.applet.suggested_size(false);
    let bar_width = applet_bar_width(suggested_w, suggested_h);
    assert_eq!(w2 - w1, bar_width + APPLET_ACCOUNT_GAP);
    assert_eq!(w3 - w2, bar_width + APPLET_ACCOUNT_GAP);
    let (lw2, _) = applet_button_size(&core, PanelIconStyle::LogoAndBars, 2);
    let (lw1, _) = applet_button_size(&core, PanelIconStyle::LogoAndBars, 1);
    assert_eq!(lw2 - lw1, bar_width + APPLET_ACCOUNT_GAP);
}

#[test]
fn applet_button_size_percent_uses_fixed_slot_width() {
    let core = cosmic::Core::default();
    let cell = applet_percent_cell_width();
    let (w1, _) = applet_button_size(&core, PanelIconStyle::PercentOnly, 1);
    let (w2, _) = applet_button_size(&core, PanelIconStyle::PercentOnly, 2);
    let (w3, _) = applet_button_size(&core, PanelIconStyle::PercentOnly, 3);
    assert_eq!(w2 - w1, cell + APPLET_PERCENT_ACCOUNT_GAP);
    assert_eq!(w3 - w2, cell + APPLET_PERCENT_ACCOUNT_GAP);
}

#[test]
fn applet_button_size_logo_and_percent_uses_fixed_slot_width() {
    let core = cosmic::Core::default();
    let (percent_only, _) = applet_button_size(&core, PanelIconStyle::PercentOnly, 2);
    let (logo_percent, _) = applet_button_size(&core, PanelIconStyle::LogoAndPercent, 2);
    let (suggested_w, suggested_h) = core.applet.suggested_size(false);
    let logo_width = f32::from(suggested_w.min(suggested_h).saturating_sub(8).max(11));

    assert_eq!(logo_percent - percent_only, logo_width + APPLET_ICON_GAP);
}

#[test]
fn applet_button_size_percent_styles_ignore_current_percent_digits() {
    let core = cosmic::Core::default();
    let short_state = state_with_selected_account_percents(&[0.0, 8.5]);
    let wide_state = state_with_selected_account_percents(&[86.5, 100.0]);
    let short_n =
        selected_provider_all_bar_layouts(&short_state, ProviderId::Codex, UsageAmountFormat::Used)
            .len();
    let wide_n =
        selected_provider_all_bar_layouts(&wide_state, ProviderId::Codex, UsageAmountFormat::Used)
            .len();

    assert_eq!(short_n, 2);
    assert_eq!(wide_n, 2);
    assert_eq!(
        applet_button_size(&core, PanelIconStyle::PercentOnly, short_n),
        applet_button_size(&core, PanelIconStyle::PercentOnly, wide_n)
    );
    assert_eq!(
        applet_button_size(&core, PanelIconStyle::LogoAndPercent, short_n),
        applet_button_size(&core, PanelIconStyle::LogoAndPercent, wide_n)
    );
}

#[test]
fn panel_fallback_is_active_when_no_provider_has_accounts() {
    let state = AppState::empty();

    assert!(panel_fallback_active(&state));
}

#[test]
fn panel_fallback_clears_when_an_enabled_provider_has_an_account() {
    let mut state = AppState::empty();
    state.upsert_account(ProviderAccountRuntimeState::empty(
        ProviderId::Codex,
        "codex-1",
        "Codex",
    ));

    assert!(!panel_fallback_active(&state));
}

#[test]
fn panel_fallback_stays_active_when_only_disabled_providers_have_accounts() {
    let mut state = AppState::empty();
    state.upsert_provider(ProviderRuntimeState::disabled(ProviderId::Codex));
    state.upsert_account(ProviderAccountRuntimeState::empty(
        ProviderId::Codex,
        "codex-1",
        "Codex",
    ));

    assert!(panel_fallback_active(&state));
}

#[test]
fn panel_fallback_stays_active_when_all_providers_are_disabled() {
    let mut state = AppState::empty();
    for provider in &mut state.providers {
        provider.enabled = false;
    }

    assert!(panel_fallback_active(&state));
}

#[test]
fn applet_fallback_button_size_is_icon_only() {
    let core = cosmic::Core::default();
    let (suggested_w, suggested_h) = core.applet.suggested_size(false);
    let (major_padding, minor_padding) = core.applet.suggested_padding(false);
    let (horizontal_padding, vertical_padding) = if core.applet.is_horizontal() {
        (major_padding, minor_padding)
    } else {
        (minor_padding, major_padding)
    };
    let icon_px = suggested_w.min(suggested_h);

    let (width, height) = applet_fallback_button_size(&core);

    assert_eq!(
        width,
        f32::from(icon_px) + f32::from(2 * horizontal_padding)
    );
    assert_eq!(height, f32::from(suggested_h + 2 * vertical_padding));
    let (bars_width, bars_height) = applet_button_size(&core, PanelIconStyle::LogoAndBars, 1);
    assert!(width < bars_width);
    assert_eq!(height, bars_height);
}

#[test]
fn applet_percent_groups_are_capped_to_four_selected_accounts() {
    let state = state_with_selected_account_percents(&[1.0, 2.0, 3.0, 4.0, 5.0]);

    let percents =
        selected_provider_all_bar_layouts(&state, ProviderId::Codex, UsageAmountFormat::Used);

    assert_eq!(percents.len(), 4);
    assert_eq!(percents.last().map(|layout| layout.primary), Some(4.0));
}

#[test]
fn popup_session_width_is_capped_to_four_selected_accounts() {
    let state = state_with_selected_account_percents(&[1.0, 2.0, 3.0, 4.0, 5.0]);

    let size = popup_session_size(&state, ProviderId::Codex);

    assert_eq!(size.width, POPUP_COLUMN_WIDTH * 4.0);
}

#[test]
fn popup_provider_tabs_share_tallest_provider_height() {
    let state = state_with_provider_window_counts(&[
        (ProviderId::Codex, 1, false),
        (ProviderId::Claude, 3, true),
        (ProviderId::Cursor, 2, false),
    ]);

    let codex = popup_session_size(&state, ProviderId::Codex);
    let claude = popup_session_size(&state, ProviderId::Claude);
    let cursor = popup_session_size(&state, ProviderId::Cursor);

    assert_eq!(codex.height, claude.height);
    assert_eq!(claude.height, cursor.height);
}

#[test]
fn popup_grows_taller_when_provider_tabs_wrap_to_second_row() {
    let mut state = state_with_provider_window_counts(&[(ProviderId::Codex, 1, false)]);
    for provider in ProviderId::ALL {
        state.provider_mut(provider).unwrap().enabled = false;
    }
    for provider in [
        ProviderId::Codex,
        ProviderId::Claude,
        ProviderId::Cursor,
        ProviderId::Gemini,
    ] {
        state.provider_mut(provider).unwrap().enabled = true;
    }

    let four_enabled = popup_session_size(&state, ProviderId::Codex);

    state.provider_mut(ProviderId::Minimax).unwrap().enabled = true;
    let five_enabled = popup_session_size(&state, ProviderId::Codex);

    assert!(five_enabled.height > four_enabled.height);

    for provider in [ProviderId::Claude, ProviderId::Cursor, ProviderId::Gemini] {
        state.provider_mut(provider).unwrap().enabled = false;
    }
    state.provider_mut(ProviderId::Minimax).unwrap().enabled = false;
    let one_enabled = popup_session_size(&state, ProviderId::Codex);

    assert_eq!(one_enabled.height, four_enabled.height);
}

#[test]
fn popup_provider_height_is_independent_from_settings_height() {
    let mut state = state_with_provider_window_counts(&[
        (ProviderId::Codex, 1, false),
        (ProviderId::Claude, 1, false),
        (ProviderId::Cursor, 1, false),
    ]);
    for provider in ProviderId::ALL {
        for i in 1..8 {
            state.upsert_account(ProviderAccountRuntimeState::empty(
                provider,
                format!("{provider:?}-{i}"),
                provider.label(),
            ));
        }
    }

    let provider = popup_session_size(&state, ProviderId::Codex);
    let settings = popup_settings_size(&state);

    assert!(settings.height > provider.height);
    assert_eq!(provider.width, POPUP_COLUMN_WIDTH);
}

#[test]
fn applet_percent_cell_width_is_fixed_to_widest_normal_percent() {
    let expected =
        super::APPLET_PERCENT_CELL_HORIZONTAL_PAD + 6.0 * super::APPLET_PERCENT_GLYPH_WIDTH;

    assert_eq!(applet_percent_text(0.0), "0.0%");
    assert_eq!(applet_percent_text(86.5), "86.5%");
    assert_eq!(applet_percent_text(100.0), "100.0%");
    assert_eq!(applet_percent_cell_width(), expected);
}

#[test]
fn applet_percent_cells_left_align_text_in_fixed_slot() {
    assert_eq!(
        applet_percent_cell_alignment(),
        cosmic::iced::Alignment::Start
    );
}

#[test]
fn applet_percent_text_uses_one_decimal_through_100_percent() {
    assert_eq!(applet_percent_text(86.54), "86.5%");
    assert_eq!(applet_percent_text(100.0), "100.0%");
}

#[test]
fn selected_provider_all_percents_uses_first_panel_window() {
    let mut state = AppState::empty();
    let mut account = ProviderAccountRuntimeState::empty(ProviderId::Codex, "codex-1", "Codex");
    account.snapshot = Some(UsageSnapshot {
        provider: ProviderId::Codex,
        source: "test".to_string(),
        updated_at: Utc::now(),
        headline: UsageHeadline(0),
        windows: vec![
            UsageWindow {
                label: "Session".to_string(),
                used_percent: 86.5,
                reset_at: None,
                window_seconds: None,
                reset_description: None,
                group: None,
            },
            UsageWindow {
                label: "Weekly".to_string(),
                used_percent: 42.0,
                reset_at: None,
                window_seconds: None,
                reset_description: None,
                group: None,
            },
        ],
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity::default(),
    });

    state
        .provider_mut(ProviderId::Codex)
        .unwrap()
        .selected_account_ids = vec!["codex-1".to_string()];
    state.upsert_account(account);

    let percents_used =
        selected_provider_all_bar_layouts(&state, ProviderId::Codex, UsageAmountFormat::Used);
    assert_eq!(
        percents_used.first().map(|layout| layout.primary),
        Some(86.5)
    );
    assert_eq!(
        percents_used.first().and_then(|layout| layout.secondary),
        Some(42.0)
    );

    let percents_left =
        selected_provider_all_bar_layouts(&state, ProviderId::Codex, UsageAmountFormat::Left);
    assert_eq!(
        percents_left.first().map(|layout| layout.primary),
        Some(13.5)
    );
}

#[test]
fn applet_bar_layout_preserves_single_bar_shape() {
    let snapshot = UsageSnapshot {
        provider: ProviderId::Copilot,
        source: "test".to_string(),
        updated_at: Utc::now(),
        headline: UsageHeadline(0),
        windows: vec![UsageWindow {
            label: "premium_interactions".to_string(),
            used_percent: 37.5,
            reset_at: None,
            window_seconds: None,
            reset_description: None,
            group: None,
        }],
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity::default(),
    };

    let layout = applet_bar_layout(
        snapshot.applet_windows(),
        snapshot.updated_at,
        UsageAmountFormat::Used,
    );

    assert_eq!(layout, AppletBarLayout::single_bar(37.5));
}

#[test]
fn selected_provider_all_bar_layouts_keeps_mixed_copilot_account_shapes() {
    let mut state = AppState::empty();
    state
        .provider_mut(ProviderId::Copilot)
        .unwrap()
        .selected_account_ids = vec!["casey-free".to_string(), "morgan-pro".to_string()];

    let mut free =
        ProviderAccountRuntimeState::empty(ProviderId::Copilot, "casey-free", "casey-free");
    free.snapshot = Some(snapshot_with_percents(ProviderId::Copilot, &[30.0, 80.0]));
    state.upsert_account(free);

    let mut paid =
        ProviderAccountRuntimeState::empty(ProviderId::Copilot, "morgan-pro", "morgan-pro");
    paid.snapshot = Some(snapshot_with_percents(ProviderId::Copilot, &[100.0]));
    state.upsert_account(paid);

    let layouts =
        selected_provider_all_bar_layouts(&state, ProviderId::Copilot, UsageAmountFormat::Used);

    assert_eq!(
        layouts,
        vec![
            AppletBarLayout::two_bar(30.0, 80.0),
            AppletBarLayout::single_bar(100.0)
        ]
    );
}

fn state_with_selected_account_percents(percents: &[f32]) -> AppState {
    let mut state = AppState::empty();
    let selected_account_ids = percents
        .iter()
        .enumerate()
        .map(|(i, _)| format!("codex-{i}"))
        .collect::<Vec<_>>();
    state
        .provider_mut(ProviderId::Codex)
        .unwrap()
        .selected_account_ids = selected_account_ids;

    for (i, percent) in percents.iter().enumerate() {
        let id = format!("codex-{i}");
        let mut account =
            ProviderAccountRuntimeState::empty(ProviderId::Codex, id.clone(), "Codex");
        account.snapshot = Some(UsageSnapshot {
            provider: ProviderId::Codex,
            source: "test".to_string(),
            updated_at: Utc::now(),
            headline: UsageHeadline(0),
            windows: vec![UsageWindow {
                label: "Session".to_string(),
                used_percent: *percent,
                reset_at: None,
                window_seconds: None,
                reset_description: None,
                group: None,
            }],
            provider_cost: None,
            extra_usage: None,
            identity: ProviderIdentity::default(),
        });
        state.upsert_account(account);
    }

    state
}

pub(super) fn test_app(refresh_owner: Option<RefreshOwner>) -> AppModel {
    let lock_path = std::env::temp_dir().join("yapcap-test-unused-owner.lock");
    AppModel {
        core: cosmic::Core::default(),
        popup: None,
        config: Config::default(),
        state: AppState::empty(),
        detection: crate::detection::DetectionSnapshot::default(),
        selected_provider: ProviderId::Codex,
        popup_route: PopupRoute::ProviderDetail,
        update_status: UpdateStatus::Unchecked,
        launch_mode: LaunchMode::Standalone,
        popup_size: None,
        popup_body_measurements: PopupBodyMeasurements::default(),
        shared_control: SharedControlState::default(),
        process_info: ProcessInfo {
            id: "test-process".to_string(),
            pid: std::process::id(),
            panel_output: None,
            flatpak_id: None,
            lock_path,
        },
        refresh_owner,
        codex_login: None,
        codex_login_handle: None,
        claude_login: None,
        claude_login_handle: None,
        cursor_scan: CursorScanState::Idle,
        cursor_scan_result: None,
        gemini_login: None,
        gemini_login_handle: None,
        copilot_login: None,
        copilot_login_handle: None,
        minimax_login: None,
        minimax_login_handle: None,
        antigravity_login: None,
        antigravity_login_handle: None,
    }
}

#[test]
fn test_app_starts_with_nothing_detected() {
    let app = test_app(None);
    for provider in ProviderId::ALL {
        assert!(!app.detection.detected(provider));
    }
}

fn ready_selected_provider(state: &mut AppState, provider: ProviderId) {
    let entry = state.provider_mut(provider).unwrap();
    entry.account_status = AccountSelectionStatus::Ready;
    entry.selected_account_ids = vec!["default".to_string()];
}

fn selected_account_without_usage(state: &mut AppState, provider: ProviderId) {
    let mut account = ProviderAccountRuntimeState::empty(provider, "default", provider.label());
    account.auth_state = crate::model::AuthState::Ready;
    state.upsert_account(account);
}

fn runtime_reconcile_provider(config: &Config, state: &mut AppState, provider: ProviderId) {
    crate::runtime::reconcile_provider(
        config,
        &crate::detection::DetectionSnapshot::default(),
        state,
        provider,
    );
}

fn control_request(provider: ProviderId) -> SharedControlState {
    let mut control = SharedControlState::default();
    control.upsert_request(refresh_request(provider, "test-process"));
    control
}

fn refresh_request(provider: ProviderId, process_id: &str) -> ProviderRefreshRequest {
    ProviderRefreshRequest {
        provider,
        reason: RefreshRequestReason::User,
        requested_at: Utc::now(),
        requesting_process_id: process_id.to_string(),
    }
}

fn copilot_account(id: &str, login: &str) -> ManagedCopilotAccountConfig {
    ManagedCopilotAccountConfig {
        id: id.to_string(),
        label: login.to_string(),
        github_user_id: 1,
        login: login.to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        last_authenticated_at: Some(Utc::now()),
    }
}

fn codex_account(id: &str) -> ManagedCodexAccountConfig {
    ManagedCodexAccountConfig {
        id: id.to_string(),
        label: id.to_string(),
        codex_home: PathBuf::from("/tmp/yapcap/codex-1"),
        email: Some("user@example.com".to_string()),
        provider_account_id: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        last_authenticated_at: None,
    }
}

fn claude_account(id: &str) -> ManagedClaudeAccountConfig {
    ManagedClaudeAccountConfig {
        id: id.to_string(),
        label: id.to_string(),
        config_dir: PathBuf::from("/tmp/yapcap/claude"),
        email: Some(format!("{id}@example.com")),
        organization: None,
        subscription_type: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        last_authenticated_at: None,
    }
}

fn cursor_account(id: &str, email: &str) -> ManagedCursorAccountConfig {
    ManagedCursorAccountConfig {
        id: id.to_string(),
        email: email.to_string(),
        label: email.to_string(),
        account_root: PathBuf::from("/tmp/yapcap/cursor"),
        display_name: None,
        plan: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        last_authenticated_at: None,
    }
}

fn gemini_account(id: &str) -> ManagedGeminiAccountConfig {
    ManagedGeminiAccountConfig {
        id: id.to_string(),
        label: id.to_string(),
        account_root: PathBuf::from("/tmp/yapcap/gemini"),
        email: format!("{id}@example.com"),
        sub: id.to_string(),
        hd: None,
        last_tier_id: None,
        last_cloudaicompanion_project: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        last_authenticated_at: None,
    }
}

fn minimax_account(id: &str) -> ManagedMinimaxAccountConfig {
    ManagedMinimaxAccountConfig {
        id: id.to_string(),
        label: id.to_string(),
        api_key_source: "env:MINIMAX_API_KEY".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        last_authenticated_at: None,
    }
}

fn antigravity_account(id: &str) -> crate::config::ManagedAntigravityAccountConfig {
    crate::config::ManagedAntigravityAccountConfig {
        id: id.to_string(),
        label: id.to_string(),
        account_root: PathBuf::from("/tmp/yapcap/antigravity"),
        email: format!("{id}@example.com"),
        sub: id.to_string(),
        last_tier_id: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        last_authenticated_at: None,
    }
}

#[test]
fn cursor_status_refresh_skipped_without_accounts() {
    let _env = crate::test_support::test_env();
    let config = Config::default();

    assert_eq!(
        config.cursor_enablement,
        crate::config::ProviderEnablement::Auto
    );
    assert!(!should_refresh_account_statuses(
        &AppState::empty(),
        ProviderId::Cursor
    ));
}

#[test]
fn cursor_status_refresh_runs_with_accounts() {
    let _env = crate::test_support::test_env();
    seed_account_storage(
        crate::config::paths().cursor_accounts_dir,
        ProviderId::Cursor,
        "one",
        "one@example.com",
    );
    let mut config = Config::default();
    config
        .cursor_managed_accounts
        .push(cursor_account("one", "one@example.com"));

    let mut state = AppState::empty();
    state.provider_mut(ProviderId::Cursor).unwrap().enabled = true;
    state.upsert_account(ProviderAccountRuntimeState::empty(
        ProviderId::Cursor,
        "cursor".to_string(),
        "Cursor".to_string(),
    ));
    assert!(should_refresh_account_statuses(&state, ProviderId::Cursor));
}

fn seed_account_storage(dir: PathBuf, provider: ProviderId, id: &str, email: &str) {
    let storage = ProviderAccountStorage::new(dir);
    storage
        .replace_account(
            id.to_string(),
            NewProviderAccount {
                provider,
                email: email.to_string(),
                provider_account_id: None,
                organization_id: None,
                organization_name: None,
                tokens: ProviderAccountTokens {
                    access_token: "access".to_string(),
                    refresh_token: "refresh".to_string(),
                    expires_at: Utc::now() + chrono::Duration::hours(1),
                    scope: Vec::new(),
                    token_id: None,
                },
                snapshot: None,
            },
        )
        .unwrap();
}

#[test]
fn delete_account_requests_refresh_for_all_providers() {
    for provider in ProviderId::ALL {
        let mut env = crate::test_support::test_env();
        let state_root = std::env::temp_dir().join(format!(
            "yapcap-delete-account-test-{provider:?}-{}",
            std::process::id()
        ));
        env.set("XDG_STATE_HOME", &state_root);
        env.set("XDG_CONFIG_HOME", &state_root);

        let mut app = test_app(None);
        let keep_id = "keep";
        let remove_account_id;

        match provider {
            ProviderId::Codex => {
                seed_account_storage(
                    crate::config::paths().codex_accounts_dir,
                    provider,
                    keep_id,
                    "keep@example.com",
                );
                app.config
                    .codex_managed_accounts
                    .push(codex_account(keep_id));
                app.config
                    .codex_managed_accounts
                    .push(codex_account("remove"));
                app.config.selected_codex_account_ids = vec![keep_id.to_string()];
                remove_account_id = "remove".to_string();
            }
            ProviderId::Claude => {
                app.config
                    .claude_managed_accounts
                    .push(claude_account(keep_id));
                app.config
                    .claude_managed_accounts
                    .push(claude_account("remove"));
                app.config.selected_claude_account_ids = vec![keep_id.to_string()];
                remove_account_id = "remove".to_string();
            }
            ProviderId::Cursor => {
                seed_account_storage(
                    crate::config::paths().cursor_accounts_dir,
                    provider,
                    keep_id,
                    "keep@example.com",
                );
                app.config
                    .cursor_managed_accounts
                    .push(cursor_account(keep_id, "keep@example.com"));
                app.config
                    .cursor_managed_accounts
                    .push(cursor_account("remove", "remove@example.com"));
                app.config.selected_cursor_account_ids = vec![format!("cursor-managed:{keep_id}")];
                remove_account_id = "cursor-managed:remove".to_string();
            }
            ProviderId::Gemini => {
                app.config
                    .gemini_managed_accounts
                    .push(gemini_account(keep_id));
                app.config
                    .gemini_managed_accounts
                    .push(gemini_account("remove"));
                app.config.selected_gemini_account_ids = vec![keep_id.to_string()];
                remove_account_id = "remove".to_string();
            }
            ProviderId::Copilot => {
                app.config
                    .copilot_managed_accounts
                    .push(copilot_account(keep_id, "keep"));
                app.config
                    .copilot_managed_accounts
                    .push(copilot_account("remove", "remove"));
                app.config.selected_copilot_account_ids = vec![keep_id.to_string()];
                remove_account_id = "remove".to_string();
            }
            ProviderId::Minimax => {
                app.config
                    .minimax_managed_accounts
                    .push(minimax_account(keep_id));
                app.config
                    .minimax_managed_accounts
                    .push(minimax_account("remove"));
                app.config.selected_minimax_account_ids = vec![keep_id.to_string()];
                remove_account_id = "remove".to_string();
            }
            ProviderId::Antigravity => {
                app.config
                    .antigravity_managed_accounts
                    .push(antigravity_account(keep_id));
                app.config
                    .antigravity_managed_accounts
                    .push(antigravity_account("remove"));
                app.config.selected_antigravity_account_ids = vec![keep_id.to_string()];
                remove_account_id = "remove".to_string();
            }
        }

        let _task = app.delete_account(provider, &remove_account_id);

        assert!(
            !crate::providers::registry::discover_accounts(provider, &app.config)
                .iter()
                .any(|account| account.account_id == remove_account_id),
            "{provider:?} should no longer discover the deleted account"
        );
        assert_eq!(
            app.state.provider(provider).unwrap().account_status,
            AccountSelectionStatus::Ready,
            "{provider:?} should remain ready with the kept account selected"
        );
        assert!(
            app.shared_control
                .requests
                .iter()
                .any(|request| request.provider == provider),
            "{provider:?} should request a refresh after delete"
        );
    }
}

fn refresh_owner(name: &str) -> RefreshOwner {
    let lock_path = std::env::temp_dir().join(format!(
        "yapcap-app-refresh-owner-{name}-{}.lock",
        std::process::id()
    ));
    match crate::refresh_owner::try_acquire(lock_path).unwrap() {
        RefreshOwnerAttempt::Owner(owner) => owner,
        RefreshOwnerAttempt::NonOwner(_) => panic!("test lock should be available"),
    }
}

fn state_with_provider_window_counts(providers: &[(ProviderId, usize, bool)]) -> AppState {
    let mut state = AppState::empty();
    for &(provider, window_count, with_extra_usage) in providers {
        let account_id = format!("{provider:?}-0");
        state.provider_mut(provider).unwrap().selected_account_ids = vec![account_id.clone()];
        let mut account =
            ProviderAccountRuntimeState::empty(provider, account_id, provider.label());
        account.snapshot = Some(snapshot_with_windows(
            provider,
            window_count,
            with_extra_usage,
        ));
        state.upsert_account(account);
    }
    state
}

fn snapshot_with_windows(
    provider: ProviderId,
    window_count: usize,
    with_extra_usage: bool,
) -> UsageSnapshot {
    UsageSnapshot {
        provider,
        source: "test".to_string(),
        updated_at: Utc::now(),
        headline: UsageHeadline(0),
        windows: (0..window_count)
            .map(|i| UsageWindow {
                label: format!("Window {i}"),
                used_percent: 10.0,
                reset_at: None,
                window_seconds: None,
                reset_description: None,
                group: None,
            })
            .collect(),
        provider_cost: None,
        extra_usage: with_extra_usage.then_some(ExtraUsageState::Active {
            used_percent: 25.0,
            cost: ProviderCost {
                used: 5.0,
                limit: Some(20.0),
                units: "USD".to_string(),
            },
        }),
        identity: ProviderIdentity::default(),
    }
}

fn snapshot_with_percents(provider: ProviderId, percents: &[f32]) -> UsageSnapshot {
    UsageSnapshot {
        provider,
        source: "test".to_string(),
        updated_at: Utc::now(),
        headline: UsageHeadline(0),
        windows: percents
            .iter()
            .enumerate()
            .map(|(i, percent)| UsageWindow {
                label: format!("Window {i}"),
                used_percent: *percent,
                reset_at: None,
                window_seconds: None,
                reset_description: None,
                group: None,
            })
            .collect(),
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity::default(),
    }
}
