---
status: done
type: AFK
blocked_by:
  - 002-device-flow-login
---

# Free-tier usage fetch + parse + render

## What to build

Implement the Copilot usage fetch path and the Free-tier branch of the parser. After this slice lands, a real Free Copilot account shows actual chat + completions usage in both the panel and the popup.

Paid tier parsing is out of scope here — it lands in #004.

See [`docs/copilot-provider.md` §5 and §6](../../../docs/copilot-provider.md) for the request shape and parsing rules.

## Acceptance criteria

- [x] `GET https://api.github.com/copilot_internal/user` with the documented headers, sourced from a single named-constants module (`src/providers/copilot/headers.rs`). Values mirror `opencode-mystatus` (Jan 2026): `Editor-Version: vscode/1.107.0`, `Editor-Plugin-Version: copilot-chat/0.35.0`, `User-Agent: GitHubCopilotChat/0.35.0`, `X-Github-Api-Version: 2025-04-01`.
- [x] No preflight token check (token has no expiry).
- [x] Copilot uses the global refresh interval; no provider-specific override.
- [x] Parser lives in one flat file (`src/providers/copilot/parse.rs`), no internal trait/enum layering.
- [x] Free schema branch (when `access_type_sku == "free_limited_copilot"`):
  - Reads `monthly_quotas.chat` and `monthly_quotas.completions` (entitlements).
  - Reads `limited_user_quotas.chat` and `limited_user_quotas.completions` (remaining).
  - Reads `limited_user_reset_date` (used as `reset_at` for both windows).
  - Produces two `UsageWindow` entries: `chat`, then `completions`.
  - `UsageHeadline` points at `completions` (index 1).
  - Plan badge: **Free**.
  - `UsageSnapshot.identity.email` left empty; `login` used as the display name slot.
- [x] Unit tests cover the Free parser against `fixtures/copilot/copilot_user_response.json` and any synthetic edge cases (missing fields, varying entitlement numbers).
- [x] Parser tolerates extra fields without failing (top-level `analytics_tracking_id`, `assigned_date`, etc.).
- [x] Unknown response shape (no `quota_snapshots`, no `monthly_quotas`) returns an actionable error like "Unrecognized Copilot response — YapCap may need updating." Preserves stale snapshot. Account `health = Error`, `auth_state` unchanged.
- [ ] Live capture: adding a real Free Copilot account renders chat and completions windows in the popup; panel shows two bars; headline percent matches `completions`.
- [x] `cargo fmt`, `just check`, `cargo test` all pass.

## Blocked by

- `002-device-flow-login`

## Notes

2026-05-18T11:03:35+02:00 — Implemented the Free-tier fetch/parser/render data path. Local HTTP tests verify the documented Copilot usage request and headers; parser tests cover the captured Free fixture, varying entitlements, missing fields, clamping, and unknown shapes. Manual live capture was not run in this AFK environment because no real GitHub Copilot account was available.
