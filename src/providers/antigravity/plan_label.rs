// SPDX-License-Identifier: MPL-2.0

pub const PLAN_FREE: &str = "Free";
pub const PLAN_PRO: &str = "Pro";
pub const PLAN_ULTRA: &str = "Ultra";
pub const PLAN_FALLBACK: &str = "Plan";

#[must_use]
pub fn plan_label(tier_id: &str) -> &'static str {
    match tier_id {
        "free-tier" => PLAN_FREE,
        "standard-tier" | "g1-pro-tier" => PLAN_PRO,
        "g1-ultra-tier" => PLAN_ULTRA,
        _ => PLAN_FALLBACK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_documented_tiers() {
        assert_eq!(plan_label("free-tier"), PLAN_FREE);
        assert_eq!(plan_label("standard-tier"), PLAN_PRO);
        assert_eq!(plan_label("g1-pro-tier"), PLAN_PRO);
        assert_eq!(plan_label("g1-ultra-tier"), PLAN_ULTRA);
        assert_eq!(plan_label("legacy-tier"), PLAN_FALLBACK);
        assert_eq!(plan_label(""), PLAN_FALLBACK);
    }
}
