// SPDX-License-Identifier: MPL-2.0

use super::*;
use chrono::TimeZone;

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
    let now = Utc.with_ymd_and_hms(2026, 7, 8, 12, 0, 0).unwrap();
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
    assert_eq!(snapshot.windows[1].label, "completions");
    assert!(
        !snapshot
            .windows
            .iter()
            .any(|window| window.label == "premium_interactions")
    );
    assert!((snapshot.windows[0].used_percent - 0.0).abs() < 0.001);
    assert!((snapshot.windows[1].used_percent - 0.0).abs() < 0.001);

    let reset = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
    assert_eq!(snapshot.windows[0].reset_at, Some(reset));
    assert_eq!(snapshot.windows[1].reset_at, Some(reset));
    let start = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
    let expected = (reset - start).num_seconds();
    assert_eq!(snapshot.windows[0].window_seconds, Some(expected));
    assert_eq!(snapshot.windows[1].window_seconds, Some(expected));
}

#[test]
fn free_missing_percent_remaining_falls_back_to_remaining_over_entitlement() {
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "login": "casey",
        "quota_reset_date_utc": "2026-08-01T00:00:00.000Z",
        "quota_snapshots": {
            "chat": {
                "entitlement": 200,
                "remaining": 50,
                "has_quota": true,
                "unlimited": false
            },
            "completions": {
                "entitlement": 2000,
                "remaining": 2000,
                "has_quota": true,
                "unlimited": false
            }
        }
    }"#;
    let snapshot = parse(body, Utc::now()).unwrap();

    assert_eq!(snapshot.windows.len(), 2);
    assert!((snapshot.windows[0].used_percent - 75.0).abs() < 0.001);
    assert!((snapshot.windows[1].used_percent - 0.0).abs() < 0.001);
}

#[test]
fn clamps_metered_usage_percent() {
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "quota_reset_date_utc": "2026-08-01T00:00:00.000Z",
        "quota_snapshots": {
            "chat": {
                "entitlement": 0,
                "remaining": 10,
                "has_quota": true,
                "unlimited": false
            },
            "completions": {
                "entitlement": 300,
                "remaining": -1,
                "has_quota": true,
                "unlimited": false
            }
        }
    }"#;
    let snapshot = parse(body, Utc::now()).unwrap();

    assert_eq!(snapshot.windows[0].used_percent, 0.0);
    assert_eq!(snapshot.windows[1].used_percent, 100.0);
}

#[test]
fn unmetered_quotas_are_skipped() {
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "quota_reset_date_utc": "2026-08-01T00:00:00.000Z",
        "quota_snapshots": {
            "premium_interactions": {
                "entitlement": 0,
                "has_quota": false,
                "unlimited": false
            },
            "chat": {
                "entitlement": 200,
                "percent_remaining": 90,
                "has_quota": true,
                "unlimited": false
            },
            "completions": {
                "entitlement": 0,
                "percent_remaining": 100,
                "unlimited": true
            }
        }
    }"#;
    let snapshot = parse(body, Utc::now()).unwrap();

    assert_eq!(snapshot.windows.len(), 1);
    assert_eq!(snapshot.windows[0].label, "chat");
    assert_eq!(snapshot.headline, UsageHeadline(0));
}

#[test]
fn quota_reset_date_utc_is_preferred_over_date_only() {
    let now = Utc.with_ymd_and_hms(2026, 7, 8, 0, 0, 0).unwrap();
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "quota_reset_date": "2026-08-01",
        "quota_reset_date_utc": "2026-08-15T06:30:00.000Z",
        "quota_snapshots": {
            "chat": {
                "entitlement": 200,
                "percent_remaining": 100,
                "has_quota": true,
                "unlimited": false
            }
        }
    }"#;
    let snapshot = parse(body, now).unwrap();

    assert_eq!(
        snapshot.windows[0].reset_at,
        Some(Utc.with_ymd_and_hms(2026, 8, 15, 6, 30, 0).unwrap())
    );
}

#[test]
fn expired_reset_leaves_window_seconds_none() {
    let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "quota_reset_date_utc": "2026-08-01T00:00:00.000Z",
        "quota_snapshots": {
            "chat": {
                "entitlement": 200,
                "percent_remaining": 100,
                "has_quota": true,
                "unlimited": false
            }
        }
    }"#;
    let snapshot = parse(body, now).unwrap();

    assert_eq!(snapshot.windows[0].window_seconds, None);
}

#[test]
fn missing_quota_snapshots_is_unrecognized_with_token_billing_detail() {
    let body = r#"{
        "access_type_sku": "free_limited_copilot",
        "login": "casey",
        "token_based_billing": true
    }"#;

    assert_unrecognized(
        parse(body, Utc::now()),
        "access_type_sku=free_limited_copilot, login=casey, token_based_billing=true",
    );
}

#[test]
fn unknown_shape_is_unrecognized() {
    let body = r#"{"access_type_sku": "mystery"}"#;

    assert_unrecognized(
        parse(body, Utc::now()),
        "access_type_sku=mystery, token_based_billing=absent",
    );
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
        "access_type_sku=no_access, login=tamascsarno, token_based_billing=absent",
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

#[test]
fn synthetic_token_based_pro_plus_fixture_renders_credits_window_and_cost_card() {
    let body = fixture_body_from(include_str!(
        "../../../../fixtures/copilot/copilot_user_pro_plus_token_response.json"
    ));
    let snapshot = parse(&body, Utc::now()).unwrap();

    assert_eq!(snapshot.identity.plan.as_deref(), Some("Pro+"));
    assert_eq!(snapshot.windows.len(), 1);
    assert_eq!(snapshot.windows[0].label, "credits");
    assert!((snapshot.windows[0].used_percent - 40.0).abs() < 0.001);

    let cost = snapshot.provider_cost.expect("token-based paid cost card");
    assert!((cost.used - 28.0).abs() < 0.001);
    assert_eq!(cost.limit, Some(70.0));
    assert_eq!(cost.units, "USD");
}

#[test]
fn token_based_cost_card_falls_back_to_integer_remaining() {
    let body = r#"{
        "access_type_sku": "unknown_paid_sku",
        "token_based_billing": true,
        "quota_reset_date": "2026-08-01",
        "quota_snapshots": {
            "premium_interactions": {
                "entitlement": 7000,
                "remaining": 4200,
                "percent_remaining": 60,
                "unlimited": false,
                "overage_permitted": true
            }
        }
    }"#;
    let snapshot = parse(body, Utc::now()).unwrap();

    assert_eq!(snapshot.windows[0].label, "credits");
    let cost = snapshot.provider_cost.expect("integer remaining cost card");
    assert!((cost.used - 28.0).abs() < 0.001);
    assert_eq!(cost.limit, Some(70.0));
}

#[test]
fn pre_migration_paid_keeps_premium_label_and_no_cost_card() {
    let snapshot = parse(&paid_overage_body(None), Utc::now()).unwrap();

    assert_eq!(snapshot.windows[0].label, "premium_interactions");
    assert_eq!(snapshot.provider_cost, None);
}

#[test]
fn free_capture_has_no_cost_card() {
    let now = Utc.with_ymd_and_hms(2026, 7, 8, 12, 0, 0).unwrap();
    let snapshot = parse(&fixture_body("body_json"), now).unwrap();

    assert_eq!(snapshot.provider_cost, None);
    assert!(
        !snapshot
            .windows
            .iter()
            .any(|window| window.label == "credits")
    );
}

#[test]
fn token_based_overage_still_renders_over_plan() {
    let body = r#"{
        "access_type_sku": "unknown_paid_sku",
        "token_based_billing": true,
        "quota_reset_date": "2026-08-01",
        "quota_snapshots": {
            "premium_interactions": {
                "entitlement": 7000,
                "remaining": 0,
                "quota_remaining": 0.0,
                "percent_remaining": 0,
                "overage_count": 12,
                "unlimited": false,
                "overage_permitted": true
            }
        }
    }"#;
    let snapshot = parse(body, Utc::now()).unwrap();

    assert_eq!(snapshot.windows[0].label, "credits");
    assert_eq!(
        snapshot.windows[0].reset_description.as_deref(),
        Some("+12 over plan")
    );
}

#[test]
fn token_based_unknown_sku_uses_entitlement_ranges() {
    for (entitlement, expected) in [
        (1500, "Pro"),
        (2000, "Pro"),
        (7000, "Pro+"),
        (10000, "Pro+"),
        (20000, "Max"),
    ] {
        let snapshot = parse(&token_based_unknown_sku_body(entitlement), Utc::now()).unwrap();
        assert_eq!(
            snapshot.identity.plan.as_deref(),
            Some(expected),
            "entitlement {entitlement}"
        );
    }
}

#[test]
fn token_based_known_sku_keeps_sku_badge_regardless_of_entitlement() {
    let body = r#"{
        "access_type_sku": "plus_monthly_subscriber_quota",
        "token_based_billing": true,
        "login": "morgan",
        "quota_reset_date": "2026-08-01",
        "quota_snapshots": {
            "premium_interactions": {
                "entitlement": 20000,
                "remaining": 10000,
                "percent_remaining": 50,
                "unlimited": false,
                "overage_permitted": true
            }
        }
    }"#;
    let snapshot = parse(body, Utc::now()).unwrap();

    assert_eq!(snapshot.identity.plan.as_deref(), Some("Pro+"));
}

#[test]
fn token_based_without_metered_premium_falls_back_to_plan() {
    let body = r#"{
        "access_type_sku": "unknown_paid_sku",
        "token_based_billing": true,
        "quota_reset_date": "2026-08-01",
        "quota_snapshots": {
            "chat": { "entitlement": 200, "percent_remaining": 90, "has_quota": true, "unlimited": false },
            "completions": { "entitlement": 2000, "percent_remaining": 90, "has_quota": true, "unlimited": false },
            "premium_interactions": { "entitlement": 0, "has_quota": false, "unlimited": false }
        }
    }"#;
    let snapshot = parse(body, Utc::now()).unwrap();

    assert_eq!(snapshot.identity.plan.as_deref(), Some("Plan"));
}

fn token_based_unknown_sku_body(entitlement: i32) -> String {
    format!(
        r#"{{
            "access_type_sku": "unknown_paid_sku",
            "token_based_billing": true,
            "login": "morgan",
            "quota_reset_date": "2026-08-01",
            "quota_snapshots": {{
                "chat": {{ "entitlement": 0, "percent_remaining": 100, "unlimited": true }},
                "completions": {{ "entitlement": 0, "percent_remaining": 100, "unlimited": true }},
                "premium_interactions": {{
                    "entitlement": {entitlement},
                    "remaining": 100,
                    "percent_remaining": 50,
                    "unlimited": false,
                    "overage_permitted": true
                }}
            }}
        }}"#
    )
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
                    "unlimited": false,
                    "overage_permitted": true
                    {overage}
                }}
            }}
        }}"#
    )
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
                "unlimited": false,
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
                "unlimited": false,
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
        "access_type_sku": "plus_monthly_subscriber_quota",
        "login": "morgan",
        "quota_reset_date": "2026-06-03",
        "quota_snapshots": {
            "premium_interactions": {
                "entitlement": 1500,
                "remaining": 0,
                "percent_remaining": 0,
                "unlimited": false,
                "overage_permitted": true
            }
        }
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
                    "unlimited": false,
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
