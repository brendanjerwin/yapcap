// SPDX-License-Identifier: MPL-2.0

mod badges;
mod detail;
mod measure;
mod settings;

use self::badges::{
    account_label_text, apply_alpha, badge_accent, badge_destructive, badge_destructive_soft,
    badge_neutral, badge_neutral_soft, badge_success, badge_success_soft, badge_warning,
    badge_warning_soft, badge_with_tooltip, disabled_account_label_text, plan_badge,
};
use self::detail::{
    active_snapshot, empty_state_view, provider_body_height_multi, selected_provider_view,
};
use self::measure::Measure;
use self::settings::{general_settings_view, provider_settings_view, settings_body_height};
use super::provider_assets::{provider_icon_handle, provider_icon_variant};
use crate::app::{Message, PopupRoute, SettingsRoute};
use crate::config::{Config, PanelIconStyle, ResetTimeFormat, UsageAmountFormat};
use crate::detection::DetectionSnapshot;
use crate::fl;
use crate::model::{
    AppState, ProviderAccountRuntimeState, ProviderId, ProviderRuntimeState, UsageWindow,
};
use crate::providers::antigravity::{AntigravityLoginState, AntigravityLoginStatus};
use crate::providers::claude::{ClaudeLoginState, ClaudeLoginStatus};
use crate::providers::codex::{CodexLoginState, CodexLoginStatus};
use crate::providers::copilot::{CopilotLoginState, CopilotLoginStatus};
use crate::providers::cursor::{CursorScanState};
use crate::providers::gemini::{GeminiLoginState, GeminiLoginStatus};
use crate::providers::interface::ProviderAccountActionSupport;
use crate::providers::minimax::MinimaxLoginState;
use crate::providers::ollama_cloud::OllamaCloudLoginState;
use crate::providers::opencode_go::OpencodeGoLoginState;
use crate::providers::registry;
use super::popup_max_height;
use crate::updates::UpdateStatus;
use crate::usage_display;
use cosmic::Element;
use cosmic::iced::widget::{column, container, progress_bar, row, scrollable};
use cosmic::iced::{Alignment, Background, Color, Length, Size};
use cosmic::widget;

pub const POPUP_COLUMN_WIDTH: f32 = 420.0;
const POPUP_WIDTH: f32 = POPUP_COLUMN_WIDTH;
const POPUP_PADDING: f32 = 32.0;
const POPUP_CHROME_SPACING: f32 = 42.0;
const POPUP_EMPTY_CHROME_SPACING: f32 = 28.0;
const POPUP_HEADER_HEIGHT: f32 = 36.0;
const POPUP_TAB_HEIGHT: f32 = 68.0;
const PROVIDER_TABS_PER_ROW: usize = 4;
const PROVIDER_TAB_ROW_SPACING: f32 = 8.0;
const POPUP_FOOTER_HEIGHT: f32 = 28.0;
const POPUP_BODY_PANEL_PADDING: f32 = 24.0;
const POPUP_BODY_BOTTOM_SLACK: f32 = 8.0;
const EMPTY_STATE_BODY_HEIGHT: f32 = 240.0;
const PROVIDER_CARD_SPACING: f32 = 8.0;
const PROVIDER_SUMMARY_HEIGHT: f32 = 58.0;
const PROVIDER_ACCOUNT_HEADER_HEIGHT: f32 = 96.0;
const PROVIDER_SECTION_HEIGHT: f32 = 84.0;
const PROVIDER_SECTION_WITH_ACTION_HEIGHT: f32 = 120.0;
const PROVIDER_GROUP_HEADER_HEIGHT: f32 = 28.0;
const PROVIDER_GROUP_PADDING: f32 = 24.0;
const PROVIDER_GROUP_SPACING: f32 = 12.0;
const PROVIDER_CARD_PADDING: f32 = 16.0;
const SETTINGS_SECTION_HEIGHT: f32 = 104.0;
const SETTINGS_PROVIDER_ROW_HEIGHT: f32 = 44.0;
const PROVIDER_TAB_ICON_SIZE: u16 = 16;
const PROVIDER_TAB_ICON_LENGTH: f32 = 16.0;
const PROVIDER_TAB_LABEL_SIZE: u16 = 11;
const PROVIDER_PICKER_TILE_HEIGHT: f32 = 104.0;
const PROVIDER_PICKER_COMPACT_TILE_HEIGHT: f32 = 48.0;
const PROVIDER_PICKER_HEIGHT: f32 = 680.0;
const UPDATE_NOTIFICATION_DOT_COLOR: Color = Color::from_rgb(0.93, 0.11, 0.15);
const ACCENT_SOFT_FILL_ALPHA: f32 = 0.14;

#[derive(Clone, Copy)]
pub struct ProviderLoginStates<'a> {
    pub provider_picker_open: bool,
    pub codex: Option<&'a CodexLoginState>,
    pub claude: Option<&'a ClaudeLoginState>,
    pub cursor_scan: &'a CursorScanState,
    pub gemini: Option<&'a GeminiLoginState>,
    pub copilot: Option<&'a CopilotLoginState>,
    pub minimax: Option<&'a MinimaxLoginState>,
    pub antigravity: Option<&'a AntigravityLoginState>,
    pub opencode_go: Option<&'a OpencodeGoLoginState>,
    pub ollama_cloud: Option<&'a OllamaCloudLoginState>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PopupBodyMeasureTarget {
    Provider(ProviderId),
    EmptyState,
    Settings(SettingsRoute),
}

pub fn popup_content<'a>(
    state: &'a AppState,
    config: &'a Config,
    detection: &'a DetectionSnapshot,
    logins: ProviderLoginStates<'a>,
    selected_provider: ProviderId,
    route: &'a PopupRoute,
    update_status: &'a UpdateStatus,
) -> Element<'a, Message> {
    let selected = selected_state(state, selected_provider);
    let empty_state = popup_empty_state_active(state);

    let picker_open =
        logins.provider_picker_open && matches!(route, PopupRoute::ProviderDetail) && !empty_state;
    let header = popup_header(route, empty_state);

    let nav_row: Option<Element<'_, Message>> = match route {
        PopupRoute::ProviderDetail if empty_state => None,
        PopupRoute::ProviderDetail if !picker_open => {
            (enabled_provider_count(state) > 1).then(|| provider_tab_rows(state, selected_provider))
        }
        PopupRoute::ProviderDetail => None,
        PopupRoute::Settings(settings_route) => {
            Some(settings_category_row(settings_route, update_status))
        }
    };

    let body = if picker_open {
        provider_picker_view(state, detection)
    } else {
        popup_body_view(
            state,
            config,
            detection,
            logins,
            selected,
            route,
            update_status,
        )
    };

    let footer_action: Element<'_, Message> = match route {
        PopupRoute::ProviderDetail => settings_footer_action(update_status),
        PopupRoute::Settings(_) => widget::button::text(fl!("done"))
            .on_press(Message::NavigateTo(PopupRoute::ProviderDetail))
            .into(),
    };

    let footer = row![
        widget::button::text(fl!("quit")).on_press(Message::Quit),
        cosmic::iced::widget::Space::new().width(Length::Fill),
        footer_action,
    ]
    .align_y(Alignment::Center);

    let body_panel: Element<'_, Message> = container(panel(scrollable(body).width(Length::Fill)))
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    let body_stack = popup_body_stack(state, config, detection, logins, update_status, body_panel);

    let mut content = column![narrow_chrome(header)];
    if let Some(nav_row) = nav_row {
        content = content.push(narrow_chrome(nav_row));
    }
    let content = content
        .push(body_stack)
        .push(narrow_chrome(footer))
        .spacing(14)
        .padding(16)
        .width(Length::Fill);

    Element::from(content)
}

fn popup_body_view<'a>(
    state: &'a AppState,
    config: &'a Config,
    detection: &'a DetectionSnapshot,
    logins: ProviderLoginStates<'a>,
    selected: Option<&'a ProviderRuntimeState>,
    route: &'a PopupRoute,
    update_status: &'a UpdateStatus,
) -> Element<'a, Message> {
    match route {
        PopupRoute::ProviderDetail if popup_empty_state_active(state) => empty_state_view(),
        PopupRoute::ProviderDetail => selected_provider_view(selected, state, config, detection),
        PopupRoute::Settings(SettingsRoute::General) => {
            general_settings_view(config, update_status)
        }
        PopupRoute::Settings(SettingsRoute::Provider(id)) => {
            provider_settings_view(state, config, detection, logins, *id)
        }
    }
}

fn popup_body_stack<'a>(
    state: &'a AppState,
    config: &'a Config,
    detection: &'a DetectionSnapshot,
    logins: ProviderLoginStates<'a>,
    update_status: &'a UpdateStatus,
    body_panel: Element<'a, Message>,
) -> Element<'a, Message> {
    let mut stack = cosmic::iced::widget::Stack::new()
        .push(body_panel)
        .width(Length::Fill)
        .height(Length::Fill);

    if popup_empty_state_active(state) {
        stack = stack.push(Measure::new(
            empty_state_view(),
            body_measure_width(1.0),
            |size| Message::PopupBodyMeasured(PopupBodyMeasureTarget::EmptyState, size),
        ));
    }

    for provider in state.providers.iter().filter(|provider| provider.enabled) {
        let provider_id = provider.provider;
        let width = body_measure_width(selected_account_count(state, provider_id));
        let body = selected_provider_view(Some(provider), state, config, detection);
        stack = stack.push(Measure::new(body, width, move |size| {
            Message::PopupBodyMeasured(PopupBodyMeasureTarget::Provider(provider_id), size)
        }));
    }

    let general = general_settings_view(config, update_status);
    stack = stack.push(Measure::new(general, body_measure_width(1.0), |size| {
        Message::PopupBodyMeasured(
            PopupBodyMeasureTarget::Settings(SettingsRoute::General),
            size,
        )
    }));

    for provider in ProviderId::ALL {
        let body = provider_settings_view(state, config, detection, logins, provider);
        stack = stack.push(Measure::new(body, body_measure_width(1.0), move |size| {
            Message::PopupBodyMeasured(
                PopupBodyMeasureTarget::Settings(SettingsRoute::Provider(provider)),
                size,
            )
        }));
    }

    stack.into()
}

pub fn popup_max_width(state: &AppState) -> f32 {
    ProviderId::ALL
        .iter()
        .map(|&p| selected_account_count(state, p))
        .fold(1.0_f32, f32::max)
        * POPUP_WIDTH
}

pub fn popup_session_size(state: &AppState, selected_provider: ProviderId) -> Size {
    if popup_empty_state_active(state) {
        return popup_empty_state_size(EMPTY_STATE_BODY_HEIGHT);
    }
    let n_cols = selected_account_count(state, selected_provider);
    let width = POPUP_WIDTH * n_cols;
    let provider_height = state
        .providers
        .iter()
        .filter(|provider| provider.enabled)
        .map(|provider| provider_body_height_multi(state, Some(provider)))
        .fold(PROVIDER_SUMMARY_HEIGHT, f32::max);
    Size::new(
        width,
        popup_total_height(provider_nav_height(state), provider_height),
    )
}

pub const fn popup_provider_picker_size() -> Size {
    Size::new(POPUP_WIDTH, PROVIDER_PICKER_HEIGHT)
}

pub fn popup_session_size_with_body_height(
    state: &AppState,
    selected_provider: ProviderId,
    body_height: f32,
) -> Size {
    if popup_empty_state_active(state) {
        return popup_empty_state_size(body_height);
    }
    let n_cols = selected_account_count(state, selected_provider);
    let width = POPUP_WIDTH * n_cols;
    Size::new(
        width,
        popup_total_height(provider_nav_height(state), body_height),
    )
}

pub fn popup_settings_size(state: &AppState) -> Size {
    Size::new(
        POPUP_WIDTH,
        popup_total_height(Some(settings_nav_height()), settings_body_height(state)),
    )
}

pub fn popup_settings_size_with_body_height(body_height: f32) -> Size {
    Size::new(
        POPUP_WIDTH,
        popup_total_height(Some(settings_nav_height()), body_height),
    )
}

fn popup_total_height(nav_height: Option<f32>, body_height: f32) -> f32 {
    let chrome_spacing = if nav_height.is_some() {
        POPUP_CHROME_SPACING
    } else {
        POPUP_EMPTY_CHROME_SPACING
    };
    let height = POPUP_PADDING
        + chrome_spacing
        + POPUP_HEADER_HEIGHT
        + nav_height.unwrap_or(0.0)
        + POPUP_FOOTER_HEIGHT
        + POPUP_BODY_PANEL_PADDING
        + POPUP_BODY_BOTTOM_SLACK
        + body_height;
    height.clamp(1.0, popup_max_height())
}

fn popup_empty_state_size(body_height: f32) -> Size {
    let height = POPUP_PADDING
        + POPUP_EMPTY_CHROME_SPACING
        + POPUP_HEADER_HEIGHT
        + POPUP_FOOTER_HEIGHT
        + POPUP_BODY_PANEL_PADDING
        + POPUP_BODY_BOTTOM_SLACK
        + body_height;
    Size::new(POPUP_WIDTH, height.clamp(1.0, popup_max_height()))
}

pub(super) fn popup_empty_state_active(state: &AppState) -> bool {
    state.providers.iter().all(|provider| !provider.enabled)
}

pub(super) fn detected_without_accounts(
    state: &AppState,
    detection: &DetectionSnapshot,
    provider: ProviderId,
) -> bool {
    detection.detected(provider) && state.accounts_for(provider).is_empty()
}

fn body_measure_width(columns: f32) -> f32 {
    columns * POPUP_WIDTH - POPUP_PADDING - POPUP_BODY_PANEL_PADDING
}

fn selected_account_count(state: &AppState, provider: ProviderId) -> f32 {
    let n = state.display_selected_account_count(provider);
    f32::from(u8::try_from(n).unwrap_or(u8::MAX))
}

fn narrow_chrome<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(container(content.into()).width(Length::Fixed(POPUP_WIDTH)))
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .into()
}

fn panel<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::from(container(content).width(Length::Fill).padding(12))
}

fn popup_header(route: &PopupRoute, empty_state: bool) -> Element<'static, Message> {
    let mut header = row![
        widget::text(fl!("app-title")).size(22),
        cosmic::iced::widget::Space::new().width(Length::Fill),
    ]
    .align_y(Alignment::Center)
    .spacing(12);

    if matches!(route, PopupRoute::ProviderDetail) && !empty_state {
        let add_provider = widget::tooltip::tooltip(
            widget::button::standard("+").on_press(Message::ToggleProviderPicker),
            widget::text(fl!("add-account")).size(12),
            widget::tooltip::Position::Top,
        );
        header = header
            .push(add_provider)
            .push(widget::button::standard(fl!("refresh-now")).on_press(Message::RefreshNow));
    }

    header.into()
}

fn provider_picker_providers(state: &AppState, detection: &DetectionSnapshot) -> Vec<ProviderId> {
    let mut providers = ProviderId::ALL.to_vec();
    providers.sort_by_key(|provider| !detected_without_accounts(state, detection, *provider));
    providers
}

fn provider_picker_view(
    state: &AppState,
    detection: &DetectionSnapshot,
) -> Element<'static, Message> {
    let providers = provider_picker_providers(state, detection);
    let detected_count = providers
        .iter()
        .take_while(|provider| detected_without_accounts(state, detection, **provider))
        .count();
    let (detected, remaining) = providers.split_at(detected_count);
    let mut content = column![].spacing(5).width(Length::Fill);

    if !detected.is_empty() {
        content = content
            .push(widget::text(fl!("provider-picker-detected-section")).size(12))
            .push(provider_picker_tile_rows(detected, true));
    }

    if !remaining.is_empty() {
        content = content
            .push(widget::text(fl!("provider-picker-all-section")).size(12))
            .push(provider_picker_tile_rows(remaining, false));
    }

    container(content).padding(4).into()
}

fn provider_picker_tile_rows(
    providers: &[ProviderId],
    detected: bool,
) -> Element<'static, Message> {
    let mut rows = column![].spacing(8);
    for pair in providers.chunks(2) {
        let mut row = row![].spacing(8);
        for provider in pair {
            row = row.push(provider_picker_tile(*provider, detected));
        }
        if pair.len() == 1 {
            row = row.push(cosmic::iced::widget::Space::new().width(Length::FillPortion(1)));
        }
        rows = rows.push(row);
    }
    rows.into()
}

fn provider_picker_tile(provider: ProviderId, detected: bool) -> Element<'static, Message> {
    let content: Element<'static, Message> = if detected {
        column![
            provider_picker_icon(provider),
            widget::text(provider.label()).size(14),
            widget::text(fl!("provider-picker-connect-account")).size(12),
        ]
        .spacing(8)
        .width(Length::Fill)
        .into()
    } else {
        row![
            provider_picker_icon(provider),
            widget::text(provider.label()).size(14),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    };
    let height = if detected {
        PROVIDER_PICKER_TILE_HEIGHT
    } else {
        PROVIDER_PICKER_COMPACT_TILE_HEIGHT
    };
    let padding = if detected { [12, 12] } else { [4, 12] };
    let content = container(content).padding(padding);
    let content = if detected {
        content
    } else {
        content.height(Length::Fill).align_y(Alignment::Center)
    };
    widget::button::custom(content)
        .class(provider_tab_class(false))
        .width(Length::FillPortion(1))
        .height(Length::Fixed(height))
        .on_press(Message::OpenProviderPickerProvider(provider))
        .into()
}

fn provider_picker_icon(provider: ProviderId) -> Element<'static, Message> {
    widget::icon::icon(provider_icon_handle(provider, provider_icon_variant()))
        .size(22)
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0))
        .into()
}

fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::from(container(content).width(Length::Fill).padding(8))
}

fn accent_selection_fill(theme: &cosmic::Theme) -> Color {
    let cosmic = theme.cosmic();
    apply_alpha(cosmic.accent.base.into(), ACCENT_SOFT_FILL_ALPHA)
}

fn settings_block<'a>(
    title: Element<'a, Message>,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    settings_block_enabled(title, body, true)
}

fn settings_block_enabled<'a>(
    title: Element<'a, Message>,
    body: impl Into<Element<'a, Message>>,
    enabled: bool,
) -> Element<'a, Message> {
    let content = column![title, body.into()].spacing(10).width(Length::Fill);

    let outer = container(content).width(Length::Fill).padding(12);
    if enabled {
        return Element::from(outer);
    }

    Element::from(outer.style(|theme| {
        let cosmic = theme.cosmic();
        widget::container::Style {
            text_color: Some(apply_alpha(
                cosmic.background(theme.transparent).on.into(),
                0.45,
            )),
            background: Some(Background::Color(apply_alpha(
                cosmic.background(theme.transparent).component.base.into(),
                0.45,
            ))),
            border: cosmic::iced::Border {
                radius: cosmic.corner_radii.radius_s.into(),
                width: 1.0,
                color: apply_alpha(cosmic.background(theme.transparent).divider.into(), 0.45),
            },
            shadow: cosmic::iced::Shadow::default(),
            icon_color: Some(apply_alpha(
                cosmic.background(theme.transparent).on.into(),
                0.45,
            )),
            snap: true,
        }
    }))
}

fn settings_category_row(
    route: &SettingsRoute,
    update_status: &UpdateStatus,
) -> Element<'static, Message> {
    let mut tabs = vec![settings_category_tab(
        fl!("settings-general-title"),
        settings_category_icon(&SettingsRoute::General),
        matches!(route, SettingsRoute::General),
        SettingsRoute::General,
        update_available(update_status),
    )];
    for provider in ProviderId::ALL {
        let target_route = SettingsRoute::Provider(provider);
        tabs.push(settings_category_tab(
            provider.label().to_string(),
            settings_category_icon(&target_route),
            matches!(route, SettingsRoute::Provider(id) if *id == provider),
            target_route,
            false,
        ));
    }

    wrap_tab_rows(tabs)
}

fn settings_category_tab(
    label: String,
    icon: widget::icon::Handle,
    selected: bool,
    route: SettingsRoute,
    notify: bool,
) -> Element<'static, Message> {
    let icon = widget::icon::icon(icon)
        .size(PROVIDER_TAB_ICON_SIZE)
        .width(Length::Fixed(PROVIDER_TAB_ICON_LENGTH))
        .height(Length::Fixed(PROVIDER_TAB_ICON_LENGTH));
    let label: Element<'static, Message> = if notify {
        container(
            row![
                widget::text(label)
                    .size(PROVIDER_TAB_LABEL_SIZE)
                    .width(Length::Shrink)
                    .align_x(Alignment::Center),
                update_notification_dot(6.0)
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .into()
    } else {
        widget::text(label)
            .size(PROVIDER_TAB_LABEL_SIZE)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .into()
    };
    let content = container(
        column![icon, label]
            .spacing(3)
            .align_x(Alignment::Center)
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding([5, 9])
    .align_x(Alignment::Center);

    Element::from(
        widget::button::custom(content)
            .class(settings_category_tab_class(selected))
            .width(Length::FillPortion(1))
            .on_press(Message::NavigateTo(PopupRoute::Settings(route))),
    )
}

fn update_available(update_status: &UpdateStatus) -> bool {
    matches!(update_status, UpdateStatus::UpdateAvailable { .. })
}

fn settings_footer_action(update_status: &UpdateStatus) -> Element<'static, Message> {
    let target = Message::NavigateTo(PopupRoute::Settings(SettingsRoute::General));

    if !update_available(update_status) {
        return widget::button::text(fl!("settings"))
            .leading_icon(widget::icon::from_name("preferences-system-symbolic"))
            .on_press(target)
            .into();
    }

    let icon = row![
        notification_dot(6.0),
        widget::icon::icon(widget::icon::from_name("preferences-system-symbolic").into())
            .size(16)
            .width(Length::Fixed(16.0))
            .height(Length::Fixed(16.0)),
    ]
    .spacing(5)
    .align_y(Alignment::Center);
    let content = row![icon, widget::text(fl!("settings")).size(14)]
        .spacing(4)
        .align_y(Alignment::Center);

    widget::button::custom(content)
        .class(cosmic::theme::Button::Text)
        .padding([0, 8])
        .on_press(target)
        .into()
}

fn notification_dot(size: f32) -> Element<'static, Message> {
    Element::from(
        container(
            cosmic::iced::widget::Space::new()
                .width(Length::Fixed(size))
                .height(Length::Fixed(size)),
        )
        .style(move |_theme: &cosmic::Theme| widget::container::Style {
            text_color: None,
            background: Some(Background::Color(UPDATE_NOTIFICATION_DOT_COLOR)),
            border: cosmic::iced::Border {
                radius: (size / 2.0).into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: cosmic::iced::Shadow::default(),
            icon_color: None,
            snap: true,
        }),
    )
}

fn update_notification_dot(size: f32) -> Element<'static, Message> {
    widget::tooltip::tooltip(
        notification_dot(size),
        widget::text(fl!("update-dot-tooltip")).size(12),
        widget::tooltip::Position::Top,
    )
    .into()
}

fn settings_category_icon(route: &SettingsRoute) -> widget::icon::Handle {
    match route {
        SettingsRoute::General => widget::icon::from_name("preferences-system-symbolic").into(),
        SettingsRoute::Provider(provider) => {
            provider_icon_handle(*provider, provider_icon_variant())
        }
    }
}

fn settings_category_tab_class(selected: bool) -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(move |_focused, theme| {
            tab_button_style(theme, selected, ButtonInteraction::idle(false), 1.0)
        }),
        disabled: Box::new(move |theme| {
            tab_button_style(theme, selected, ButtonInteraction::idle(false), 0.45)
        }),
        hovered: Box::new(move |_focused, theme| {
            tab_button_style(theme, selected, ButtonInteraction::hover(false), 1.0)
        }),
        pressed: Box::new(move |_focused, theme| {
            tab_button_style(theme, selected, ButtonInteraction::press(false), 0.92)
        }),
    }
}

#[derive(Clone, Copy)]
struct ButtonInteraction {
    focused: bool,
    hovered: bool,
    pressed: bool,
}

impl ButtonInteraction {
    const fn idle(focused: bool) -> Self {
        Self {
            focused,
            hovered: false,
            pressed: false,
        }
    }

    const fn hover(focused: bool) -> Self {
        Self {
            focused,
            hovered: true,
            pressed: false,
        }
    }

    const fn press(focused: bool) -> Self {
        Self {
            focused,
            hovered: true,
            pressed: true,
        }
    }
}

fn provider_tab_rows(state: &AppState, selected_provider: ProviderId) -> Element<'static, Message> {
    let tabs: Vec<Element<'static, Message>> = state
        .providers
        .iter()
        .filter(|provider| provider.enabled)
        .map(|provider| provider_tab(state, provider, provider.provider == selected_provider))
        .collect();

    if tabs.len() < PROVIDER_TABS_PER_ROW {
        let mut tab_row = row![].spacing(8);
        for tab in tabs {
            tab_row = tab_row.push(tab);
        }
        return tab_row.into();
    }

    wrap_tab_rows(tabs)
}

fn wrap_tab_rows(tabs: Vec<Element<'static, Message>>) -> Element<'static, Message> {
    let mut rows = column![].spacing(PROVIDER_TAB_ROW_SPACING);
    let mut tabs = tabs.into_iter().peekable();
    while tabs.peek().is_some() {
        let mut tab_row = row![].spacing(8);
        for _ in 0..PROVIDER_TABS_PER_ROW {
            tab_row = match tabs.next() {
                Some(tab) => tab_row.push(tab),
                None => {
                    tab_row.push(cosmic::iced::widget::Space::new().width(Length::FillPortion(1)))
                }
            };
        }
        rows = rows.push(tab_row);
    }
    rows.into()
}

fn nav_rows_height(tab_count: usize) -> f32 {
    let rows = tab_count.div_ceil(PROVIDER_TABS_PER_ROW).max(1);
    let rows = f32::from(u8::try_from(rows).unwrap_or(u8::MAX));
    rows * POPUP_TAB_HEIGHT + (rows - 1.0) * PROVIDER_TAB_ROW_SPACING
}

fn enabled_provider_count(state: &AppState) -> usize {
    state
        .providers
        .iter()
        .filter(|provider| provider.enabled)
        .count()
}

fn provider_nav_height(state: &AppState) -> Option<f32> {
    let count = enabled_provider_count(state);
    (count > 1).then(|| nav_rows_height(count))
}

fn settings_nav_height() -> f32 {
    nav_rows_height(ProviderId::ALL.len() + 1)
}

fn provider_tab(
    state: &AppState,
    provider: &ProviderRuntimeState,
    selected: bool,
) -> Element<'static, Message> {
    let percents = tab_percents(state, provider);
    let icon_variant = provider_icon_variant();
    let badge = widget::icon::icon(provider_icon_handle(provider.provider, icon_variant))
        .size(PROVIDER_TAB_ICON_SIZE)
        .width(Length::Fixed(PROVIDER_TAB_ICON_LENGTH))
        .height(Length::Fixed(PROVIDER_TAB_ICON_LENGTH));
    let label = widget::text(provider.provider.label())
        .size(PROVIDER_TAB_LABEL_SIZE)
        .width(Length::Fill)
        .align_x(Alignment::Center);
    let bars = percents.into_iter().fold(column![].spacing(3), |col, pct| {
        col.push(
            progress_bar(0.0..=100.0, pct)
                .length(Length::Fill)
                .girth(Length::Fixed(4.0)),
        )
    });

    let content = container(
        column![badge, label, bars]
            .spacing(3)
            .align_x(Alignment::Center)
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding([7, 9]);

    Element::from(
        widget::button::custom(content)
            .class(provider_tab_class(selected))
            .width(Length::FillPortion(1))
            .on_press(Message::SelectProvider(provider.provider)),
    )
}

fn provider_tab_class(selected: bool) -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, theme| {
            tab_button_style(theme, selected, ButtonInteraction::idle(focused), 1.0)
        }),
        disabled: Box::new(move |theme| {
            tab_button_style(theme, selected, ButtonInteraction::idle(false), 0.45)
        }),
        hovered: Box::new(move |focused, theme| {
            tab_button_style(theme, selected, ButtonInteraction::hover(focused), 1.0)
        }),
        pressed: Box::new(move |focused, theme| {
            tab_button_style(theme, selected, ButtonInteraction::press(focused), 0.92)
        }),
    }
}

fn tab_button_style(
    theme: &cosmic::Theme,
    selected: bool,
    interaction: ButtonInteraction,
    opacity: f32,
) -> widget::button::Style {
    let cosmic = theme.cosmic();
    let mut style = widget::button::Style::new();
    let surface = &cosmic.background(theme.transparent).component;

    let background = if selected {
        if interaction.pressed {
            surface.divider.into()
        } else {
            accent_selection_fill(theme)
        }
    } else if interaction.pressed {
        surface.divider.into()
    } else if interaction.hovered {
        cosmic.background(theme.transparent).component.hover.into()
    } else {
        surface.base.into()
    };

    style.background = Some(Background::Color(apply_alpha(background, opacity)));
    style.border_radius = cosmic.corner_radii.radius_s.into();
    style.border_width = if selected { 2.0 } else { 1.0 };
    style.border_color = if selected {
        apply_alpha(cosmic.accent.base.into(), opacity)
    } else {
        apply_alpha(surface.divider.into(), opacity)
    };
    style.outline_width = if interaction.focused && selected {
        1.0
    } else {
        0.0
    };
    style.outline_color = cosmic.accent.base.into();
    style.text_color = Some(apply_alpha(surface.on.into(), opacity));
    style.icon_color = Some(apply_alpha(surface.on.into(), opacity));

    style
}

fn provider_summary(
    provider: &ProviderRuntimeState,
    detected_without_accounts: bool,
) -> Element<'static, Message> {
    let mut title = row![
        widget::icon::icon(provider_icon_handle(
            provider.provider,
            provider_icon_variant(),
        ))
        .size(24)
        .width(Length::Fixed(24.0))
        .height(Length::Fixed(24.0)),
        widget::text(provider.provider.label()).size(28),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    if detected_without_accounts {
        title = title.push(badge_accent(fl!("provider-detected-chip")));
    }

    card(title)
}

fn info_block(
    title: String,
    primary: String,
    secondary: Option<String>,
    action: Option<Element<'static, Message>>,
) -> Element<'static, Message> {
    let mut col = column![widget::text(title).size(15), widget::text(primary).size(14)].spacing(6);

    if let Some(secondary) = secondary {
        col = col.push(widget::text(secondary).size(13));
    }

    if let Some(action) = action {
        col = col.push(action);
    }

    card(col)
}

fn selected_state(
    state: &AppState,
    selected_provider: ProviderId,
) -> Option<&ProviderRuntimeState> {
    state
        .providers
        .iter()
        .find(|p| p.provider == selected_provider && p.enabled)
        .or_else(|| state.providers.iter().find(|p| p.enabled))
}

fn tab_percents(state: &AppState, provider: &ProviderRuntimeState) -> Vec<f32> {
    let now = chrono::Utc::now();
    let accounts = state.display_selected_accounts(provider.provider);
    if accounts.is_empty() {
        let pct = active_snapshot(state, provider)
            .and_then(|s| s.headline_window())
            .map_or(0.0, |w| usage_display::displayed_percent(w, now));
        return vec![pct];
    }
    accounts
        .into_iter()
        .map(|account| {
            account
                .snapshot
                .as_ref()
                .and_then(|s| s.headline_window())
                .map_or(0.0, |w| usage_display::displayed_percent(w, now))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_is_active_without_enabled_provider_tabs() {
        let mut state = AppState::empty();
        for provider in ProviderId::ALL {
            state.provider_mut(provider).unwrap().enabled = false;
        }

        assert!(popup_empty_state_active(&state));
        assert_eq!(
            popup_session_size(&state, ProviderId::Codex).width,
            POPUP_WIDTH
        );

        state.provider_mut(ProviderId::Codex).unwrap().enabled = true;

        assert!(!popup_empty_state_active(&state));
    }

    #[test]
    fn detected_settings_hint_ignores_explicit_disablement_but_hides_after_account_added() {
        let home = tempfile::tempdir().expect("create temporary home");
        std::fs::create_dir(home.path().join(".codex")).expect("create Codex marker");
        let detection = crate::detection::detect(home.path());
        let mut state = AppState::empty();
        state.provider_mut(ProviderId::Codex).unwrap().enabled = false;

        assert!(detected_without_accounts(
            &state,
            &detection,
            ProviderId::Codex
        ));

        state
            .provider_accounts
            .push(ProviderAccountRuntimeState::empty(
                ProviderId::Codex,
                "codex-test",
                "test@example.com",
            ));
        assert!(!detected_without_accounts(
            &state,
            &detection,
            ProviderId::Codex
        ));
    }

    #[test]
    fn provider_picker_lists_detected_unconfigured_providers_first() {
        let home = tempfile::tempdir().expect("create temporary home");
        std::fs::create_dir(home.path().join(".codex")).expect("create Codex marker");
        let detection = crate::detection::detect(home.path());
        let state = AppState::empty();

        assert_eq!(
            provider_picker_providers(&state, &detection),
            vec![
                ProviderId::Codex,
                ProviderId::Claude,
                ProviderId::Cursor,
                ProviderId::Antigravity,
                ProviderId::Gemini,
                ProviderId::Copilot,
                ProviderId::Minimax,
                ProviderId::OpencodeGo,
                ProviderId::OllamaCloud,
            ]
        );
    }

    #[test]
    fn provider_picker_has_room_for_the_complete_chooser() {
        assert_eq!(popup_provider_picker_size(), Size::new(POPUP_WIDTH, 680.0));
    }
}
