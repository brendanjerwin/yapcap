// SPDX-License-Identifier: MPL-2.0

use super::*;
use chrono::{Datelike, TimeZone};

fn fixture_body(name: &str) -> String {
    let envelope: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../fixtures/copilot/copilot_user_response.json"
    ))
    .unwrap();
    envelope[name].to_string()
}

fn fixture_body_from(raw: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(raw).unwrap();
    value.get("body_json").unwrap_or(&value).to_string()
}

#[test]
fn parses_free_tier_fixture() {
    let now = Utc.timestamp_opt(1_770_000_000, 0).single().unwrap();
    let snapshot = parse(&fixture_body("body_json"), now).unwrap();

    assert_eq!(snapshot.provider, ProviderId::Copilot);
    assert_eq!(snapshot.source, "Managed Account");
    assert_eq!(snapshot.updated_at, now);
    assert_eq!(snapshot.headline, UsageHeadline(1));
    assert_eq!(snapshot.identity.email, None);
    assert_eq!(
        snapshot.identity.display_name.as_deref(),
        Some("exampleuser")
    );
    assert_eq!(snapshot.identity.plan.as_deref(), Some("Free"));
    assert_eq!(snapshot.windows.len(), 2);
    assert_eq!(snapshot.windows[0].label, "chat");
    assert!((snapshot.windows[0].used_percent - 4.0).abs() < 0.001);
    assert_eq!(snapshot.windows[1].label, "completions");
    assert!((snapshot.windows[1].used_percent - 0.0).abs() < 0.001);
    let reset = snapshot.windows[0].reset_at.unwrap();
    assert_eq!(reset.year(), 2026);
    assert_eq!(reset.month(), 6);
    assert_eq!(reset.day(), 3);
    assert_eq!(snapshot.windows[1].reset_at, Some(reset));
}

#[test]
fn parses_varying_free_entitlements() {
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "login": "casey",
        "monthly_quotas": { "chat": 500, "completions": 300 },
        "limited_user_quotas": { "chat": 350, "completions": 60 },
        "limited_user_reset_date": "2026-06-03"
    }"#;
    let snapshot = parse(body, Utc::now()).unwrap();

    assert!((snapshot.windows[0].used_percent - 30.0).abs() < 0.001);
    assert!((snapshot.windows[1].used_percent - 80.0).abs() < 0.001);
}

#[test]
fn clamps_free_usage_percent() {
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "monthly_quotas": { "chat": 0, "completions": 300 },
        "limited_user_quotas": { "chat": 10, "completions": -1 },
        "limited_user_reset_date": "2026-06-03"
    }"#;
    let snapshot = parse(body, Utc::now()).unwrap();

    assert_eq!(snapshot.windows[0].used_percent, 0.0);
    assert_eq!(snapshot.windows[1].used_percent, 100.0);
}

#[test]
fn missing_free_fields_are_unrecognized() {
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "monthly_quotas": { "chat": 500 },
        "limited_user_quotas": { "chat": 350 },
        "limited_user_reset_date": "2026-06-03"
    }"#;

    assert_unrecognized(
        parse(body, Utc::now()),
        "missing monthly_quotas.completions",
    );
}

#[test]
fn unknown_shape_is_unrecognized() {
    let body = r#"{"access_type_sku": "mystery"}"#;

    assert_unrecognized(parse(body, Utc::now()), "access_type_sku=mystery");
}

#[test]
fn no_access_shape_is_unrecognized_with_access_type_detail() {
    let body = r#"{
        "access_type_sku": "no_access",
        "login": "tamascsarno",
        "copilot_plan": "individual",
        "can_signup_for_limited": true,
        "chat_enabled": false,
        "cli_enabled": false
    }"#;

    assert_unrecognized(
        parse(body, Utc::now()),
        "access_type_sku=no_access, login=tamascsarno",
    );
}

#[test]
fn parses_pro_plus_paid_tier_fixture() {
    let body = fixture_body_from(include_str!(
        "../../../../fixtures/copilot/copilot_user_pro_plus_response.json"
    ));
    let snapshot = parse(&body, Utc::now()).unwrap();

    assert_eq!(snapshot.headline, UsageHeadline(0));
    assert_eq!(snapshot.identity.plan.as_deref(), Some("Pro+"));
    assert_eq!(snapshot.windows.len(), 1);
    assert_eq!(snapshot.windows[0].label, "premium_interactions");
    assert!((snapshot.windows[0].used_percent - 11.496).abs() < 0.001);
    assert_eq!(
        snapshot.windows[0].reset_at.unwrap(),
        Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap()
    );
}

#[test]
fn parses_business_paid_tier_fixture() {
    let body = fixture_body_from(include_str!(
        "../../../../fixtures/copilot/copilot_user_business_response.json"
    ));
    let snapshot = parse(&body, Utc::now()).unwrap();

    assert_eq!(snapshot.headline, UsageHeadline(0));
    assert_eq!(snapshot.identity.plan.as_deref(), Some("Business"));
    assert_eq!(snapshot.windows.len(), 1);
    assert_eq!(snapshot.windows[0].label, "premium_interactions");
    assert!((snapshot.windows[0].used_percent - 68.833_336).abs() < 0.001);
    assert_eq!(
        snapshot.windows[0].reset_at.unwrap(),
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
    );
}

#[test]
fn paid_overage_count_adds_overage_text() {
    let snapshot = parse(&paid_overage_body(Some(42)), Utc::now()).unwrap();

    assert_eq!(
        snapshot.windows[0].reset_description.as_deref(),
        Some("+42 over plan")
    );
}

#[test]
fn paid_zero_overage_count_has_no_overage_text() {
    let snapshot = parse(&paid_overage_body(Some(0)), Utc::now()).unwrap();

    assert_eq!(snapshot.windows[0].reset_description, None);
}

#[test]
fn paid_missing_overage_count_has_no_overage_text() {
    let snapshot = parse(&paid_overage_body(None), Utc::now()).unwrap();

    assert_eq!(snapshot.windows[0].reset_description, None);
}

#[test]
fn maps_unknown_paid_sku_by_entitlement() {
    let snapshot = parse(&unknown_paid_sku_body(300), Utc::now()).unwrap();
    assert_eq!(snapshot.identity.plan.as_deref(), Some("Pro"));

    let snapshot = parse(&unknown_paid_sku_body(1500), Utc::now()).unwrap();
    assert_eq!(snapshot.identity.plan.as_deref(), Some("Pro+"));

    let snapshot = parse(&unknown_paid_sku_body(999), Utc::now()).unwrap();
    assert_eq!(snapshot.identity.plan.as_deref(), Some("Plan"));
}

fn paid_overage_body(overage_count: Option<i32>) -> String {
    let overage = overage_count.map_or(String::new(), |count| {
        format!(r#","overage_count": {count}"#)
    });
    format!(
        r#"{{
            "access_type_sku": "plus_monthly_subscriber_quota",
            "login": "morgan",
            "quota_reset_date": "2026-06-03",
            "quota_snapshots": {{
                "premium_interactions": {{
                    "entitlement": 1500,
                    "remaining": 0,
                    "percent_remaining": 0,
                    "quota_id": "premium_interactions",
                    "timestamp_utc": "2026-05-18T00:00:00Z",
                    "overage_permitted": true
                    {overage}
                }}
            }}
        }}"#
    )
}

#[test]
fn free_inferred_window_seconds_is_thirty_days_when_reset_is_future() {
    let now = Utc.with_ymd_and_hms(2026, 5, 18, 0, 0, 0).unwrap();
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "monthly_quotas": { "chat": 500, "completions": 300 },
        "limited_user_quotas": { "chat": 350, "completions": 60 },
        "limited_user_reset_date": "2026-06-03"
    }"#;
    let snapshot = parse(body, now).unwrap();
    assert_eq!(snapshot.windows[0].window_seconds, Some(30 * 24 * 3600));
    assert_eq!(snapshot.windows[1].window_seconds, Some(30 * 24 * 3600));
}

#[test]
fn free_expired_reset_leaves_window_seconds_none() {
    let now = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "monthly_quotas": { "chat": 500, "completions": 300 },
        "limited_user_quotas": { "chat": 350, "completions": 60 },
        "limited_user_reset_date": "2026-06-03"
    }"#;
    let snapshot = parse(body, now).unwrap();
    assert_eq!(snapshot.windows[0].window_seconds, None);
    assert_eq!(snapshot.windows[1].window_seconds, None);
}

#[test]
fn paid_premium_window_seconds_uses_prior_month_boundary() {
    let now = Utc.with_ymd_and_hms(2026, 1, 15, 0, 0, 0).unwrap();
    let snapshot = parse(&paid_overage_body(None), now).unwrap();
    let reset_at = snapshot.windows[0].reset_at.unwrap();
    let start = Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();
    let expected = (reset_at - start).num_seconds();
    assert_eq!(snapshot.windows[0].window_seconds, Some(expected));
}

#[test]
fn paid_premium_reset_on_month_start_uses_previous_full_month() {
    let now = Utc.with_ymd_and_hms(2026, 1, 15, 0, 0, 0).unwrap();
    let body = r#"{
        "access_type_sku": "plus_monthly_subscriber_quota",
        "login": "morgan",
        "quota_reset_date": "2026-02-01",
        "quota_snapshots": {
            "premium_interactions": {
                "entitlement": 1500,
                "remaining": 0,
                "percent_remaining": 0,
                "overage_permitted": true
            }
        }
    }"#;
    let snapshot = parse(body, now).unwrap();
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let reset_at = snapshot.windows[0].reset_at.unwrap();
    assert_eq!(
        snapshot.windows[0].window_seconds,
        Some((reset_at - start).num_seconds())
    );
}

#[test]
fn paid_premium_january_boundary_wraps_to_previous_year() {
    let now = Utc.with_ymd_and_hms(2025, 12, 15, 0, 0, 0).unwrap();
    let body = r#"{
        "access_type_sku": "plus_monthly_subscriber_quota",
        "login": "morgan",
        "quota_reset_date": "2026-01-01",
        "quota_snapshots": {
            "premium_interactions": {
                "entitlement": 1500,
                "remaining": 0,
                "percent_remaining": 0,
                "overage_permitted": true
            }
        }
    }"#;
    let snapshot = parse(body, now).unwrap();
    let start = Utc.with_ymd_and_hms(2025, 12, 1, 0, 0, 0).unwrap();
    let reset_at = snapshot.windows[0].reset_at.unwrap();
    assert_eq!(
        snapshot.windows[0].window_seconds,
        Some((reset_at - start).num_seconds())
    );
}

#[test]
fn paid_premium_expired_reset_leaves_window_seconds_none() {
    let now = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
    let snapshot = parse(&paid_overage_body(None), now).unwrap();
    assert_eq!(snapshot.windows[0].window_seconds, None);
}

#[test]
fn date_only_reset_is_parsed_as_utc_midnight() {
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "monthly_quotas": { "chat": 500, "completions": 300 },
        "limited_user_quotas": { "chat": 350, "completions": 60 },
        "limited_user_reset_date": "2026-06-03"
    }"#;
    let now = Utc.with_ymd_and_hms(2026, 5, 18, 0, 0, 0).unwrap();
    let snapshot = parse(body, now).unwrap();
    let reset_at = snapshot.windows[0].reset_at.unwrap();
    assert_eq!(reset_at, Utc.with_ymd_and_hms(2026, 6, 3, 0, 0, 0).unwrap());
}

fn unknown_paid_sku_body(entitlement: i32) -> String {
    format!(
        r#"{{
            "access_type_sku": "unknown_paid_sku",
            "login": "morgan",
            "quota_reset_date": "2026-06-03",
            "quota_snapshots": {{
                "chat": {{ "entitlement": 0, "percent_remaining": 100, "unlimited": true }},
                "completions": {{ "entitlement": 0, "percent_remaining": 100, "unlimited": true }},
                "premium_interactions": {{
                    "entitlement": {entitlement},
                    "remaining": 42,
                    "percent_remaining": 14,
                    "quota_remaining": 42.9,
                    "quota_id": "premium_interactions",
                    "timestamp_utc": "2026-05-18T00:00:00Z",
                    "overage_permitted": true,
                    "overage_count": 7
                }}
            }}
        }}"#
    )
}

fn assert_unrecognized(result: Result<UsageSnapshot, CopilotError>, expected_detail: &str) {
    match result {
        Err(CopilotError::UnrecognizedResponse { detail }) => {
            assert_eq!(detail, expected_detail);
        }
        other => panic!("expected unrecognized response, got {other:?}"),
    }
}
