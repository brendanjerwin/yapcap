use super::super::provider_assets::app_icon_handle;
use super::{
    Message, PROVIDER_ACCOUNT_HEADER_HEIGHT, PROVIDER_CARD_SPACING, PROVIDER_GROUP_HEADER_HEIGHT,
    PROVIDER_SECTION_HEIGHT, PROVIDER_SECTION_WITH_ACTION_HEIGHT, PROVIDER_SUMMARY_HEIGHT,
    PopupRoute, SettingsRoute, account_label_text, apply_alpha, badge_destructive, badge_neutral,
    badge_success, badge_warning, badge_with_tooltip, card, detected_without_accounts, info_block,
    plan_badge, provider_icon_handle, provider_icon_variant, provider_summary,
};
use crate::config::{Config, ResetTimeFormat, UsageAmountFormat};
use crate::currency_format;
use crate::fl;
use crate::model::{
    AccountSelectionStatus, AppState, AuthState, ExtraUsageState, ProviderAccountRuntimeState,
    ProviderCost, ProviderHealth, ProviderId, ProviderRuntimeState, STALE_THRESHOLD, UsageSnapshot,
    UsageWindow,
};
use crate::usage_display;
use cosmic::Element;
use cosmic::iced::widget::{column, container, progress_bar, row};
use cosmic::iced::{Alignment, Background, Color, Length};
use cosmic::widget;

pub(super) fn selected_provider_view<'a>(
    provider: Option<&'a ProviderRuntimeState>,
    state: &'a AppState,
    config: &'a Config,
    detection: &'a crate::detection::DetectionSnapshot,
) -> Element<'a, Message> {
    let Some(provider) = provider else {
        return empty_state_view();
    };
    let accounts = state.display_selected_accounts(provider.provider);
    let detected_without_accounts = detected_without_accounts(state, detection, provider.provider);
    let summary = provider_summary(provider, detected_without_accounts);

    if accounts.len() <= 1 {
        let account = accounts.first().copied();
        let items = account_column_items(account, provider, state, config, detection);
        let mut content = column![summary]
            .spacing(PROVIDER_CARD_SPACING)
            .width(Length::Fill);
        for item in items {
            content = content.push(item);
        }
        Element::from(content)
    } else {
        let mut content = column![summary]
            .spacing(PROVIDER_CARD_SPACING)
            .width(Length::Fill);
        let mut cols_row = row![].spacing(8);
        for account in &accounts {
            cols_row = cols_row.push(account_column_view(
                account, provider, state, config, detection,
            ));
        }
        content = content.push(cols_row);
        Element::from(content)
    }
}

pub(super) fn provider_body_height_multi(
    state: &AppState,
    provider: Option<&ProviderRuntimeState>,
) -> f32 {
    let Some(provider) = provider else {
        return PROVIDER_SUMMARY_HEIGHT;
    };
    let accounts = state.display_selected_accounts(provider.provider);
    if accounts.is_empty() {
        return provider_body_height_for_account(provider, None);
    }
    accounts
        .iter()
        .map(|account| provider_body_height_for_account(provider, Some(account)))
        .fold(PROVIDER_SUMMARY_HEIGHT, f32::max)
}

pub(super) fn active_snapshot<'a>(
    state: &'a AppState,
    provider: &'a ProviderRuntimeState,
) -> Option<&'a UsageSnapshot> {
    state
        .active_account(provider.provider)
        .and_then(|account| account.snapshot.as_ref())
        .or(provider.legacy_display_snapshot.as_ref())
}

fn account_column_items<'a>(
    account: Option<&'a ProviderAccountRuntimeState>,
    provider: &'a ProviderRuntimeState,
    state: &'a AppState,
    config: &'a Config,
    detection: &'a crate::detection::DetectionSnapshot,
) -> Vec<Element<'a, Message>> {
    let mut items = Vec::new();
    if let Some(account) = account {
        items.push(account_column_header(account, provider));
    }
    items.extend(account_column_body_items(
        account, provider, state, config, detection,
    ));
    items
}

fn account_column_body_items<'a>(
    account: Option<&'a ProviderAccountRuntimeState>,
    provider: &'a ProviderRuntimeState,
    state: &'a AppState,
    config: &'a Config,
    detection: &'a crate::detection::DetectionSnapshot,
) -> Vec<Element<'a, Message>> {
    let mut items = Vec::new();
    let snapshot = active_snapshot_for_account(account, provider);
    if let Some(snapshot) = snapshot {
        if account.is_some_and(|account| account.health == ProviderHealth::Error) {
            items.extend(provider_status_info(provider, state, account, detection));
        }
        let mut previous_group: Option<&str> = None;
        for window in &snapshot.windows {
            if let Some(group) = window.group.as_deref()
                && previous_group != Some(group)
            {
                items.push(group_header(group));
            }
            previous_group = window.group.as_deref();
            items.push(usage_section(
                window,
                window_display_label(snapshot.provider, &window.label),
                config.reset_time_format,
                config.usage_amount_format,
            ));
        }
        match snapshot.provider {
            ProviderId::Claude => {
                if let Some(extra) = snapshot.extra_usage.as_ref() {
                    items.push(extra_usage_detail_section(
                        extra,
                        config.usage_amount_format,
                    ));
                } else if let Some(cost) = snapshot.provider_cost.as_ref() {
                    items.push(extra_usage_cost_bar(cost, None, config.usage_amount_format));
                }
            }
            _ => {
                if let Some(cost) = snapshot.provider_cost.as_ref() {
                    items.push(cost_section(snapshot.provider, cost));
                }
            }
        }
    } else {
        items.extend(provider_status_info(provider, state, account, detection));
    }
    items
}

fn account_column_header<'a>(
    account: &'a ProviderAccountRuntimeState,
    provider: &'a ProviderRuntimeState,
) -> Element<'a, Message> {
    let snapshot = account.snapshot.as_ref();
    let account_label = snapshot
        .and_then(|snapshot| snapshot.identity.email.as_deref())
        .filter(|email| !email.is_empty())
        .unwrap_or(account.label.as_str());
    let plan_label = snapshot.and_then(|snapshot| snapshot.identity.plan.as_deref());

    let mut label_row = row![account_label_text(account_label, 14)]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill);
    label_row = label_row.push(cosmic::iced::widget::Space::new().width(Length::Fill));
    if let Some(plan) = plan_label.filter(|plan| !plan.trim().is_empty()) {
        label_row = label_row.push(plan_badge(plan));
    }

    let status = account_status_badge(account, provider);
    let mut status_row = row![status].spacing(8).align_y(Alignment::Center);
    let active_id = provider.system_active_account_id.as_deref();
    if active_id == Some(account.account_id.as_str()) {
        status_row = status_row.push(badge_with_tooltip(
            badge_success(fl!("badge-active")),
            fl!("badge-active-tooltip"),
        ));
    }
    if let Some(updated) = account.last_success_at.map(format_updated_label) {
        status_row = status_row.push(cosmic::iced::widget::Space::new().width(Length::Fill));
        status_row = status_row.push(widget::text(updated).size(12));
    }

    card(
        column![
            widget::text(fl!("account-label")).size(18),
            label_row,
            status_row,
        ]
        .spacing(6)
        .width(Length::Fill),
    )
}

fn account_column_view<'a>(
    account: &'a ProviderAccountRuntimeState,
    provider: &'a ProviderRuntimeState,
    state: &'a AppState,
    config: &'a Config,
    detection: &'a crate::detection::DetectionSnapshot,
) -> Element<'a, Message> {
    let header = account_column_header(account, provider);
    let body = account_column_body_items(Some(account), provider, state, config, detection);
    let mut content = column![header]
        .spacing(PROVIDER_CARD_SPACING)
        .width(Length::Fill);
    for item in body {
        content = content.push(item);
    }
    container(content)
        .width(Length::FillPortion(1))
        .padding([0, 8])
        .style(|theme: &cosmic::Theme| {
            let cosmic = theme.cosmic();
            widget::container::Style {
                text_color: None,
                background: Some(Background::Color(
                    cosmic.background(theme.transparent).component.base.into(),
                )),
                border: cosmic::iced::Border {
                    radius: cosmic.corner_radii.radius_m.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                shadow: cosmic::iced::Shadow::default(),
                icon_color: None,
                snap: false,
            }
        })
        .into()
}

pub(super) fn empty_state_view<'a>() -> Element<'a, Message> {
    let logos = row![
        widget::icon::icon(provider_icon_handle(
            ProviderId::Claude,
            provider_icon_variant()
        ))
        .size(22),
        widget::icon::icon(app_icon_handle()).size(48),
        widget::icon::icon(provider_icon_handle(
            ProviderId::Codex,
            provider_icon_variant()
        ))
        .size(22),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let content = column![
        logos,
        widget::text(fl!("no-providers")).size(22),
        widget::text(fl!("no-providers-detail")).size(13),
        widget::button::suggested(fl!("no-providers-open-settings")).on_press(Message::NavigateTo(
            PopupRoute::Settings(SettingsRoute::General)
        )),
    ]
    .spacing(12)
    .align_x(Alignment::Center)
    .width(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(32)
        .into()
}

fn provider_status_info(
    provider: &ProviderRuntimeState,
    state: &AppState,
    active_account: Option<&ProviderAccountRuntimeState>,
    detection: &crate::detection::DetectionSnapshot,
) -> Option<Element<'static, Message>> {
    if detected_without_accounts(state, detection, provider.provider) {
        return Some(detected_provider_cta(provider.provider));
    }
    let message = provider_status_message(provider, state, active_account);
    if message.is_empty() {
        return None;
    }
    Some(info_block(
        fl!("status-label"),
        message,
        None,
        login_required_settings_action(provider, state, active_account),
    ))
}

fn detected_provider_cta(provider: ProviderId) -> Element<'static, Message> {
    info_block(
        fl!("provider-detected-chip"),
        fl!("provider-detected-cta", provider = provider.label()),
        None,
        Some(
            widget::button::suggested(fl!("provider-detected-add-account"))
                .on_press(Message::NavigateTo(PopupRoute::Settings(
                    SettingsRoute::Provider(provider),
                )))
                .into(),
        ),
    )
}

fn login_required_settings_action(
    provider: &ProviderRuntimeState,
    state: &AppState,
    active_account: Option<&ProviderAccountRuntimeState>,
) -> Option<Element<'static, Message>> {
    if !should_show_login_required_settings_action(provider, state, active_account) {
        return None;
    }

    Some(
        widget::button::standard(fl!(
            "open-provider-settings",
            provider = provider.provider.label()
        ))
        .on_press(Message::NavigateTo(PopupRoute::Settings(
            SettingsRoute::Provider(provider.provider),
        )))
        .into(),
    )
}

fn should_show_login_required_settings_action(
    provider: &ProviderRuntimeState,
    state: &AppState,
    active_account: Option<&ProviderAccountRuntimeState>,
) -> bool {
    active_account.is_none()
        && state.accounts_for(provider.provider).is_empty()
        && provider.account_status == AccountSelectionStatus::LoginRequired
}

fn provider_status_message(
    provider: &ProviderRuntimeState,
    _state: &AppState,
    active_account: Option<&ProviderAccountRuntimeState>,
) -> String {
    let mut messages = Vec::new();

    let cursor_reauth_needed = active_account.is_some_and(|a| {
        a.provider == ProviderId::Cursor && a.auth_state == AuthState::ActionRequired
    });

    if cursor_reauth_needed {
        messages.push(fl!("cursor-account-reauth-detail"));
    } else if let Some(account) = active_account
        && account.health == ProviderHealth::Error
    {
        if account.auth_state == AuthState::ActionRequired {
            messages.push(fl!("account-reauth-summary"));
        } else if let Some(error) = &account.error {
            messages.push(error.clone());
        }
    } else {
        messages.push(provider.status_line(active_account));
    }

    dedup_status_messages(messages).join(" ")
}

fn dedup_status_messages(messages: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for message in messages {
        if !message.is_empty() && !deduped.contains(&message) {
            deduped.push(message);
        }
    }
    deduped
}

fn provider_body_height_for_account(
    provider: &ProviderRuntimeState,
    account: Option<&ProviderAccountRuntimeState>,
) -> f32 {
    let mut height = PROVIDER_SUMMARY_HEIGHT;
    let mut cards = 1usize;

    if account.is_some() {
        height += PROVIDER_ACCOUNT_HEADER_HEIGHT;
        cards += 1;
    }

    let snapshot = active_snapshot_for_account(account, provider);
    if let Some(snapshot) = snapshot {
        if account.map(|account| &account.health) == Some(&ProviderHealth::Error) {
            height += PROVIDER_SECTION_HEIGHT;
            cards += 1;
        }

        let window_count = f32::from(u16::try_from(snapshot.windows.len()).unwrap_or(u16::MAX));
        height += window_count * PROVIDER_SECTION_HEIGHT;
        cards += snapshot.windows.len();

        let group_headers = group_header_count(&snapshot.windows);
        if group_headers > 0 {
            height += f32::from(u16::try_from(group_headers).unwrap_or(u16::MAX))
                * (PROVIDER_GROUP_HEADER_HEIGHT + PROVIDER_CARD_SPACING);
            cards += group_headers;
        }
        match snapshot.provider {
            ProviderId::Claude => {
                if snapshot.extra_usage.is_some() || snapshot.provider_cost.as_ref().is_some() {
                    height += PROVIDER_SECTION_HEIGHT;
                    cards += 1;
                }
            }
            _ => {
                if snapshot.provider_cost.as_ref().is_some() {
                    height += PROVIDER_SECTION_HEIGHT;
                    cards += 1;
                }
            }
        }
    } else {
        height += if account.is_none()
            && provider.account_status == AccountSelectionStatus::LoginRequired
        {
            PROVIDER_SECTION_WITH_ACTION_HEIGHT
        } else {
            PROVIDER_SECTION_HEIGHT
        };
        cards += 1;
    }

    let gaps = f32::from(u16::try_from(cards.saturating_sub(1)).unwrap_or(u16::MAX));
    height + gaps * PROVIDER_CARD_SPACING
}

fn active_snapshot_for_account<'a>(
    account: Option<&'a ProviderAccountRuntimeState>,
    provider: &'a ProviderRuntimeState,
) -> Option<&'a UsageSnapshot> {
    account
        .and_then(|account| account.snapshot.as_ref())
        .or(provider.legacy_display_snapshot.as_ref())
}

fn usage_section(
    window: &UsageWindow,
    display_label: String,
    reset_time_format: ResetTimeFormat,
    usage_amount_format: UsageAmountFormat,
) -> Element<'static, Message> {
    let now = chrono::Utc::now();
    let pace = usage_display::pace(window, now);
    usage_block(
        display_label,
        usage_display::displayed_amount_percent(window, now, usage_amount_format),
        usage_display::usage_amount_label(window, now, usage_amount_format),
        UsageBlockDetails {
            secondary: usage_display::reset_label(window, now, reset_time_format),
            secondary_tooltip: None,
            pace,
            pace_marker_percent: pace_marker_percent(pace, usage_amount_format),
            overage: overage_text(window),
        },
    )
}

fn group_header_count(windows: &[UsageWindow]) -> usize {
    let mut count = 0;
    let mut previous: Option<&str> = None;
    for window in windows {
        if let Some(group) = window.group.as_deref()
            && previous != Some(group)
        {
            count += 1;
        }
        previous = window.group.as_deref();
    }
    count
}

fn group_header(group: &str) -> Element<'static, Message> {
    container(widget::text(group.to_string()).size(15))
        .width(Length::Fill)
        .padding([4, 4])
        .into()
}

fn window_display_label(provider: ProviderId, label: &str) -> String {
    if provider == ProviderId::Copilot {
        match label {
            "chat" => return fl!("copilot-window-chat"),
            "completions" => return fl!("copilot-window-completions"),
            "premium_interactions" => return fl!("copilot-window-premium"),
            "credits" => return fl!("copilot-window-credits"),
            _ => {}
        }
    }
    label.to_string()
}

fn overage_text(window: &UsageWindow) -> Option<String> {
    if window.label == "premium_interactions" || window.label == "credits" {
        return window.reset_description.clone();
    }
    None
}

fn extra_usage_detail_section(
    state: &ExtraUsageState,
    usage_amount_format: UsageAmountFormat,
) -> Element<'static, Message> {
    match state {
        ExtraUsageState::Disabled => info_block(
            fl!("extra-usage-label"),
            fl!("extra-usage-disabled"),
            None,
            None,
        ),
        ExtraUsageState::Active { used_percent, cost } => {
            extra_usage_cost_bar(cost, Some(*used_percent), usage_amount_format)
        }
    }
}

fn extra_usage_pct_from_cost(cost: &ProviderCost) -> f32 {
    cost.limit
        .filter(|l| *l > f64::EPSILON)
        .map_or(0.0_f32, |l| usage_display::portion_percent(cost.used, l))
}

fn extra_usage_cost_bar(
    cost: &ProviderCost,
    used_percent: Option<f32>,
    usage_amount_format: UsageAmountFormat,
) -> Element<'static, Message> {
    let now = chrono::Utc::now();
    let used_percent = used_percent
        .unwrap_or_else(|| extra_usage_pct_from_cost(cost))
        .clamp(0.0, 100.0);
    let window = UsageWindow {
        label: String::new(),
        used_percent,
        reset_at: None,
        window_seconds: None,
        reset_description: None,
        group: None,
    };
    let (cost_line, cost_tip) = currency_format::format_provider_cost(cost);
    usage_block(
        fl!("extra-usage-label"),
        usage_display::displayed_amount_percent(&window, now, usage_amount_format),
        usage_display::usage_amount_label(&window, now, usage_amount_format),
        UsageBlockDetails {
            secondary: Some(cost_line),
            secondary_tooltip: Some(cost_tip),
            pace: None,
            pace_marker_percent: None,
            overage: None,
        },
    )
}

fn cost_section(provider: ProviderId, cost: &ProviderCost) -> Element<'static, Message> {
    if provider == ProviderId::Codex {
        return credit_section(cost);
    }
    let (primary, iso_tip) = currency_format::format_provider_cost(cost);
    let body = widget::tooltip::tooltip(
        widget::text(primary).size(14),
        widget::text(iso_tip).size(12),
        widget::tooltip::Position::Top,
    );
    card(column![widget::text(fl!("extra-usage-label")).size(18), body,].spacing(6))
}

fn credit_section(cost: &ProviderCost) -> Element<'static, Message> {
    let balance = if cost.used.fract() == 0.0 {
        format!("{:.0}", cost.used)
    } else {
        format!("{:.2}", cost.used)
    };

    card(
        column![
            widget::text(fl!("credits-label")).size(18),
            widget::text(fl!("credits-available", balance = balance.as_str())).size(14),
        ]
        .spacing(6),
    )
}

fn usage_block(
    title: String,
    percent: f32,
    primary: String,
    details: UsageBlockDetails,
) -> Element<'static, Message> {
    let pct_row = row![
        widget::text(primary).size(14),
        cosmic::iced::widget::Space::new().width(Length::Fill),
        secondary_cost_text(
            details.secondary.unwrap_or_default(),
            details.secondary_tooltip
        ),
    ]
    .align_y(Alignment::Center);

    let mut content = column![
        widget::text(title).size(18),
        paced_progress_bar(
            percent,
            details.pace_marker_percent,
            details.pace.map(usage_display::pace_label)
        ),
    ]
    .spacing(6);

    if let Some(overage) = details.overage {
        content = content.push(overage_line(overage));
    }

    card(content.push(pct_row))
}

struct UsageBlockDetails {
    secondary: Option<String>,
    secondary_tooltip: Option<String>,
    pace: Option<usage_display::UsagePace>,
    pace_marker_percent: Option<f32>,
    overage: Option<String>,
}

fn overage_line(text: String) -> Element<'static, Message> {
    container(widget::text(text).size(13))
        .style(|theme: &cosmic::Theme| {
            let color = apply_alpha(theme.cosmic().warning.base.into(), 0.92);
            widget::container::Style {
                text_color: Some(color),
                background: None,
                border: cosmic::iced::Border::default(),
                shadow: cosmic::iced::Shadow::default(),
                icon_color: Some(color),
                snap: true,
            }
        })
        .into()
}

fn secondary_cost_text(text: String, tooltip: Option<String>) -> Element<'static, Message> {
    if text.is_empty() {
        cosmic::iced::widget::Space::new()
            .width(Length::Shrink)
            .into()
    } else if let Some(tip) = tooltip {
        widget::tooltip::tooltip(
            widget::text(text).size(13),
            widget::text(tip).size(12),
            widget::tooltip::Position::Top,
        )
        .into()
    } else {
        widget::text(text).size(13).into()
    }
}

fn paced_progress_bar(
    percent: f32,
    pace_marker_percent: Option<f32>,
    pace_label: Option<String>,
) -> Element<'static, Message> {
    let progress: Element<'static, Message> = progress_bar(0.0..=100.0, percent)
        .length(Length::Fill)
        .girth(Length::Fixed(8.0))
        .into();

    let bar = if let Some(marker_percent) = pace_marker_percent {
        cosmic::iced::widget::Stack::new()
            .push(progress)
            .push(pace_marker(marker_percent))
            .width(Length::Fill)
            .height(Length::Fixed(8.0))
            .into()
    } else {
        progress
    };

    if let Some(label) = pace_label {
        widget::tooltip::tooltip(
            bar,
            widget::text(label).size(12),
            widget::tooltip::Position::Top,
        )
        .into()
    } else {
        bar
    }
}

fn pace_marker_percent(
    pace: Option<usage_display::UsagePace>,
    usage_amount_format: UsageAmountFormat,
) -> Option<f32> {
    pace.map(|pace| match usage_amount_format {
        UsageAmountFormat::Used => pace.expected_percent,
        UsageAmountFormat::Left => 100.0 - pace.expected_percent,
    })
}

fn pace_marker(expected_percent: f32) -> Element<'static, Message> {
    let left = pace_marker_portion(expected_percent);
    let right = 1000 - left;
    row![
        cosmic::iced::widget::Space::new().width(Length::FillPortion(left)),
        container(cosmic::iced::widget::Space::new())
            .width(Length::Fixed(3.0))
            .height(Length::Fixed(8.0))
            .style(|theme: &cosmic::Theme| {
                let cosmic = theme.cosmic();
                widget::container::Style {
                    text_color: None,
                    background: Some(Background::Color(cosmic.accent.pressed.into())),
                    border: cosmic::iced::Border {
                        radius: 0.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    shadow: cosmic::iced::Shadow::default(),
                    icon_color: None,
                    snap: true,
                }
            }),
        cosmic::iced::widget::Space::new().width(Length::FillPortion(right)),
    ]
    .width(Length::Fill)
    .height(Length::Fixed(8.0))
    .into()
}

fn pace_marker_portion(expected_percent: f32) -> u16 {
    let scaled = (expected_percent * 10.0).clamp(1.0, 999.0);
    let mut portion = 1u16;
    while portion < 999 && f32::from(portion) + 0.5 <= scaled {
        portion += 1;
    }
    portion
}

fn account_status_badge(
    account: &ProviderAccountRuntimeState,
    provider: &ProviderRuntimeState,
) -> Element<'static, Message> {
    if provider.is_refreshing {
        return badge_with_tooltip(
            badge_neutral(fl!("badge-refreshing")),
            fl!("badge-refreshing-tooltip"),
        );
    }
    if account.auth_state == AuthState::ActionRequired {
        if account.provider == ProviderId::Cursor {
            return badge_with_tooltip(
                badge_neutral(fl!("badge-cursor-reauth-needed")),
                fl!("badge-cursor-reauth-needed-tooltip"),
            );
        }
        return badge_with_tooltip(
            badge_warning(fl!("badge-login-required")),
            fl!("badge-login-required-tooltip"),
        );
    }
    if account.health == ProviderHealth::Error {
        return badge_with_tooltip(
            badge_destructive(fl!("badge-error")),
            fl!("badge-error-tooltip"),
        );
    }
    let now = chrono::Utc::now();
    if account.health == ProviderHealth::Ok
        && account.snapshot.is_some()
        && account
            .last_success_at
            .is_some_and(|updated| now - updated < STALE_THRESHOLD)
    {
        return badge_with_tooltip(badge_success(fl!("badge-live")), fl!("badge-live-tooltip"));
    }
    if account.snapshot.is_some() {
        return badge_with_tooltip(
            badge_warning(fl!("badge-stale")),
            fl!("badge-stale-tooltip"),
        );
    }
    badge_with_tooltip(
        badge_neutral(fl!("badge-loading")),
        fl!("badge-loading-tooltip"),
    )
}

fn format_updated_label(last_success_at: chrono::DateTime<chrono::Utc>) -> String {
    let age = chrono::Utc::now() - last_success_at;
    if age.num_seconds() < 10 {
        fl!("updated-just-now")
    } else if age.num_minutes() < 1 {
        fl!("updated-seconds-ago", n = age.num_seconds())
    } else if age.num_hours() < 1 {
        fl!("updated-minutes-ago", n = age.num_minutes())
    } else if age.num_days() < 1 {
        fl!("updated-hours-ago", n = age.num_hours())
    } else {
        let date = last_success_at.format("%Y-%m-%d %H:%M").to_string();
        fl!("updated-at", date = date.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AccountSelectionStatus, AuthState, ProviderHealth};

    fn grouped_window(group: &str) -> UsageWindow {
        UsageWindow {
            label: "Weekly Limit".to_string(),
            used_percent: 10.0,
            reset_at: None,
            window_seconds: Some(5 * 3600),
            reset_description: None,
            group: Some(group.to_string()),
        }
    }

    fn ungrouped_window() -> UsageWindow {
        UsageWindow {
            label: "Session".to_string(),
            used_percent: 10.0,
            reset_at: None,
            window_seconds: None,
            reset_description: None,
            group: None,
        }
    }

    #[test]
    fn group_header_count_counts_only_group_boundaries() {
        let windows = vec![
            grouped_window("Gemini Models"),
            grouped_window("Gemini Models"),
            grouped_window("Claude and GPT models"),
            grouped_window("Claude and GPT models"),
        ];
        assert_eq!(group_header_count(&windows), 2);
    }

    #[test]
    fn group_header_count_is_zero_for_ungrouped_providers() {
        let windows = vec![ungrouped_window(), ungrouped_window()];
        assert_eq!(group_header_count(&windows), 0);
    }

    #[test]
    fn action_required_account_reports_reauth_message() {
        let provider = ProviderRuntimeState {
            provider: ProviderId::Claude,
            enabled: true,
            selected_account_ids: vec!["claude-1".to_string()],
            active_account_id: Some("claude-1".to_string()),
            system_active_account_id: None,
            account_status: AccountSelectionStatus::Ready,
            is_refreshing: false,
            refresh_started_at: None,
            legacy_display_snapshot: None,
            error: None,
        };
        let mut account =
            ProviderAccountRuntimeState::empty(ProviderId::Claude, "claude-1", "Claude account");
        account.health = ProviderHealth::Error;
        account.auth_state = AuthState::ActionRequired;
        account.error = Some("claude token refresh returned http 400".to_string());
        let state = AppState::empty();

        let message = provider_status_message(&provider, &state, Some(&account));
        assert!(
            !message.contains("http 400"),
            "status message must not contain raw error: {message}"
        );
        assert!(
            message.contains("Re-authenticate") || message.contains("Settings"),
            "status message should be action-oriented: {message}"
        );
    }

    #[test]
    fn cursor_action_required_account_reports_reauth_needed_message() {
        let provider = ProviderRuntimeState {
            provider: ProviderId::Cursor,
            enabled: true,
            selected_account_ids: vec!["cursor-1".to_string()],
            active_account_id: Some("cursor-1".to_string()),
            system_active_account_id: Some("cursor-1".to_string()),
            account_status: AccountSelectionStatus::Ready,
            is_refreshing: false,
            refresh_started_at: None,
            legacy_display_snapshot: None,
            error: None,
        };
        let mut account =
            ProviderAccountRuntimeState::empty(ProviderId::Cursor, "cursor-1", "user@example.com");
        account.health = ProviderHealth::Error;
        account.auth_state = AuthState::ActionRequired;
        account.error = Some("Unauthorized".to_string());
        let state = AppState::empty();

        let message = provider_status_message(&provider, &state, Some(&account));

        assert!(
            message.contains("Re-authenticate") || message.contains("rescan"),
            "status message should explain the Cursor recovery path: {message}"
        );
        assert!(
            !message.contains("Inactive") && !message.contains("inactive"),
            "status message must not describe auth failures as inactive: {message}"
        );
    }

    #[test]
    fn cursor_action_required_badge_copy_uses_reauth_needed() {
        assert_eq!(fl!("badge-cursor-reauth-needed"), "Re-auth needed");
        assert!(!fl!("badge-cursor-reauth-needed-tooltip").contains("inactive"));
    }

    #[test]
    fn codex_without_accounts_reports_login_required() {
        let provider = ProviderRuntimeState {
            provider: ProviderId::Codex,
            enabled: true,
            selected_account_ids: Vec::new(),
            active_account_id: None,
            system_active_account_id: None,
            account_status: AccountSelectionStatus::LoginRequired,
            is_refreshing: false,
            refresh_started_at: None,
            legacy_display_snapshot: None,
            error: Some("Login required".to_string()),
        };
        let state = AppState::empty();

        assert_eq!(
            provider_status_message(&provider, &state, None),
            "Login required"
        );
    }

    #[test]
    fn login_required_empty_state_links_to_each_provider_settings_page() {
        for provider_id in ProviderId::ALL {
            let provider = ProviderRuntimeState {
                provider: provider_id,
                enabled: true,
                selected_account_ids: Vec::new(),
                active_account_id: None,
                system_active_account_id: None,
                account_status: AccountSelectionStatus::LoginRequired,
                is_refreshing: false,
                refresh_started_at: None,
                legacy_display_snapshot: None,
                error: Some("Login required".to_string()),
            };
            let state = AppState::empty();

            assert!(should_show_login_required_settings_action(
                &provider, &state, None
            ));
        }
    }

    #[test]
    fn login_required_settings_action_hides_when_account_exists() {
        let provider = ProviderRuntimeState {
            provider: ProviderId::Codex,
            enabled: true,
            selected_account_ids: Vec::new(),
            active_account_id: None,
            system_active_account_id: None,
            account_status: AccountSelectionStatus::LoginRequired,
            is_refreshing: false,
            refresh_started_at: None,
            legacy_display_snapshot: None,
            error: Some("Login required".to_string()),
        };
        let mut state = AppState::empty();
        state
            .provider_accounts
            .push(ProviderAccountRuntimeState::empty(
                ProviderId::Codex,
                "codex-test",
                "test@example.com",
            ));

        assert!(!should_show_login_required_settings_action(
            &provider, &state, None
        ));
    }

    #[test]
    fn detected_provider_without_accounts_shows_detected_cta() {
        let home = tempfile::tempdir().expect("create temporary home");
        std::fs::create_dir(home.path().join(".codex")).expect("create Codex marker");
        let detection = crate::detection::detect(home.path());
        let state = AppState::empty();

        assert!(detected_without_accounts(
            &state,
            &detection,
            ProviderId::Codex
        ));
        assert!(!detected_without_accounts(
            &state,
            &detection,
            ProviderId::Claude
        ));

        let mut with_account = state.clone();
        with_account
            .provider_accounts
            .push(ProviderAccountRuntimeState::empty(
                ProviderId::Codex,
                "codex-test",
                "test@example.com",
            ));
        assert!(!detected_without_accounts(
            &with_account,
            &detection,
            ProviderId::Codex
        ));
    }
}
