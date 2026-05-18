---
status: done
type: AFK
blocked_by:
  - 003-free-tier-fetch-and-render
---

# Paid-tier parsing + plan badge mapping

## What to build

Add the paid-tier branch to the Copilot parser (`quota_snapshots.premium_interactions`) and the full plan-badge mapping. Tested against the existing Pro+ and Business fixtures.

Panel layout for paid Copilot ships in #005; until then paid accounts render acceptably with the existing two-bar panel (second bar empty/0%).

See [`docs/copilot-provider.md` §6 and §7](../../../docs/copilot-provider.md).

## Acceptance criteria

- [x] Paid schema branch (when `quota_snapshots` is present in the response):
  - Reads `quota_snapshots.premium_interactions.entitlement`, `.remaining` (integer), `.percent_remaining` (float).
  - Produces a single `UsageWindow` for `premium_interactions`, using `quota_reset_date` as `reset_at`.
  - `UsageHeadline` points at this window.
  - Skips `chat` and `completions` from `quota_snapshots` because they are `unlimited: true` on paid tiers.
  - `quota_remaining` (fractional) is informational only and not surfaced.
- [x] Plan badge mapping table:
  - `free_limited_copilot` → **Free**
  - `plus_monthly_subscriber_quota` → **Pro+**
  - `copilot_standalone_seat_quota` → **Business**
  - Unknown SKU with `quota_snapshots`:
    - If `premium_interactions.entitlement == 300` → **Pro**
    - If `premium_interactions.entitlement == 1500` → **Pro+**
    - Otherwise → **Plan**
  - Anything else → **Plan**
- [x] Unit tests cover the paid parser against `fixtures/copilot/copilot_user_pro_plus_response.json` and `fixtures/copilot/copilot_user_business_response.json`. Both produce a single `premium_interactions` window with correct numbers and the right plan badge.
- [x] Unit tests cover the entitlement-disambiguation fallback (synthetic SKU `"unknown_paid_sku"` with `entitlement: 300` → Pro badge; same SKU with `entitlement: 1500` → Pro+ badge; `entitlement: 999` → "Plan" badge).
- [x] Parser tolerates extra per-quota fields (`quota_id`, `timestamp_utc`, `overage_permitted`, `overage_count`) — the latter two are read but not yet rendered (rendering ships in #006).
- [x] `cargo fmt`, `just check`, `cargo test` all pass.

## Blocked by

- `003-free-tier-fetch-and-render`

## Notes

2026-05-18T11:09:29+02:00 — Implemented paid Copilot parser support for `quota_snapshots.premium_interactions`, single-window headline output, paid plan badge mapping, and entitlement-based fallback labels. Added fixture-backed Pro+ and Business tests plus synthetic unknown-SKU fallback coverage.
