// SPDX-License-Identifier: MPL-2.0

use serde::Deserialize;

use crate::error::ClaudeError;
use crate::model::UsageWindow;

use super::{SEVEN_DAY_SECONDS, parse_reset_timestamp};

#[derive(Debug, Deserialize)]
pub(super) struct ClaudeLimit {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub percent: Option<f32>,
    #[serde(default)]
    pub resets_at: Option<String>,
    #[serde(default)]
    pub scope: Option<ClaudeLimitScope>,
    #[serde(default)]
    #[allow(dead_code)]
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ClaudeLimitScope {
    #[serde(default)]
    pub model: Option<ClaudeLimitScopeModel>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ClaudeLimitScopeModel {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

pub(super) fn scoped_weekly_windows(
    limits: &[ClaudeLimit],
) -> Result<Vec<UsageWindow>, ClaudeError> {
    let mut windows = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for limit in limits {
        let Some(display_name) = scoped_display_name(limit) else {
            continue;
        };
        let Some(used_percent) = limit.percent else {
            continue;
        };
        let key = dedup_key(limit, display_name);
        if seen.contains(&key) {
            continue;
        }
        let reset_at = parse_reset_timestamp(limit.resets_at.as_deref())?;
        seen.push(key);
        windows.push(UsageWindow {
            label: display_name.to_string(),
            used_percent,
            reset_at,
            window_seconds: Some(SEVEN_DAY_SECONDS),
            reset_description: reset_at.map(|dt| dt.to_rfc3339()),
            group: None,
        });
    }
    Ok(windows)
}

fn scoped_display_name(limit: &ClaudeLimit) -> Option<&str> {
    if limit.group.as_deref() != Some("weekly") || limit.kind.as_deref() != Some("weekly_scoped") {
        return None;
    }
    limit
        .scope
        .as_ref()
        .and_then(|scope| scope.model.as_ref())
        .and_then(|model| model.display_name.as_deref())
        .filter(|name| !name.is_empty())
}

fn dedup_key(limit: &ClaudeLimit, display_name: &str) -> String {
    let id = limit
        .scope
        .as_ref()
        .and_then(|scope| scope.model.as_ref())
        .and_then(|model| model.id.as_deref())
        .filter(|id| !id.is_empty());
    match id {
        Some(id) => format!("id:{id}"),
        None => format!("name:{display_name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits_from(json: &str) -> Vec<ClaudeLimit> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn weekly_scoped_entry_produces_labeled_window() {
        let limits = limits_from(
            r#"[
                {"kind":"weekly_scoped","group":"weekly","percent":42.0,
                 "resets_at":"2026-07-07T12:00:00+00:00",
                 "scope":{"model":{"id":"fable-5","display_name":"Fable"}},
                 "is_active":true}
            ]"#,
        );
        let windows = scoped_weekly_windows(&limits).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "Fable");
        assert_eq!(windows[0].used_percent, 42.0);
        assert_eq!(windows[0].window_seconds, Some(SEVEN_DAY_SECONDS));
        assert_eq!(
            windows[0].reset_at.unwrap().to_rfc3339(),
            "2026-07-07T12:00:00+00:00"
        );
    }

    #[test]
    fn inactive_entry_still_produces_window() {
        let limits = limits_from(
            r#"[
                {"kind":"weekly_scoped","group":"weekly","percent":10.0,
                 "scope":{"model":{"display_name":"Fable"}},
                 "is_active":false}
            ]"#,
        );
        let windows = scoped_weekly_windows(&limits).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "Fable");
    }

    #[test]
    fn entry_without_display_name_is_ignored() {
        let limits = limits_from(
            r#"[
                {"kind":"weekly_scoped","group":"weekly","percent":10.0,"scope":null},
                {"kind":"weekly_scoped","group":"weekly","percent":10.0,"scope":{"model":null}},
                {"kind":"weekly_scoped","group":"weekly","percent":10.0,"scope":{"model":{"display_name":""}}}
            ]"#,
        );
        assert!(scoped_weekly_windows(&limits).unwrap().is_empty());
    }

    #[test]
    fn entry_without_percent_is_skipped() {
        let limits = limits_from(
            r#"[
                {"kind":"weekly_scoped","group":"weekly","scope":{"model":{"display_name":"Fable"}}},
                {"kind":"weekly_scoped","group":"weekly","percent":25.0,"scope":{"model":{"display_name":"Fable"}}}
            ]"#,
        );
        let windows = scoped_weekly_windows(&limits).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].used_percent, 25.0);
    }

    #[test]
    fn non_weekly_scoped_entries_are_ignored() {
        let limits = limits_from(
            r#"[
                {"kind":"five_hour","group":"weekly","percent":10.0,"scope":{"model":{"display_name":"Fable"}}},
                {"kind":"weekly_scoped","group":"session","percent":10.0,"scope":{"model":{"display_name":"Fable"}}}
            ]"#,
        );
        assert!(scoped_weekly_windows(&limits).unwrap().is_empty());
    }

    #[test]
    fn duplicate_model_id_collapses_to_single_window() {
        let limits = limits_from(
            r#"[
                {"kind":"weekly_scoped","group":"weekly","percent":10.0,"scope":{"model":{"id":"fable-5","display_name":"Fable"}}},
                {"kind":"weekly_scoped","group":"weekly","percent":90.0,"scope":{"model":{"id":"fable-5","display_name":"Fable Redux"}}}
            ]"#,
        );
        let windows = scoped_weekly_windows(&limits).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "Fable");
        assert_eq!(windows[0].used_percent, 10.0);
    }

    #[test]
    fn duplicate_display_name_without_id_collapses() {
        let limits = limits_from(
            r#"[
                {"kind":"weekly_scoped","group":"weekly","percent":10.0,"scope":{"model":{"display_name":"Fable"}}},
                {"kind":"weekly_scoped","group":"weekly","percent":90.0,"scope":{"model":{"display_name":"Fable"}}}
            ]"#,
        );
        let windows = scoped_weekly_windows(&limits).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].used_percent, 10.0);
    }

    #[test]
    fn empty_limits_produce_no_windows() {
        assert!(scoped_weekly_windows(&[]).unwrap().is_empty());
    }
}
