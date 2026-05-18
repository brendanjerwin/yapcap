---
status: done
type: AFK
blocked_by:
  - 003-free-tier-fetch-and-render
  - 004-paid-tier-parsing
  - 005-single-bar-panel-layout
  - 006-overage-rendering
---

# `YAPCAP_DEMO` Copilot seed

## What to build

Extend the `YAPCAP_DEMO` synthetic-data seeding (per `docs/spec.md` §7.2) to include two managed Copilot accounts. Exercises multi-account display, mixed bar-count panel, plan badges, and overage rendering — all the visual features that aren't reproducible against real GitHub accounts.

See [`docs/copilot-provider.md` §12](../../../docs/copilot-provider.md).

## Acceptance criteria

- [ ] When `YAPCAP_DEMO=1` (or other truthy values per the existing detection logic), two Copilot accounts are seeded into the synthetic `AppState`:
  - **`casey-free`** — Free tier
    - `chat`: 350/500 remaining (~30% used)
    - `completions`: 60/300 remaining (~80% used) — headline
    - `limited_user_reset_date`: ~2 weeks from `now`
    - Plan badge: **Free**
  - **`morgan-pro`** — Pro+ tier, in overage
    - `premium_interactions.entitlement`: 1500
    - `premium_interactions.remaining`: 0
    - `premium_interactions.percent_remaining`: 0
    - `premium_interactions.overage_count`: 42
    - `quota_reset_date`: ~2 weeks from `now`
    - Plan badge: **Pro+**
- [ ] Both accounts selected; `show_all_accounts: true` for Copilot.
- [ ] `provider_visibility_mode = user_managed` (matches existing demo behavior).
- [ ] `copilot_enabled = true`.
- [ ] Demo refresh is a no-op (matches existing demo behavior for other providers).
- [ ] Snapshot-cache writes are skipped (matches existing demo behavior).
- [ ] Visual validation: `YAPCAP_DEMO=1 cargo run` shows the popup with both Copilot accounts side-by-side. Panel renders 2 bars (Free) next to 1 centered bar (Pro+). Popup shows the `+42 over plan` text under `morgan-pro`'s premium bar.
- [ ] `cargo fmt`, `just check`, `cargo test` all pass.

## Blocked by

- `003-free-tier-fetch-and-render`
- `004-paid-tier-parsing`
- `005-single-bar-panel-layout`
- `006-overage-rendering`

## Completion note

Implemented the `YAPCAP_DEMO` Copilot seed with two selected managed accounts: `casey-free` on Free with chat/completions usage and `morgan-pro` on Pro+ with a single premium window and `+42 over plan`.
