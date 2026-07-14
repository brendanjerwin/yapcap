mod login_controls;
mod rows;

use self::login_controls::{
    antigravity_login_controls, claude_login_controls, codex_login_controls,
    copilot_login_controls, cursor_scan_controls, gemini_login_controls, minimax_login_controls,
};
use self::rows::{account_selector_list, account_settings_row, show_all_accounts_row};
use super::super::{
    AppState, Config, Element, Length, Message, ProviderId, ProviderLoginStates, fl,
    settings_block, settings_block_enabled, widget,
};
use crate::providers::antigravity::AntigravityLoginState;
use crate::providers::claude::ClaudeLoginState;
use crate::providers::codex::CodexLoginState;
use crate::providers::copilot::CopilotLoginState;
use crate::providers::cursor::CursorScanState;
use crate::providers::gemini::GeminiLoginState;
use crate::providers::minimax::MinimaxLoginState;

pub(super) fn provider_settings_view<'a>(
    state: &'a AppState,
    config: &'a Config,
    logins: ProviderLoginStates<'a>,
    provider_id: ProviderId,
) -> Element<'a, Message> {
    let enabled = state
        .provider(provider_id)
        .is_some_and(|provider| provider.enabled);

    let enable_section = settings_block(
        widget::text(fl!("provider-enabled-title")).size(16).into(),
        widget::toggler(enabled)
            .width(Length::Fill)
            .on_toggle(move |enabled| Message::SetProviderEnabled(provider_id, enabled)),
    );

    let accounts_section = match provider_id {
        ProviderId::Codex => codex_accounts_section(state, config, logins.codex, enabled),
        ProviderId::Claude => claude_accounts_section(state, config, logins.claude, enabled),
        ProviderId::Cursor => cursor_accounts_section(state, config, logins.cursor_scan, enabled),
        ProviderId::Gemini => gemini_accounts_section(state, config, logins.gemini, enabled),
        ProviderId::Copilot => copilot_accounts_section(state, config, logins.copilot, enabled),
        ProviderId::Minimax => minimax_accounts_section(state, config, logins.minimax, enabled),
        ProviderId::Antigravity => {
            antigravity_accounts_section(state, config, logins.antigravity, enabled)
        }
    };

    Element::from(
        cosmic::iced::widget::column![enable_section, accounts_section]
            .spacing(14)
            .width(Length::Fill),
    )
}

fn codex_accounts_section<'a>(
    state: &'a AppState,
    config: &'a Config,
    codex_login: Option<&'a CodexLoginState>,
    enabled: bool,
) -> Element<'a, Message> {
    let codex = state.provider(ProviderId::Codex);
    let selected_ids: Vec<&str> = codex
        .map(|provider| {
            provider
                .selected_account_ids
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default();
    let accounts = state.accounts_for(ProviderId::Codex);
    let active_id = codex.and_then(|provider| provider.system_active_account_id.as_deref());
    let mut rows = cosmic::iced::widget::column![]
        .spacing(8)
        .width(Length::Fill);

    if accounts.is_empty() {
        rows = rows.push(widget::text(fl!("codex-accounts-empty")).size(13));
    } else {
        let mut account_rows = cosmic::iced::widget::column![]
            .spacing(6)
            .width(Length::Fill);
        for account in &accounts {
            account_rows = account_rows.push(account_settings_row(
                ProviderId::Codex,
                account,
                &selected_ids,
                active_id,
                config,
                enabled,
            ));
        }
        rows = rows.push(account_selector_list(account_rows));
    }

    if let Some(provider) = codex
        && provider.account_status == crate::model::AccountSelectionStatus::SelectionRequired
    {
        rows = rows.push(widget::text(fl!("codex-account-select-required")).size(13));
    }

    if accounts.len() > 1 {
        rows = rows.push(show_all_accounts_row(
            ProviderId::Codex,
            config.show_all_accounts(ProviderId::Codex),
            enabled,
        ));
    }

    rows = rows.push(codex_login_controls(codex_login, enabled));

    settings_block_enabled(
        widget::text(fl!("codex-accounts-title")).size(16).into(),
        rows,
        enabled,
    )
}

fn claude_accounts_section<'a>(
    state: &'a AppState,
    config: &'a Config,
    claude_login: Option<&'a ClaudeLoginState>,
    enabled: bool,
) -> Element<'a, Message> {
    let selected_ids: Vec<&str> = state
        .provider(ProviderId::Claude)
        .map(|provider| {
            provider
                .selected_account_ids
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default();
    let accounts = state.accounts_for(ProviderId::Claude);
    let active_id = state
        .provider(ProviderId::Claude)
        .and_then(|provider| provider.system_active_account_id.as_deref());
    let mut rows = cosmic::iced::widget::column![]
        .spacing(8)
        .width(Length::Fill);

    if accounts.is_empty() {
        rows = rows.push(widget::text(fl!("claude-accounts-empty")).size(13));
    } else {
        let mut account_rows = cosmic::iced::widget::column![]
            .spacing(6)
            .width(Length::Fill);
        for account in &accounts {
            account_rows = account_rows.push(account_settings_row(
                ProviderId::Claude,
                account,
                &selected_ids,
                active_id,
                config,
                enabled,
            ));
        }
        rows = rows.push(account_selector_list(account_rows));
    }

    if accounts.len() > 1 {
        rows = rows.push(show_all_accounts_row(
            ProviderId::Claude,
            config.show_all_accounts(ProviderId::Claude),
            enabled,
        ));
    }

    if let Some(login) = claude_login
        && login.status == crate::providers::claude::ClaudeLoginStatus::Running
    {
        rows = rows.push(widget::text(fl!("account-browser-login-hint")).size(12));
    }
    rows = rows.push(claude_login_controls(claude_login, enabled));

    settings_block_enabled(
        widget::text(fl!("claude-accounts-title")).size(16).into(),
        rows,
        enabled,
    )
}

fn gemini_accounts_section<'a>(
    state: &'a AppState,
    config: &'a Config,
    gemini_login: Option<&'a GeminiLoginState>,
    enabled: bool,
) -> Element<'a, Message> {
    let selected_ids: Vec<&str> = state
        .provider(ProviderId::Gemini)
        .map(|provider| {
            provider
                .selected_account_ids
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default();
    let accounts = state.accounts_for(ProviderId::Gemini);
    let active_id = state
        .provider(ProviderId::Gemini)
        .and_then(|provider| provider.system_active_account_id.as_deref());
    let mut rows = cosmic::iced::widget::column![]
        .spacing(8)
        .width(Length::Fill);

    if accounts.is_empty() {
        rows = rows.push(widget::text(fl!("gemini-accounts-empty")).size(13));
    } else {
        let mut account_rows = cosmic::iced::widget::column![]
            .spacing(6)
            .width(Length::Fill);
        for account in &accounts {
            account_rows = account_rows.push(account_settings_row(
                ProviderId::Gemini,
                account,
                &selected_ids,
                active_id,
                config,
                enabled,
            ));
        }
        rows = rows.push(account_selector_list(account_rows));
    }

    if accounts.len() > 1 {
        rows = rows.push(show_all_accounts_row(
            ProviderId::Gemini,
            config.show_all_accounts(ProviderId::Gemini),
            enabled,
        ));
    }

    rows = rows.push(gemini_login_controls(gemini_login, enabled));

    settings_block_enabled(
        widget::text(fl!("gemini-accounts-title")).size(16).into(),
        rows,
        enabled,
    )
}

fn antigravity_accounts_section<'a>(
    state: &'a AppState,
    config: &'a Config,
    antigravity_login: Option<&'a AntigravityLoginState>,
    enabled: bool,
) -> Element<'a, Message> {
    let selected_ids: Vec<&str> = state
        .provider(ProviderId::Antigravity)
        .map(|provider| {
            provider
                .selected_account_ids
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default();
    let accounts = state.accounts_for(ProviderId::Antigravity);
    let active_id = state
        .provider(ProviderId::Antigravity)
        .and_then(|provider| provider.system_active_account_id.as_deref());
    let mut rows = cosmic::iced::widget::column![]
        .spacing(8)
        .width(Length::Fill);

    if accounts.is_empty() {
        rows = rows.push(widget::text(fl!("antigravity-accounts-empty")).size(13));
    } else {
        let mut account_rows = cosmic::iced::widget::column![]
            .spacing(6)
            .width(Length::Fill);
        for account in &accounts {
            account_rows = account_rows.push(account_settings_row(
                ProviderId::Antigravity,
                account,
                &selected_ids,
                active_id,
                config,
                enabled,
            ));
        }
        rows = rows.push(account_selector_list(account_rows));
    }

    if accounts.len() > 1 {
        rows = rows.push(show_all_accounts_row(
            ProviderId::Antigravity,
            config.show_all_accounts(ProviderId::Antigravity),
            enabled,
        ));
    }

    rows = rows.push(antigravity_login_controls(antigravity_login, enabled));

    settings_block_enabled(
        widget::text(fl!("antigravity-accounts-title"))
            .size(16)
            .into(),
        rows,
        enabled,
    )
}

fn cursor_accounts_section<'a>(
    state: &'a AppState,
    config: &'a Config,
    cursor_scan: &'a CursorScanState,
    enabled: bool,
) -> Element<'a, Message> {
    let selected_ids: Vec<&str> = state
        .provider(ProviderId::Cursor)
        .map(|provider| {
            provider
                .selected_account_ids
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default();
    let accounts = state.accounts_for(ProviderId::Cursor);
    let active_id = state
        .provider(ProviderId::Cursor)
        .and_then(|provider| provider.system_active_account_id.as_deref());
    let mut rows = cosmic::iced::widget::column![]
        .spacing(8)
        .width(Length::Fill);

    if accounts.is_empty() {
        rows = rows.push(widget::text(fl!("cursor-accounts-empty")).size(13));
    } else {
        let mut account_rows = cosmic::iced::widget::column![]
            .spacing(6)
            .width(Length::Fill);
        for account in &accounts {
            account_rows = account_rows.push(account_settings_row(
                ProviderId::Cursor,
                account,
                &selected_ids,
                active_id,
                config,
                enabled,
            ));
        }
        rows = rows.push(account_selector_list(account_rows));
    }

    if accounts.len() > 1 {
        rows = rows.push(show_all_accounts_row(
            ProviderId::Cursor,
            config.show_all_accounts(ProviderId::Cursor),
            enabled,
        ));
    }

    rows = rows.push(cursor_scan_controls(cursor_scan, enabled));

    settings_block_enabled(
        widget::text(fl!("cursor-accounts-title")).size(16).into(),
        rows,
        enabled,
    )
}

fn copilot_accounts_section<'a>(
    state: &'a AppState,
    config: &'a Config,
    copilot_login: Option<&'a CopilotLoginState>,
    enabled: bool,
) -> Element<'a, Message> {
    let selected_ids: Vec<&str> = state
        .provider(ProviderId::Copilot)
        .map(|provider| {
            provider
                .selected_account_ids
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default();
    let accounts = state.accounts_for(ProviderId::Copilot);
    let active_id = state
        .provider(ProviderId::Copilot)
        .and_then(|provider| provider.system_active_account_id.as_deref());
    let mut rows = cosmic::iced::widget::column![]
        .spacing(8)
        .width(Length::Fill);

    if accounts.is_empty() {
        rows = rows.push(widget::text(fl!("copilot-accounts-empty")).size(13));
    } else {
        let mut account_rows = cosmic::iced::widget::column![]
            .spacing(6)
            .width(Length::Fill);
        for account in &accounts {
            account_rows = account_rows.push(account_settings_row(
                ProviderId::Copilot,
                account,
                &selected_ids,
                active_id,
                config,
                enabled,
            ));
        }
        rows = rows.push(account_selector_list(account_rows));
    }

    if accounts.len() > 1 {
        rows = rows.push(show_all_accounts_row(
            ProviderId::Copilot,
            config.show_all_accounts(ProviderId::Copilot),
            enabled,
        ));
    }

    rows = rows.push(copilot_login_controls(copilot_login, enabled));

    settings_block_enabled(
        widget::text(fl!("copilot-accounts-title")).size(16).into(),
        rows,
        enabled,
    )
}

fn minimax_accounts_section<'a>(
    state: &'a AppState,
    config: &'a Config,
    minimax_login: Option<&'a MinimaxLoginState>,
    enabled: bool,
) -> Element<'a, Message> {
    let minimax = state.provider(ProviderId::Minimax);
    let selected_ids: Vec<&str> = minimax
        .map(|provider| {
            provider
                .selected_account_ids
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default();
    let accounts = state.accounts_for(ProviderId::Minimax);
    let active_id = minimax.and_then(|provider| provider.system_active_account_id.as_deref());
    let mut rows = cosmic::iced::widget::column![]
        .spacing(8)
        .width(Length::Fill);

    if accounts.is_empty() {
        rows = rows.push(widget::text(fl!("minimax-accounts-empty")).size(13));
    } else {
        let mut account_rows = cosmic::iced::widget::column![]
            .spacing(6)
            .width(Length::Fill);
        for account in &accounts {
            account_rows = account_rows.push(account_settings_row(
                ProviderId::Minimax,
                account,
                &selected_ids,
                active_id,
                config,
                enabled,
            ));
        }
        rows = rows.push(account_selector_list(account_rows));
    }

    if let Some(provider) = minimax
        && provider.account_status == crate::model::AccountSelectionStatus::SelectionRequired
    {
        rows = rows.push(widget::text(fl!("minimax-account-select-required")).size(13));
    }

    if accounts.len() > 1 {
        rows = rows.push(show_all_accounts_row(
            ProviderId::Minimax,
            config.show_all_accounts(ProviderId::Minimax),
            enabled,
        ));
    }

    rows = rows.push(minimax_login_controls(minimax_login, enabled));

    settings_block_enabled(
        widget::text("Minimax Accounts").size(16).into(),
        rows,
        enabled,
    )
}
