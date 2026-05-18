// SPDX-License-Identifier: MPL-2.0

use crate::error::CopilotError;
use crate::fl;
use crate::model::{ProviderId, ProviderIdentity, UsageHeadline, UsageSnapshot, UsageWindow};
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use serde::Deserialize;

const FREE_WINDOW_SECONDS: i64 = 30 * 24 * 3600;

#[derive(Debug, Deserialize)]
struct CopilotUserResponse {
    access_type_sku: Option<String>,
    login: Option<String>,
    monthly_quotas: Option<FreeQuotas>,
    limited_user_quotas: Option<FreeQuotas>,
    limited_user_reset_date: Option<String>,
    quota_reset_date: Option<String>,
    quota_snapshots: Option<PaidQuotaSnapshots>,
}

#[derive(Debug, Deserialize)]
struct FreeQuotas {
    chat: Option<f32>,
    completions: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct PaidQuotaSnapshots {
    premium_interactions: Option<PaidQuota>,
}

#[derive(Debug, Deserialize)]
struct PaidQuota {
    entitlement: Option<i32>,
    remaining: Option<i32>,
    percent_remaining: Option<f32>,
    overage_permitted: Option<bool>,
    overage_count: Option<i32>,
}

pub fn parse(body: &str, updated_at: DateTime<Utc>) -> Result<UsageSnapshot, CopilotError> {
    let response: CopilotUserResponse =
        serde_json::from_str(body).map_err(CopilotError::ParseUsage)?;

    if response.access_type_sku.as_deref() == Some("free_limited_copilot") {
        return parse_free(response, updated_at);
    }

    if response.quota_snapshots.is_some() {
        return parse_paid(response, updated_at);
    }

    Err(unrecognized_response(&response))
}

fn parse_free(
    response: CopilotUserResponse,
    updated_at: DateTime<Utc>,
) -> Result<UsageSnapshot, CopilotError> {
    let monthly = response
        .monthly_quotas
        .ok_or_else(|| unrecognized("missing monthly_quotas"))?;
    let remaining = response
        .limited_user_quotas
        .ok_or_else(|| unrecognized("missing limited_user_quotas"))?;
    let reset_at = response
        .limited_user_reset_date
        .as_deref()
        .map(parse_reset_date)
        .transpose()?
        .ok_or_else(|| unrecognized("missing limited_user_reset_date"))?;

    let window_seconds = free_window_seconds(reset_at, updated_at);
    let windows = vec![
        free_window(
            "chat",
            monthly
                .chat
                .ok_or_else(|| unrecognized("missing monthly_quotas.chat"))?,
            remaining
                .chat
                .ok_or_else(|| unrecognized("missing limited_user_quotas.chat"))?,
            reset_at,
            window_seconds,
        ),
        free_window(
            "completions",
            monthly
                .completions
                .ok_or_else(|| unrecognized("missing monthly_quotas.completions"))?,
            remaining
                .completions
                .ok_or_else(|| unrecognized("missing limited_user_quotas.completions"))?,
            reset_at,
            window_seconds,
        ),
    ];

    Ok(UsageSnapshot {
        provider: ProviderId::Copilot,
        source: "Managed Account".to_string(),
        updated_at,
        headline: UsageHeadline(1),
        windows,
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity {
            email: None,
            account_id: None,
            plan: Some(plan_badge(response.access_type_sku.as_deref(), None, false)),
            display_name: response.login,
        },
    })
}

fn parse_paid(
    response: CopilotUserResponse,
    updated_at: DateTime<Utc>,
) -> Result<UsageSnapshot, CopilotError> {
    let snapshots = response
        .quota_snapshots
        .ok_or_else(|| unrecognized("missing quota_snapshots"))?;
    let premium = snapshots
        .premium_interactions
        .ok_or_else(|| unrecognized("missing quota_snapshots.premium_interactions"))?;
    let reset_at = response
        .quota_reset_date
        .as_deref()
        .map(parse_reset_date)
        .transpose()?
        .ok_or_else(|| unrecognized("missing quota_reset_date"))?;
    let entitlement = premium
        .entitlement
        .ok_or_else(|| unrecognized("missing premium_interactions.entitlement"))?;
    let remaining = premium
        .remaining
        .ok_or_else(|| unrecognized("missing premium_interactions.remaining"))?;
    let percent_remaining = premium
        .percent_remaining
        .ok_or_else(|| unrecognized("missing premium_interactions.percent_remaining"))?;
    let _ = (remaining, premium.overage_permitted);

    Ok(UsageSnapshot {
        provider: ProviderId::Copilot,
        source: "Managed Account".to_string(),
        updated_at,
        headline: UsageHeadline(0),
        windows: vec![paid_window(
            "premium_interactions",
            percent_remaining,
            reset_at,
            premium.overage_count,
            paid_window_seconds(reset_at, updated_at),
        )],
        provider_cost: None,
        extra_usage: None,
        identity: ProviderIdentity {
            email: None,
            account_id: None,
            plan: Some(plan_badge(
                response.access_type_sku.as_deref(),
                Some(entitlement),
                true,
            )),
            display_name: response.login,
        },
    })
}

fn free_window(
    label: &str,
    entitlement: f32,
    remaining: f32,
    reset_at: DateTime<Utc>,
    window_seconds: Option<i64>,
) -> UsageWindow {
    let used_percent = if entitlement <= 0.0 {
        0.0
    } else {
        ((entitlement - remaining) / entitlement * 100.0).clamp(0.0, 100.0)
    };
    UsageWindow {
        label: label.to_string(),
        used_percent,
        reset_at: Some(reset_at),
        window_seconds,
        reset_description: Some(reset_at.to_rfc3339()),
    }
}

fn paid_window(
    label: &str,
    percent_remaining: f32,
    reset_at: DateTime<Utc>,
    overage_count: Option<i32>,
    window_seconds: Option<i64>,
) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used_percent: (100.0 - percent_remaining).clamp(0.0, 100.0),
        reset_at: Some(reset_at),
        window_seconds,
        reset_description: overage_count
            .filter(|count| *count > 0)
            .map(overage_description),
    }
}

fn free_window_seconds(reset_at: DateTime<Utc>, now: DateTime<Utc>) -> Option<i64> {
    if reset_at <= now {
        return None;
    }
    Some(FREE_WINDOW_SECONDS)
}

fn paid_window_seconds(reset_at: DateTime<Utc>, now: DateTime<Utc>) -> Option<i64> {
    if reset_at <= now {
        return None;
    }
    let start = previous_month_boundary(reset_at)?;
    let secs = (reset_at - start).num_seconds();
    (secs > 0).then_some(secs)
}

fn previous_month_boundary(reset_at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let same_month_start = Utc
        .with_ymd_and_hms(reset_at.year(), reset_at.month(), 1, 0, 0, 0)
        .single()?;
    if same_month_start < reset_at {
        return Some(same_month_start);
    }
    let (year, month) = if reset_at.month() == 1 {
        (reset_at.year() - 1, 12)
    } else {
        (reset_at.year(), reset_at.month() - 1)
    };
    Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0).single()
}

fn overage_description(count: i32) -> String {
    fl!("copilot-overage-over-plan", count = count).replace(['\u{2068}', '\u{2069}'], "")
}

fn unrecognized(detail: impl Into<String>) -> CopilotError {
    CopilotError::UnrecognizedResponse {
        detail: detail.into(),
    }
}

fn unrecognized_response(response: &CopilotUserResponse) -> CopilotError {
    let mut details = Vec::new();
    if let Some(access_type_sku) = response.access_type_sku.as_deref() {
        details.push(format!("access_type_sku={access_type_sku}"));
    }
    if let Some(login) = response.login.as_deref() {
        details.push(format!("login={login}"));
    }
    if details.is_empty() {
        details.push("missing quota fields".to_string());
    }
    unrecognized(details.join(", "))
}

fn plan_badge(
    access_type_sku: Option<&str>,
    premium_entitlement: Option<i32>,
    has_quota_snapshots: bool,
) -> String {
    match access_type_sku {
        Some("free_limited_copilot") => "Free".to_string(),
        Some("plus_monthly_subscriber_quota") => "Pro+".to_string(),
        Some("copilot_standalone_seat_quota") => "Business".to_string(),
        _ if has_quota_snapshots => match premium_entitlement {
            Some(300) => "Pro".to_string(),
            Some(1500) => "Pro+".to_string(),
            _ => "Plan".to_string(),
        },
        _ => "Plan".to_string(),
    }
}

fn parse_reset_date(value: &str) -> Result<DateTime<Utc>, CopilotError> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Ok(parsed.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|source| {
        CopilotError::InvalidResetDate {
            value: value.to_string(),
            source,
        }
    })?;
    Ok(Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap()))
}

#[cfg(test)]
mod tests;
