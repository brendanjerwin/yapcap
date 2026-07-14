// SPDX-License-Identifier: MPL-2.0

use crate::error::CopilotError;
use crate::fl;
use crate::model::{
    ProviderCost, ProviderId, ProviderIdentity, UsageHeadline, UsageSnapshot, UsageWindow,
};
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CopilotUserResponse {
    access_type_sku: Option<String>,
    login: Option<String>,
    token_based_billing: Option<bool>,
    quota_reset_date: Option<String>,
    quota_reset_date_utc: Option<String>,
    quota_snapshots: Option<QuotaSnapshots>,
}

#[derive(Debug, Deserialize)]
struct QuotaSnapshots {
    premium_interactions: Option<Quota>,
    chat: Option<Quota>,
    completions: Option<Quota>,
}

#[derive(Debug, Deserialize)]
struct Quota {
    entitlement: Option<i32>,
    remaining: Option<i32>,
    quota_remaining: Option<f64>,
    percent_remaining: Option<f32>,
    has_quota: Option<bool>,
    unlimited: Option<bool>,
    overage_count: Option<i32>,
}

pub fn parse(body: &str, updated_at: DateTime<Utc>) -> Result<UsageSnapshot, CopilotError> {
    let response: CopilotUserResponse =
        serde_json::from_str(body).map_err(CopilotError::ParseUsage)?;

    let Some(snapshots) = response.quota_snapshots.as_ref() else {
        return Err(unrecognized_response(&response));
    };

    let reset_at = resolve_reset(&response)?;
    let window_seconds = window_seconds(reset_at, updated_at);
    let token_based = response.token_based_billing == Some(true);

    let mut windows = Vec::new();
    let mut premium_headline = None;
    let mut completions_headline = None;
    for (label, quota) in ordered_quotas(snapshots) {
        let Some(quota) = quota else { continue };
        if !is_metered(quota) {
            continue;
        }
        let index = windows.len();
        windows.push(quota_window(
            window_label(label, token_based),
            quota,
            reset_at,
            window_seconds,
        ));
        match label {
            "premium_interactions" => premium_headline = Some(index),
            "completions" => completions_headline = Some(index),
            _ => {}
        }
    }

    if windows.is_empty() {
        return Err(unrecognized_response(&response));
    }

    let headline = premium_headline.or(completions_headline).unwrap_or(0);
    let premium = snapshots.premium_interactions.as_ref();
    let premium_entitlement = premium.and_then(|quota| quota.entitlement);
    let has_metered_premium = premium.is_some_and(is_metered);
    let provider_cost = (token_based && has_metered_premium)
        .then(|| credits_cost(premium))
        .flatten();

    Ok(UsageSnapshot {
        provider: ProviderId::Copilot,
        source: "Managed Account".to_string(),
        updated_at,
        headline: UsageHeadline(headline),
        windows,
        provider_cost,
        extra_usage: None,
        identity: ProviderIdentity {
            email: None,
            account_id: None,
            plan: Some(plan_badge(
                response.access_type_sku.as_deref(),
                premium_entitlement,
                token_based,
                has_metered_premium,
            )),
            display_name: response.login,
        },
    })
}

fn ordered_quotas(snapshots: &QuotaSnapshots) -> [(&'static str, &Option<Quota>); 3] {
    [
        ("premium_interactions", &snapshots.premium_interactions),
        ("chat", &snapshots.chat),
        ("completions", &snapshots.completions),
    ]
}

fn window_label(label: &str, token_based: bool) -> &str {
    if label == "premium_interactions" && token_based {
        "credits"
    } else {
        label
    }
}

fn credits_cost(quota: Option<&Quota>) -> Option<ProviderCost> {
    let quota = quota?;
    let entitlement = f64::from(quota.entitlement?);
    let remaining = quota
        .quota_remaining
        .or_else(|| quota.remaining.map(f64::from))
        .unwrap_or(0.0);
    Some(ProviderCost {
        used: (entitlement - remaining) / 100.0,
        limit: Some(entitlement / 100.0),
        units: "USD".to_string(),
    })
}

fn is_metered(quota: &Quota) -> bool {
    if quota.unlimited == Some(true) {
        return false;
    }
    match quota.has_quota {
        Some(has_quota) => has_quota,
        None => quota.entitlement.unwrap_or(0) > 0,
    }
}

fn quota_window(
    label: &str,
    quota: &Quota,
    reset_at: DateTime<Utc>,
    window_seconds: Option<i64>,
) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used_percent: used_percent(quota),
        reset_at: Some(reset_at),
        window_seconds,
        reset_description: quota
            .overage_count
            .filter(|count| *count > 0)
            .map(overage_description),
        group: None,
    }
}

fn used_percent(quota: &Quota) -> f32 {
    if let Some(percent_remaining) = quota.percent_remaining {
        return (100.0 - percent_remaining).clamp(0.0, 100.0);
    }
    let entitlement = quota.entitlement.unwrap_or(0) as f32;
    if entitlement <= 0.0 {
        return 0.0;
    }
    let remaining = quota.remaining.unwrap_or(0) as f32;
    ((entitlement - remaining) / entitlement * 100.0).clamp(0.0, 100.0)
}

fn resolve_reset(response: &CopilotUserResponse) -> Result<DateTime<Utc>, CopilotError> {
    if let Some(value) = response.quota_reset_date_utc.as_deref() {
        return parse_reset_date(value);
    }
    if let Some(value) = response.quota_reset_date.as_deref() {
        return parse_reset_date(value);
    }
    Err(unrecognized("missing quota_reset_date_utc"))
}

fn window_seconds(reset_at: DateTime<Utc>, now: DateTime<Utc>) -> Option<i64> {
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
    match response.token_based_billing {
        Some(value) => details.push(format!("token_based_billing={value}")),
        None => details.push("token_based_billing=absent".to_string()),
    }
    unrecognized(details.join(", "))
}

fn plan_badge(
    access_type_sku: Option<&str>,
    premium_entitlement: Option<i32>,
    token_based: bool,
    has_metered_premium: bool,
) -> String {
    match access_type_sku {
        Some("free_limited_copilot") => return "Free".to_string(),
        Some("plus_monthly_subscriber_quota") => return "Pro+".to_string(),
        Some("copilot_standalone_seat_quota") => return "Business".to_string(),
        _ => {}
    }
    if token_based {
        return token_based_badge(premium_entitlement, has_metered_premium);
    }
    match premium_entitlement {
        Some(300) => "Pro".to_string(),
        Some(1500) => "Pro+".to_string(),
        _ => "Plan".to_string(),
    }
}

fn token_based_badge(premium_entitlement: Option<i32>, has_metered_premium: bool) -> String {
    if !has_metered_premium {
        return "Plan".to_string();
    }
    match premium_entitlement {
        Some(entitlement) if entitlement <= 2_000 => "Pro".to_string(),
        Some(entitlement) if entitlement <= 10_000 => "Pro+".to_string(),
        Some(_) => "Max".to_string(),
        None => "Plan".to_string(),
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
